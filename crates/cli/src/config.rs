use crate::build;
use anyhow::{anyhow, bail, Result};
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use port_selector::random_free_port;
use std::{collections::HashMap, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process,
    process::Child,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::transport::Channel;
use tracing::{info, warn};
use vorpal_schema::{
    archive::v0::archive_service_client::ArchiveServiceClient,
    artifact::v0::{artifact_service_client::ArtifactServiceClient, Artifact},
    worker::v0::worker_service_client::WorkerServiceClient,
};
use vorpal_store::paths::get_store_path;

pub async fn get_artifacts(
    artifact: &Artifact,
    artifact_digest: &str,
    build_store: &mut HashMap<String, Artifact>,
    config_store: &HashMap<String, Artifact>,
) -> Result<()> {
    if !build_store.contains_key(artifact_digest) {
        build_store.insert(artifact_digest.to_string(), artifact.clone());
    }

    for step in artifact.steps.iter() {
        for artifact_digest in step.artifacts.iter() {
            if build_store.contains_key(artifact_digest) {
                continue;
            }

            let artifact = config_store
                .get(artifact_digest)
                .ok_or_else(|| anyhow!("artifact 'config' not found: {}", artifact_digest))?;

            build_store.insert(artifact_digest.to_string(), artifact.clone());

            Box::pin(get_artifacts(
                artifact,
                artifact_digest,
                build_store,
                config_store,
            ))
            .await?
        }
    }

    Ok(())
}

pub async fn get_order(config_artifact: &HashMap<String, Artifact>) -> Result<Vec<String>> {
    let mut artifact_graph = DiGraphMap::<&String, Artifact>::new();

    for (artifact_hash, artifact) in config_artifact.iter() {
        artifact_graph.add_node(artifact_hash);

        for step in artifact.steps.iter() {
            for step_artifact_hash in step.artifacts.iter() {
                artifact_graph.add_edge(step_artifact_hash, artifact_hash, artifact.clone());
            }
        }
    }

    let build_order = match toposort(&artifact_graph, None) {
        Err(err) => bail!("{:?}", err),
        Ok(order) => order,
    };

    let build_order: Vec<String> = build_order.into_iter().cloned().collect();

    Ok(build_order)
}

pub async fn start(
    agent: String,
    artifact: String,
    artifact_context: PathBuf,
    file: String,
    registry: String,
    system: String,
    variable: Vec<String>,
) -> Result<(Child, ArtifactServiceClient<Channel>)> {
    let command_artifact_context = artifact_context.display().to_string();
    let command_port = random_free_port().ok_or_else(|| anyhow!("failed to find free port"))?;
    let command_port = command_port.to_string();

    let mut command = process::Command::new(file.clone());

    let command_arguments = vec![
        "start",
        "--agent",
        &agent,
        "--artifact",
        &artifact,
        "--artifact-context",
        &command_artifact_context,
        "--port",
        &command_port,
        "--registry",
        &registry,
        "--system",
        &system,
    ];

    command.args(command_arguments);

    for var in variable.iter() {
        command.arg("--variable").arg(var);
    }

    let mut config_process = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| anyhow!("failed to start config server"))?;

    let stdout = config_process.stdout.take().unwrap();
    let stderr = config_process.stderr.take().unwrap();

    let stdout = LinesStream::new(BufReader::new(stdout).lines());
    let stderr = LinesStream::new(BufReader::new(stderr).lines());

    let mut stdio_merged = StreamExt::merge(stdout, stderr);

    loop {
        match stdio_merged.next().await {
            Some(Ok(line)) => {
                if line.contains("artifact service:") {
                    break;
                }

                if line.starts_with("Error: ") {
                    let _ = config_process
                        .kill()
                        .await
                        .map_err(|_| anyhow!("failed to kill config server"));

                    bail!("{}", line.replace("Error: ", ""));
                } else {
                    info!("{}", line);
                }
            }

            Some(Err(err)) => {
                let _ = config_process
                    .kill()
                    .await
                    .map_err(|_| anyhow!("failed to kill config server"));

                bail!("failed to read line: {:?}", err);
            }

            None => break,
        }
    }

    let config_host = format!("http://localhost:{}", command_port);

    let mut attempts = 0;
    let max_attempts = 3;
    let max_wait_time = Duration::from_millis(500);

    let config_client = loop {
        attempts += 1;

        match ArtifactServiceClient::connect(config_host.clone()).await {
            Ok(srv) => break srv,
            Err(e) => {
                if attempts >= max_attempts {
                    let _ = config_process
                        .kill()
                        .await
                        .map_err(|_| anyhow!("failed to kill config server"));

                    bail!("failed to connect after {} attempts: {}", max_attempts, e);
                }

                warn!(
                    "config connection {}/{} failed, retry in {} ms...",
                    attempts,
                    max_attempts,
                    max_wait_time.as_millis()
                );

                tokio::time::sleep(max_wait_time).await;
            }
        }
    };

    Ok((config_process, config_client))
}

pub async fn build_artifacts(
    artifact_path: bool,
    artifact_selected: Option<&Artifact>,
    build_store: HashMap<String, Artifact>,
    client_archive: &mut ArchiveServiceClient<Channel>,
    client_worker: &mut WorkerServiceClient<Channel>,
) -> Result<()> {
    let artifact_order = get_order(&build_store).await?;
    let mut build_complete = HashMap::<String, Artifact>::new();

    for artifact_digest in artifact_order {
        match build_store.get(&artifact_digest) {
            None => bail!("artifact 'config' not found: {}", artifact_digest),

            Some(artifact) => {
                for step in artifact.steps.iter() {
                    for hash in step.artifacts.iter() {
                        if !build_complete.contains_key(hash) {
                            bail!("artifact 'build' not found: {}", hash);
                        }
                    }
                }

                build(artifact, &artifact_digest, client_archive, client_worker).await?;

                build_complete.insert(artifact_digest.to_string(), artifact.clone());

                if let Some(artifact_selected) = artifact_selected {
                    if artifact_selected.name == artifact.name {
                        let mut output = artifact_digest.clone();

                        if artifact_path {
                            output = get_store_path(&artifact_digest).display().to_string();
                        }

                        println!("{}", output);
                    }
                }
            }
        }
    }

    Ok(())
}
