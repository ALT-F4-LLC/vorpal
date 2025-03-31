use crate::artifact::{build, build_source};
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use console::style;
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use port_selector::random_free_port;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::{process, process::Child};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::transport::{Channel, Server};
use tracing::{info, warn, Level};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::FmtSubscriber;
use vorpal_registry::{RegistryBackend, RegistryServer, RegistryServerBackend};
use vorpal_schema::{
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient,
        artifact_service_server::ArtifactServiceServer,
    },
    config::v0::{
        config_service_client::ConfigServiceClient, ConfigArtifact, ConfigArtifactRequest,
        ConfigArtifactSystem, ConfigRequest,
    },
    registry::v0::{
        registry_service_client::RegistryServiceClient,
        registry_service_server::RegistryServiceServer,
    },
    system_default, system_default_str, system_from_str,
};
use vorpal_store::paths::{get_public_key_path, get_store_path};
use vorpal_worker::artifact::ArtifactServer;

mod artifact;
mod build;

pub fn get_prefix(name: &str) -> String {
    style(format!("{} |>", name)).bold().to_string()
}

#[derive(Subcommand)]
enum Command {
    Artifact {
        #[arg(default_value_t = false, long)]
        export: bool,

        #[arg(long)]
        name: String,

        #[clap(default_value = "http://localhost:23151", long)]
        service: String,

        #[arg(default_value_t = system_default_str(), long)]
        target: String,
    },

    #[clap(subcommand)]
    Keys(CommandKeys),

