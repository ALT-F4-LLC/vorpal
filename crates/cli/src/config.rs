use crate::{build, get_prefix};
use anyhow::{anyhow, bail, Result};
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use port_selector::random_free_port;
use std::{
    collections::HashMap,
    env::var,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process,
    process::Child,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::{transport::Channel, Code::NotFound};
use tracing::info;
use vorpal_schema::{
    archive::v0::archive_service_client::ArchiveServiceClient,
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient,
        Artifact, ArtifactRequest, ArtifactSystem,
        ArtifactSystem::{Aarch64Linux, X8664Linux},
    },
    worker::v0::worker_service_client::WorkerServiceClient,
};
use vorpal_sdk::{
    artifact::{language::rust, linux_vorpal, protoc, rust_toolchain},
    context::ConfigContext,
};
use vorpal_store::paths::get_store_path;

fn fetch_artifacts_context(
    context: &mut ConfigContext,
    config: Artifact,
    pending: &mut HashMap<String, Artifact>,
) -> Result<()> {
    for step in config.steps.iter() {
        for hash in step.artifacts.iter() {
            let step_artifact = context.get_artifact(hash);

            if step_artifact.is_none() {
                bail!("rust 'artifact' not found: {}", hash);
            }

            let step_artifact = step_artifact.unwrap();

            pending.insert(hash.to_string(), step_artifact.clone());

            fetch_artifacts_context(context, step_artifact.clone(), pending)?
        }
    }

    Ok(())
}

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

