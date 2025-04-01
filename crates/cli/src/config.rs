use crate::{build, build_source, get_prefix};
use anyhow::{anyhow, bail, Result};
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use port_selector::random_free_port;
use std::{
    collections::HashMap,
    env::var,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process,
    process::Child,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::{transport::Channel, Code};
use tracing::info;
use vorpal_schema::{
    artifact::v0::artifact_service_client::ArtifactServiceClient,
    config::v0::{
        config_service_client::ConfigServiceClient,
        ConfigArtifact, ConfigArtifactRequest, ConfigArtifactSystem,
        ConfigArtifactSystem::{Aarch64Linux, X8664Linux},
    },
    registry::v0::registry_service_client::RegistryServiceClient,
};
use vorpal_sdk::{
    artifact::{language::rust, linux_vorpal, protoc, rust_toolchain},
    context::ConfigContext,
};
use vorpal_store::paths::get_store_path;

fn fetch_artifacts_context(
    context: &mut ConfigContext,
    config: ConfigArtifact,
    pending: &mut HashMap<String, ConfigArtifact>,
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

pub async fn get_order(build_artifact: &HashMap<String, ConfigArtifact>) -> Result<Vec<String>> {
    let mut artifact_graph = DiGraphMap::<&String, ConfigArtifact>::new();

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
    language: String,
    registry: &String,
    rust_bin: &Option<String>,
    rust_path: &String,
    service: &String,
    target: ConfigArtifactSystem,
) -> Result<PathBuf> {
    match language.as_str() {
        "rust" => {
            info!("{} language: {}", get_prefix("config"), language);

            if rust_bin.is_none() {
                bail!("no `--rust-bin` specified");
            }

            info!(
                "{} rust-bin: {}",
                get_prefix("config"),
                rust_bin.as_ref().unwrap()
            );

            info!("{} rust-path: {}", get_prefix("config"), rust_path);

            // Setup context

            let mut context = ConfigContext::new(0, registry.clone(), target);

            // Setup artifacts

            let mut artifact_pending = HashMap::new();

            if target == Aarch64Linux || target == X8664Linux {
                let linux_vorpal = linux_vorpal::build(&mut context).await?;
                let linux_vorpal_config = context.get_artifact(&linux_vorpal);

                if linux_vorpal_config.is_none() {
                    bail!("artifact 'linux-vorpal' not found");
                }

                artifact_pending.insert(linux_vorpal, linux_vorpal_config.clone().unwrap());

                fetch_artifacts_context(
                    &mut context,
                    linux_vorpal_config.unwrap(),
                    &mut artifact_pending,
                )?;
            }

            let protoc = protoc::build(&mut context).await?;
            let protoc_config = context.get_artifact(&protoc);

            if protoc_config.is_none() {
                bail!("artifact 'protoc' not found");
            }

            artifact_pending.insert(protoc.clone(), protoc_config.clone().unwrap());

            fetch_artifacts_context(&mut context, protoc_config.unwrap(), &mut artifact_pending)?;

            let rust_toolchain = rust_toolchain::build(&mut context).await?;
            let rust_toolchain_config = context.get_artifact(&rust_toolchain);

            if rust_toolchain_config.is_none() {
                bail!("artifact 'rust-toolchain' not found");
            }

            artifact_pending.insert(
                rust_toolchain.clone(),
                rust_toolchain_config.clone().unwrap(),
            );

            fetch_artifacts_context(
                &mut context,
                rust_toolchain_config.unwrap(),
                &mut artifact_pending,
            )?;

            // Build artifacts

            let mut client_artifact = ArtifactServiceClient::connect(service.to_owned())
                .await
                .expect("failed to connect to artifact");

            let mut client_registry = RegistryServiceClient::connect(registry.to_owned())
                .await
                .expect("failed to connect to registry");

            build_artifacts(
                None,
                artifact_pending,
                &mut client_artifact,
                &mut client_registry,
            )
            .await?;

            // Get protoc

            let protoc_path = get_store_path(&protoc);

            if !protoc_path.exists() {
                bail!("protoc not found: {}", protoc_path.display());
            }

            // Get rust-toolchain

            let rust_toolchain_path = get_store_path(&rust_toolchain);

            if !rust_toolchain_path.exists() {
                bail!(
                    "rust-toolchain not found: {}",
                    rust_toolchain_path.display()
                );
            }

            let rust_toolchain_target = rust::toolchain_target(target)?;
            let rust_toolchain_version = rust::toolchain_version();

            let rust_toolchain_bin_path = Path::new(&format!(
                "{}/toolchains/{}-{}/bin",
                rust_toolchain_path.display(),
                rust_toolchain_version,
                rust_toolchain_target
            ))
            .to_path_buf();

            let rust_toolchain_cargo_path =
                Path::new(&format!("{}/cargo", rust_toolchain_bin_path.display())).to_path_buf();

            if !rust_toolchain_cargo_path.exists() {
                bail!("cargo not found: {}", rust_toolchain_cargo_path.display());
            }

            // Build configuration with toolchain

            let mut command = process::Command::new(rust_toolchain_cargo_path.clone());

            // Setup environment variables

            let env_path = format!(
                "{}:{}/bin:{}",
                rust_toolchain_bin_path.display(),
                protoc_path.display(),
                var("PATH").unwrap_or_default()
            );

            command.env("PATH", env_path.as_str());

            info!("{} path: {}", get_prefix("config"), env_path);

            command.env("RUSTUP_HOME", rust_toolchain_path.display().to_string());

            command.env(
                "RUSTUP_TOOLCHAIN",
                format!("{}-{}", rust_toolchain_version, rust_toolchain_target),
            );

            // Setup command

            let config_bin = rust_bin.as_ref().unwrap();

            command.args(["build", "--bin", config_bin]);

            info!(
                "{} command: {} build --bin {}",
                get_prefix("config"),
                rust_toolchain_cargo_path.display(),
                config_bin
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

            let config_file_path = format!("{}/target/debug/{}", rust_path, config_bin);

            info!("{} path: {}", get_prefix("config"), config_file_path);

            Ok(Path::new(&config_file_path).to_path_buf())
        }

        _ => bail!("unsupported language: {}", language),
    }
}

pub async fn start(
    file: String,
    registry: String,
) -> Result<(Child, ConfigServiceClient<Channel>)> {
    let port = random_free_port().ok_or_else(|| anyhow!("failed to find free port"))?;

    let mut command = process::Command::new(file);

    command.args([
        "start",
        "--port",
        &port.to_string(),
        "--registry",
        &registry,
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

    let host = format!("http://localhost:{:?}", port);

    while let Some(line) = stdio_merged.next().await {
        let line = line.map_err(|err| anyhow!("failed to read line: {:?}", err))?;

        if !line.contains("Config listening") {
            info!("{}", line);
        }

        if line.contains("Config listening") {
            break;
        }
    }

    let config_service = match ConfigServiceClient::connect(host).await {
        Ok(srv) => srv,
        Err(e) => {
            let _ = config_process
                .kill()
                .await
                .map_err(|_| anyhow!("failed to kill config server"));

            bail!("failed to connect to config server: {}", e);
        }
    };

    Ok((config_process, config_service))
}

pub async fn fetch_artifacts(
    artifact: &ConfigArtifact,
    artifact_map: &mut HashMap<String, ConfigArtifact>,
    client_config: &mut ConfigServiceClient<Channel>,
    client_registry: &mut RegistryServiceClient<Channel>,
) -> Result<()> {
    for step in artifact.steps.iter() {
        for step_artifact_hash in step.artifacts.iter() {
            if artifact_map.contains_key(step_artifact_hash) {
                continue;
            }

            let request = ConfigArtifactRequest {
                hash: step_artifact_hash.to_string(),
            };

            let response = match client_config.get_config_artifact(request).await {
                Ok(res) => res,
                Err(error) => {
                    if error.code() != Code::NotFound {
                        bail!("artifact not found in config: {}", step_artifact_hash);
                    }

                    let registry_request = ConfigArtifactRequest {
                        hash: step_artifact_hash.to_string(),
                    };

                    match client_registry.get_config_artifact(registry_request).await {
                        Ok(res) => res,
                        Err(status) => {
                            if status.code() != Code::NotFound {
                                bail!("registry get artifact error: {:?}", status);
                            }

                            bail!("artifact not found in registry: {}", step_artifact_hash);
                        }
                    }
                }
            };

            let artifact = response.into_inner();

            artifact_map.insert(step_artifact_hash.to_string(), artifact.clone());

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

pub async fn build_artifacts(
    artifact_selected: Option<&ConfigArtifact>,
    artifact_config: HashMap<String, ConfigArtifact>,
    client_artifact: &mut ArtifactServiceClient<Channel>,
    client_registry: &mut RegistryServiceClient<Channel>,
) -> Result<()> {
    let artifact_order = get_order(&artifact_config).await?;
    let mut artifact_complete = HashMap::<String, ConfigArtifact>::new();

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

                let mut artifact_source_hash = HashMap::<String, String>::new();

                for source in artifact.sources.iter() {
                    let hash = build_source(&artifact.name, source, client_registry).await?;

                    artifact_source_hash.insert(source.name.clone(), hash);
                }

                build(
                    artifact,
                    &artifact_hash,
                    &artifact_source_hash,
                    client_artifact,
                    client_registry,
                )
                .await?;

                match client_registry.put_config_artifact(artifact.clone()).await {
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
