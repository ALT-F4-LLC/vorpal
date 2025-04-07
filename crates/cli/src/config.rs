use crate::build;
use anyhow::{anyhow, bail, Result};
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use port_selector::random_free_port;
use std::{collections::HashMap, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process,
    process::Child,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::{transport::Channel, Code::NotFound};
use tracing::{info, warn};
use vorpal_schema::{
    archive::v0::archive_service_client::ArchiveServiceClient,
    artifact::v0::{artifact_service_client::ArtifactServiceClient, Artifact, ArtifactRequest},
    worker::v0::worker_service_client::WorkerServiceClient,
};

use vorpal_store::paths::get_store_path;

pub async fn fetch_artifacts(
    artifact: &Artifact,
    artifact_map: &mut HashMap<String, Artifact>,
    client_config: &mut ArtifactServiceClient<Channel>,
    client_registry: &mut ArtifactServiceClient<Channel>,
) -> Result<()> {
    for step in artifact.steps.iter() {
        for digest in step.artifacts.iter() {
            if artifact_map.contains_key(digest) {
                continue;
            }

            let request = ArtifactRequest {
                digest: digest.to_string(),
            };

            let response = match client_config.get_artifact(request).await {
                Ok(res) => res,
                Err(error) => {
                    if error.code() != NotFound {
                        bail!("config get artifact error: {:?}", error);
                    }

                    let registry_request = ArtifactRequest {
                        digest: digest.to_string(),
                    };

                    match client_registry.get_artifact(registry_request).await {
                        Ok(res) => res,
                        Err(status) => {
                            if status.code() != NotFound {
                                bail!("registry get artifact error: {:?}", status);
                            }

                            bail!("artifact not found in registry: {}", digest);
                        }
                    }
                }
            };

            let artifact = response.into_inner();

            artifact_map.insert(digest.to_string(), artifact.clone());

            Box::pin(fetch_artifacts(
                &artifact,
                artifact_map,
                client_config,
                client_registry,
            ))
            .await?
        }
    }

    Ok(())
}

pub async fn get_order(build_artifact: &HashMap<String, Artifact>) -> Result<Vec<String>> {
    let mut artifact_graph = DiGraphMap::<&String, Artifact>::new();

    for (artifact_hash, artifact) in build_artifact.iter() {
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
    file: String,
    registry: String,
    target: String,
) -> Result<(Child, ArtifactServiceClient<Channel>)> {
    let port = random_free_port().ok_or_else(|| anyhow!("failed to find free port"))?;

    let mut command = process::Command::new(file.clone());

    command.args([
        "start",
        "--port",
        &port.to_string(),
        "--registry",
        &registry,
        "--target",
        &target,
    ]);

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

    while let Some(line) = stdio_merged.next().await {
        let line = line.map_err(|err| anyhow!("failed to read line: {:?}", err))?;

        if !line.contains("artifact service:") {
            info!("{}", line);
        }

        if line.contains("artifact service:") {
            break;
        }
    }

    let config_host = format!("http://localhost:{:?}", port);

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
    artifact_selected: Option<&Artifact>,
    artifact_config: HashMap<String, Artifact>,
    client_archive: &mut ArchiveServiceClient<Channel>,
    client_artifact: &mut ArtifactServiceClient<Channel>,
    client_worker: &mut WorkerServiceClient<Channel>,
) -> Result<()> {
    let artifact_order = get_order(&artifact_config).await?;
    let mut artifact_complete = HashMap::<String, Artifact>::new();

    for artifact_hash in artifact_order {
        match artifact_config.get(&artifact_hash) {
            None => bail!("artifact 'config' not found: {}", artifact_hash),

            Some(artifact) => {
                for step in artifact.steps.iter() {
                    for hash in step.artifacts.iter() {
                        if !artifact_complete.contains_key(hash) {
                            bail!("artifact 'build' not found: {}", hash);
                        }
                    }
                }

                build(artifact, &artifact_hash, client_archive, client_worker).await?;

                match client_artifact.store_artifact(artifact.clone()).await {
                    Err(status) => {
                        bail!("registry put error: {:?}", status);
                    }

                    Ok(_) => {}
                }

                artifact_complete.insert(artifact_hash.to_string(), artifact.clone());

                if let Some(artifact_selected) = artifact_selected {
                    if artifact_selected.name == artifact.name {
                        println!("{}", get_store_path(&artifact_hash).display());
                    }
                }
            }
        }
    }

    Ok(())
}