    Start {
        #[clap(default_value = "23151", long)]
        port: u16,

        #[arg(default_value = "artifact,registry", long)]
        services: String,

        #[arg(default_value = "local", long)]
        registry_backend: String,

        #[arg(long)]
        registry_backend_s3_bucket: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum CommandKeys {
    Generate {},
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(default_value = "Vorpal.toml", long, short)]
    config: String,

    #[arg(default_value = "rust", long)]
    language: String,

    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,

    #[clap(default_value = "http://localhost:23151", long, short)]
    registry: String,

    #[arg(default_value = "vorpal-config", long)]
    rust_bin: Option<String>,

    #[arg(default_value = ".", long)]
    rust_path: Option<String>,
}

async fn start_config(
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

async fn get_config_path(
    _artifact_target: ConfigArtifactSystem,
    language: String,
    _registry: String,
    rust_bin: Option<String>,
    rust_path: Option<String>,
    _service: String,
) -> Result<PathBuf> {
    match language.as_str() {
        "rust" => {
            if rust_bin.is_none() {
                bail!("no `--rust-bin` specified");
            }

            if rust_path.is_none() {
                bail!("no `--rust-path` specified");
            }

            // Setup context

            // let mut build_context = ConfigContext::new(0, registry.clone(), artifact_system);

            // Setup toolchain artifacts

            // let protoc = protoc::artifact(&mut build_context).await?;
            // let toolchain = rust::toolchain_artifact(&mut build_context, "vorpal").await?;

            // Setup build

            // let build_order = build::get_order(&build_context.artifact_id).await?;

            // let mut ready_artifacts = vec![];

            // for artifact_id in &build_order {
            //     match build_context.artifact_id.get(artifact_id) {
            //         None => bail!("build artifact not found: {}", artifact_id.name),
            //         Some(artifact) => {
            //             for artifact in &artifact.artifacts {
            //                 if !ready_artifacts.contains(&artifact) {
            //                     bail!("Artifact not found: {}", artifact.name);
            //                 }
            //             }
            //
            //             build(artifact, artifact_id, artifact_system, &registry, &service).await?;
            //
            //             ready_artifacts.push(artifact_id);
            //         }
            //     }
            // }

            // Get protoc

            // let protoc_path = Path::new(&format!(
            //     "{}/bin/protoc",
            //     get_artifact_path(&protoc.hash, &protoc.name).display()
            // ))
            // .to_path_buf();

            // if !protoc_path.exists() {
            //     bail!("protoc not found: {}", protoc_path.display());
            // }

            // Get toolchain

            // let toolchain_path = get_artifact_path(&toolchain.hash, &toolchain.name);

            // if !toolchain_path.exists() {
            //     bail!("config toolchain not found: {}", toolchain_path.display());
            // }

            // let toolchain_target = rust::get_rust_toolchain_target(artifact_system)?;
            // let toolchain_version = get_rust_toolchain_version();

            // let toolchain_bin_path = Path::new(&format!(
            //     "{}/toolchains/{}-{}/bin",
            //     toolchain_path.display(),
            //     toolchain_version,
            //     toolchain_target
            // ))
            // .to_path_buf();

            // let toolchain_cargo_path =
            //     Path::new(&format!("{}/cargo", toolchain_bin_path.display())).to_path_buf();

            // if !toolchain_cargo_path.exists() {
            //     bail!("cargo not found: {}", toolchain_cargo_path.display());
            // }

            // Build configuration with toolchain

            // let mut command = process::Command::new(toolchain_cargo_path);

            // Setup environment variables

            // command.env(
            //     "PATH",
            //     format!(
            //         "{}:{}/bin:{}",
            //         toolchain_bin_path.display(),
            //         get_artifact_path(&protoc.hash, &protoc.name).display(),
            //         var("PATH").unwrap_or_default()
            //     )
            //     .as_str(),
            // );

            // command.env("RUSTUP_HOME", toolchain_path.display().to_string());

            // command.env(
            //     "RUSTUP_TOOLCHAIN",
            //     format!("{}-{}", toolchain_version, toolchain_target),
            // );

            // Setup command

            let config_bin = rust_bin.as_ref().unwrap();

            // command.args(["build", "--bin", config_bin]);

            // let mut process = command
            //     .stdout(Stdio::piped())
            //     .stderr(Stdio::piped())
            //     .spawn()
            //     .map_err(|_| anyhow!("failed to start config server"))?;

            // let stdout = process.stdout.take().unwrap();
            // let stderr = process.stderr.take().unwrap();

            // let stdout = LinesStream::new(BufReader::new(stdout).lines());
            // let stderr = LinesStream::new(BufReader::new(stderr).lines());

            // let mut stdio_merged = StreamExt::merge(stdout, stderr);

            // while let Some(line) = stdio_merged.next().await {
            //     let line = line.map_err(|err| anyhow!("failed to read line: {:?}", err))?;
            //
            //     info!("{}", line);
            // }

            let config_file_path = format!(
                "{}/target/debug/{}",
                rust_path.as_ref().unwrap(),
                config_bin
            );

            info!("{} path: {}", get_prefix("config"), config_file_path);

            Ok(Path::new(&config_file_path).to_path_buf())
        }

        _ => bail!("unsupported language: {}", language),
    }
}

pub async fn fetch_artifacts(
    artifact: &ConfigArtifact,
    artifact_map: &mut HashMap<String, ConfigArtifact>,
    config_service: &mut ConfigServiceClient<Channel>,
) -> Result<()> {
    for step in artifact.steps.iter() {
        for step_artifact_hash in step.artifacts.iter() {
            if artifact_map.contains_key(step_artifact_hash) {
                continue;
            }

            let request = ConfigArtifactRequest {
                hash: step_artifact_hash.to_string(),
            };

            let response = match config_service.get_config_artifact(request).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to evaluate config: {}", error);
                }
            };

            let artifact = response.into_inner();

            artifact_map.insert(step_artifact_hash.to_string(), artifact.clone());

            Box::pin(fetch_artifacts(&artifact, artifact_map, config_service)).await?
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let Cli {
        command,
        config: _,
        language,
        level,
        registry,
        rust_bin,
        rust_path,
    } = cli;

    match &command {
        Command::Artifact {
            export: _artifact_export,
            name,
            service,
            target,
        } => {
            let stderr_writer = std::io::stderr.with_max_level(level);

            let mut subscriber = FmtSubscriber::builder()
                .with_max_level(level)
                .with_target(false)
                .with_writer(stderr_writer)
                .without_time();

            if [Level::DEBUG, Level::TRACE].contains(&level) {
                subscriber = subscriber.with_file(true).with_line_number(true);
            }

            let subscriber = subscriber.finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber");

            if service.is_empty() {
                bail!("no `--artifact-service` specified");
            }

            let artifact_target = system_from_str(target)?;

            if artifact_target == ConfigArtifactSystem::UnknownSystem {
                bail!("unsupported target: {}", artifact_target.as_str_name());
            }

            // Get config

            let config_path = get_config_path(
                artifact_target,
                language,
                registry.clone(),
                rust_bin.clone(),
                rust_path.clone(),
                service.clone(),
            )
            .await?;

            if !config_path.exists() {
                bail!("config file not found: {}", config_path.display());
            }

            let (mut config_process, mut config_service) =
                start_config(config_path.display().to_string(), registry.clone()).await?;

            let config_response = match config_service.get_config(ConfigRequest {}).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to evaluate config: {}", error);
                }
            };

            let config_response = config_response.into_inner();

            // Populate artifacts

            let mut config_response_artifact = HashMap::<String, ConfigArtifact>::new();

            for artifact_hash in config_response.artifacts.into_iter() {
                let artifact_request = ConfigArtifactRequest {
                    hash: artifact_hash.clone(),
                };

                let artifact_response =
                    match config_service.get_config_artifact(artifact_request).await {
                        Ok(res) => res,
                        Err(error) => {
                            bail!("failed to get artifact: {}", error);
                        }
                    };

                config_response_artifact.insert(artifact_hash, artifact_response.into_inner());
            }

            // Find artifact

            let (build_artifact_selected_hash, build_artifact_selected) = config_response_artifact
                .clone()
                .into_iter()
                .find(|(_, artifact)| artifact.name == *name)
                .ok_or_else(|| anyhow!("selected 'artifact' not found: {}", name))?;

            // Fetch artifacts

            let mut build_artifact = HashMap::<String, ConfigArtifact>::new();

            build_artifact.insert(
                build_artifact_selected_hash.to_string(),
                build_artifact_selected.clone(),
            );

            fetch_artifacts(
                &build_artifact_selected,
                &mut build_artifact,
                &mut config_service,
            )
            .await?;

            config_process.kill().await?;

            // Create build order

            let build_artifact_order = get_order(&build_artifact).await?;

            // Setup services

            let mut artifact_service = ArtifactServiceClient::connect(service.to_owned())
                .await
                .expect("failed to connect to artifact");

            let mut registry_service = RegistryServiceClient::connect(registry.to_owned())
                .await
                .expect("failed to connect to registry");

            // Build artifacts

            let mut build_artifact_ready = vec![];

            for artifact_hash in &build_artifact_order {
                match build_artifact.get(artifact_hash) {
                    None => bail!("artifact not found in build: {}", artifact_hash),
                    Some(artifact) => {
                        for step in artifact.steps.iter() {
                            for step_artifact_hash in step.artifacts.iter() {
                                if !build_artifact_ready.contains(step_artifact_hash) {
                                    bail!("artifact not ready: {}", step_artifact_hash);
                                }
                            }
                        }

                        for source in artifact.sources.iter() {
                            build_source(artifact, &mut registry_service, source).await?;
                        }

                        build(
                            artifact,
                            artifact_hash,
                            &mut artifact_service,
                            &mut registry_service,
                        )
                        .await?;

                        match registry_service.put_artifact(artifact.clone()).await {
                            Err(status) => {
                                bail!("registry put error: {:?}", status);
                            }

                            Ok(_) => {
                                info!("{} store: {}", get_prefix(&artifact.name), artifact_hash);
                            }
                        }

                        build_artifact_ready.push(artifact_hash.to_string());

                        if artifact.name == *name {
                            println!("{}", get_store_path(&artifact_hash).display());
                        }
                    }
                }
            }

            Ok(())
        }

        Command::Keys(keys) => match keys {
            CommandKeys::Generate {} => {
                let key_dir_path = vorpal_store::paths::get_key_dir_path();
                let private_key_path = vorpal_store::paths::get_private_key_path();
                let public_key_path = vorpal_store::paths::get_public_key_path();

                if private_key_path.exists() && public_key_path.exists() {
                    warn!("Keys already exist: {}", key_dir_path.display());

                    return Ok(());
                }

                if private_key_path.exists() && !public_key_path.exists() {
                    bail!("private key exists but public key is missing");
                }

                if !private_key_path.exists() && public_key_path.exists() {
                    bail!("public key exists but private key is missing");
                }

                vorpal_notary::generate_keys(key_dir_path, private_key_path, public_key_path)
                    .await?;

                Ok(())
            }
        },

        Command::Start {
            port,
            registry_backend,
            registry_backend_s3_bucket,
            services,
        } => {
            let mut subscriber = FmtSubscriber::builder()
                .with_target(false)
                .without_time()
                .with_max_level(level);

            if [Level::DEBUG, Level::TRACE].contains(&level) {
                subscriber = subscriber.with_file(true).with_line_number(true);
            }

            let subscriber = subscriber.finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber");

            let public_key_path = get_public_key_path();

            if !public_key_path.exists() {
                return Err(anyhow::anyhow!(
                    "public key not found - run 'vorpal keys generate' or copy from agent"
                ));
            }

            let (_, health_service) = tonic_health::server::health_reporter();

            let mut router = Server::builder().add_service(health_service);

            if services.contains("artifact") {
                let system = system_default()?;

                let service = ArtifactServiceServer::new(ArtifactServer::new(registry, system));

                info!("artifact service: [::]:{}", port);

                router = router.add_service(service);
            }

            if services.contains("registry") {
                let backend = match registry_backend.as_str() {
                    "gha" => RegistryServerBackend::GHA,
                    "local" => RegistryServerBackend::Local,
                    "s3" => RegistryServerBackend::S3,
                    _ => RegistryServerBackend::Unknown,
                };

                if backend == RegistryServerBackend::Unknown {
                    bail!("unknown registry backend: {}", registry_backend);
                }

                if backend == RegistryServerBackend::S3 && registry_backend_s3_bucket.is_none() {
                    bail!("s3 backend requires '--registry-backend-s3-bucket' parameter");
                }

                let backend: Box<dyn RegistryBackend> = match backend {
                    RegistryServerBackend::Local => {
                        Box::new(vorpal_registry::LocalRegistryBackend::new()?)
                    }
                    RegistryServerBackend::S3 => Box::new(
                        vorpal_registry::S3RegistryBackend::new(registry_backend_s3_bucket.clone())
                            .await?,
                    ),
                    RegistryServerBackend::GHA => {
                        Box::new(vorpal_registry::GhaRegistryBackend::new()?)
                    }
                    RegistryServerBackend::Unknown => unreachable!(),
                };

                let service = RegistryServiceServer::new(RegistryServer::new(backend));

                info!("registry service: [::]:{}", port);

                router = router.add_service(service);
            }

            let address = format!("[::]:{}", port)
                .parse()
                .expect("failed to parse address");

            router
                .serve(address)
                .await
                .expect("failed to start worker server");

            Ok(())
        }
    }
}