pub async fn get_path(
    agent: &String,
    config_path: &String,
    language: &String,
    registry: &String,
    registry_archive: &mut ArchiveServiceClient<Channel>,
    registry_artifact: &mut ArtifactServiceClient<Channel>,
    rust_bin: &String,
    target: &ArtifactSystem,
    worker: &mut WorkerServiceClient<Channel>,
) -> Result<PathBuf> {
    info!("{} language: {}", get_prefix("config"), language);

    if config_path.is_empty() {
        bail!("no `--go-path` specified");
    }

    info!("{} path: {}", get_prefix("config"), config_path);

    // Setup context

    let mut context = ConfigContext::new(agent.clone(), 0, registry.clone(), *target);

    // Setup artifacts

    let mut toolkit_artifact = HashMap::new();

    if *target == Aarch64Linux || *target == X8664Linux {
        let linux_vorpal = linux_vorpal::build(&mut context).await?;
        let linux_vorpal_config = context.get_artifact(&linux_vorpal);

        if linux_vorpal_config.is_none() {
            bail!("toolkit artifact not found: linux-vorpal");
        }

        toolkit_artifact.insert(linux_vorpal, linux_vorpal_config.clone().unwrap());

        fetch_artifacts_context(
            &mut context,
            linux_vorpal_config.unwrap(),
            &mut toolkit_artifact,
        )?;
    }

    // Setup default artifacts

    let protoc_hash = protoc::build(&mut context).await?;
    let protoc = context.get_artifact(&protoc_hash);

    if protoc.is_none() {
        bail!("toolkit artifact not found: protoc");
    }

    toolkit_artifact.insert(protoc_hash.clone(), protoc.clone().unwrap());

    fetch_artifacts_context(&mut context, protoc.unwrap(), &mut toolkit_artifact)?;

    // Setup clients

    let config_file = match language.as_str() {
        // "go" => Ok(()),
        "rust" => {
            info!("{} rust-bin: {}", get_prefix("config"), rust_bin);

            let rust_toolchain_hash = rust_toolchain::build(&mut context).await?;
            let rust_toolchain = context.get_artifact(&rust_toolchain_hash);

            if rust_toolchain.is_none() {
                bail!("artifact 'rust-toolchain' not found");
            }

            toolkit_artifact.insert(rust_toolchain_hash.clone(), rust_toolchain.clone().unwrap());

            fetch_artifacts_context(&mut context, rust_toolchain.unwrap(), &mut toolkit_artifact)?;

            // Build artifacts

            build_artifacts(
                None,
                toolkit_artifact,
                registry_archive,
                registry_artifact,
                worker,
            )
            .await?;

            // Get rust-toolchain

            let rust_toolchain_path = get_store_path(&rust_toolchain_hash);

            if !rust_toolchain_path.exists() {
                bail!(
                    "rust-toolchain not found: {}",
                    rust_toolchain_path.display()
                );
            }

            let rust_toolchain_target = rust::toolchain_target(*target)?;
            let rust_toolchain_version = rust::toolchain_version();
            let rust_toolchain_identifier =
                format!("{}-{}", rust_toolchain_version, rust_toolchain_target);

            let rust_toolchain_path = Path::new(&format!(
                "{}/toolchains/{}",
                rust_toolchain_path.display(),
                rust_toolchain_identifier
            ))
            .to_path_buf();

            let rust_toolchain_cargo_path =
                Path::new(&format!("{}/bin/cargo", rust_toolchain_path.display())).to_path_buf();

            if !rust_toolchain_cargo_path.exists() {
                bail!("cargo not found: {}", rust_toolchain_cargo_path.display());
            }

            // Setup command

            let mut command = process::Command::new(rust_toolchain_cargo_path.clone());

            // Setup command environment

            let protoc_path = get_store_path(&protoc_hash);

            if !protoc_path.exists() {
                bail!("protoc not found: {}", protoc_path.display());
            }

            let env_path = format!(
                "{}/bin:{}/bin:{}",
                rust_toolchain_path.display(),
                protoc_path.display(),
                var("PATH").unwrap_or_default()
            );

            command.env("PATH", env_path.as_str());
            command.env("RUSTUP_HOME", rust_toolchain_path.display().to_string());
            command.env("RUSTUP_TOOLCHAIN", rust_toolchain_identifier);

            // Setup command arguments

            command.args(["build", "--bin", rust_bin]);

            info!(
                "{} build: {} build --bin {}",
                get_prefix("config"),
                rust_toolchain_cargo_path.display(),
                rust_bin
            );

            let mut process = command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|_| anyhow!("failed to start config server"))?;

            let stdout = process.stdout.take().unwrap();
            let stderr = process.stderr.take().unwrap();

            let stdout = LinesStream::new(BufReader::new(stdout).lines());
            let stderr = LinesStream::new(BufReader::new(stderr).lines());

            let mut stdio_merged = StreamExt::merge(stdout, stderr);

            while let Some(line) = stdio_merged.next().await {
                let line = line.map_err(|err| anyhow!("failed to read line: {:?}", err))?;

                info!("{}", line);
            }

            format!("{}/target/debug/{}", config_path, rust_bin)
        }

        _ => bail!("unsupported language: {}", language),
    };

    info!("{} file: {}", get_prefix("config"), config_file);

    Ok(Path::new(&config_file).to_path_buf())
}

pub async fn start(
    file: String,
    registry: String,
) -> Result<(Child, ArtifactServiceClient<Channel>)> {
    let port = random_free_port().ok_or_else(|| anyhow!("failed to find free port"))?;

    let mut command = process::Command::new(file.clone());

    command.args([
        "start",
        "--port",
        &port.to_string(),
        "--registry",
        &registry,
    ]);

    info!(
        "{} start: {} start --port {} --registry {}",
        get_prefix("config"),
        file,
        port,
        registry
    );

    let mut config_process = command
        .spawn()
        .map_err(|_| anyhow!("failed to start config server"))?;

    info!(
        "{} process: {}",
        get_prefix("config"),
        config_process.id().unwrap()
    );

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

                info!(
                    "{} connection {}/{} failed, retry in {} ms...",
                    get_prefix("config"),
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

                    Ok(_) => {
                        info!("{} store: {}", get_prefix(&artifact.name), artifact_hash);
                    }
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
