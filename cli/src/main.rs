use crate::{
    artifact::build,
    rust::{get_rust_toolchain_version, rust_toolchain},
};
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use port_selector::random_free_port;
use std::{
    collections::HashMap,
    env::{
        consts::{ARCH, OS},
        var,
    },
    path::Path,
    process::Stdio,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::{process, process::Child};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::transport::{Channel, Server};
use tracing::{info, warn, Level};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::FmtSubscriber;
use vorpal_registry::{RegistryServer, RegistryServerBackend};
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{
            artifact_service_server::ArtifactServiceServer, Artifact, ArtifactId, ArtifactSystem,
            ArtifactSystem::UnknownSystem,
        },
        config::v0::{config_service_client::ConfigServiceClient, ConfigRequest},
        registry::v0::registry_service_server::RegistryServiceServer,
    },
};
use vorpal_sdk::config::{
    artifact::{language::rust, toolchain::protoc},
    ConfigContext,
};
use vorpal_store::paths::{get_artifact_path, get_public_key_path, setup_paths};

use vorpal_worker::artifact::ArtifactServer;

mod artifact;
mod build;

pub struct VorpalTomlLanguage {
    pub name: String,
}

pub struct VorpalTomlRust {
    pub bin: String,
    pub path: String,
}

pub struct VorpalToml {
    pub language: VorpalTomlLanguage,
    pub rust: VorpalTomlRust,
}

#[derive(Subcommand)]
enum Command {
    Artifact {
        #[arg(long)]
        name: String,

        #[clap(default_value = "http://localhost:23151", long)]
        service: String,

        #[arg(default_value_t = get_default_system(), long)]
        system: String,
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

fn get_default_system() -> String {
    format!("{}-{}", ARCH, OS)
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

    let service = match ConfigServiceClient::connect(host).await {
        Ok(srv) => srv,
        Err(e) => {
            let _ = process
                .kill()
                .await
                .map_err(|_| anyhow!("failed to kill config server"));

            bail!("failed to connect to config server: {}", e);
        }
    };

    Ok((process, service))
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
            name,
            service,
            system,
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

            // Load the system

            let artifact_system: ArtifactSystem = get_artifact_system(system);

            if artifact_system == UnknownSystem {
                bail!("unknown target: {}", artifact_system.as_str_name());
            }

            // Build the configuration

            let config_file = match language.as_str() {
                "rust" => {
                    if rust_bin.is_none() {
                        bail!("no `--rust-bin` specified");
                    }

                    if rust_path.is_none() {
                        bail!("no `--rust-path` specified");
                    }

                    // Build rust toolchain, if not installed

                    info!("-> building configuration toolchain artifacts...");

                    // Setup context
                    let mut build_context =
                        ConfigContext::new(0, registry.clone(), artifact_system);

                    // Setup toolchain
                    let protoc = protoc::artifact(&mut build_context).await?;
                    let toolchain = rust_toolchain(&mut build_context, "vorpal").await?;

                    // Setup build
                    let build_order = build::get_order(&build_context.artifact_id).await?;

                    let mut ready_artifacts = vec![];

                    info!("-> building configuration toolchain...");

                    for artifact_id in &build_order {
                        match build_context.artifact_id.get(artifact_id) {
                            None => bail!("build artifact not found: {}", artifact_id.name),
                            Some(artifact) => {
                                for artifact in &artifact.artifacts {
                                    if !ready_artifacts.contains(&artifact) {
                                        bail!("Artifact not found: {}", artifact.name);
                                    }
                                }

                                build(artifact, artifact_id, artifact_system, &registry, service)
                                    .await?;

                                ready_artifacts.push(artifact_id);
                            }
                        }
                    }

                    let toolchain_path = get_artifact_path(&toolchain.hash, &toolchain.name);

                    if !toolchain_path.exists() {
                        bail!("config toolchain not found: {}", toolchain_path.display());
                    }

                    let toolchain_target = rust::get_toolchain_target(artifact_system)?;
                    let toolchain_version = get_rust_toolchain_version();

                    let toolchain_bin_path = Path::new(&format!(
                        "{}/toolchains/{}-{}/bin",
                        toolchain_path.display(),
                        toolchain_version,
                        toolchain_target
                    ))
                    .to_path_buf();

                    let toolchain_cargo_path =
                        Path::new(&format!("{}/cargo", toolchain_bin_path.display())).to_path_buf();

                    if !toolchain_cargo_path.exists() {
                        bail!("cargo not found: {}", toolchain_cargo_path.display());
                    }

                    // Get protoc

                    let protoc_path = Path::new(&format!(
                        "{}/bin/protoc",
                        get_artifact_path(&protoc.hash, &protoc.name).display()
                    ))
                    .to_path_buf();

                    if !protoc_path.exists() {
                        bail!("protoc not found: {}", protoc_path.display());
                    }

                    // Build the configuration

                    let mut command = process::Command::new(toolchain_cargo_path);

                    command.env(
                        "PATH",
                        format!(
                            "{}:{}/bin:{}",
                            toolchain_bin_path.display(),
                            get_artifact_path(&protoc.hash, &protoc.name).display(),
                            var("PATH").unwrap_or_default()
                        )
                        .as_str(),
                    );

                    command.env("RUSTUP_HOME", toolchain_path.display().to_string());

                    command.env(
                        "RUSTUP_TOOLCHAIN",
                        format!("{}-{}", toolchain_version, toolchain_target),
                    );

                    let config_bin = rust_bin.as_ref().unwrap();

                    command.args(["build", "--bin", config_bin, "--release"]);

                    info!("-> building configuration...");

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

                    let target_path = format!(
                        "{}/target/release/{}",
                        rust_path.as_ref().unwrap(),
                        config_bin
                    );

                    Path::new(&target_path).to_path_buf()
                }

                _ => bail!("unsupported language: {}", language),
            };

            if !config_file.exists() {
                bail!("config file not found: {}", config_file.display());
            }

            if service.is_empty() {
                bail!("no `--artifact-service` specified");
            }

            let (mut config_process, mut config_service) =
                start_config(config_file.display().to_string(), registry.clone()).await?;

            let config_response = match config_service.get_config(ConfigRequest {}).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to evaluate config: {}", error);
                }
            };

            let config = config_response.into_inner();

            let config_artifact_id = config
                .artifacts
                .into_iter()
                .find(|a| a.name == *name)
                .ok_or_else(|| anyhow!("artifact not found: {}", name))?;

            // Create the artifact graph and map

            let mut build_artifact = HashMap::<ArtifactId, Artifact>::new();

            // Get the artifact

            let config_artifact_request = tonic::Request::new(config_artifact_id.clone());

            let config_artifact_response =
                match config_service.get_artifact(config_artifact_request).await {
                    Ok(res) => res,
                    Err(error) => {
                        bail!("failed to evaluate artifact: {}", error);
                    }
                };

            let config_artifact = config_artifact_response.into_inner();

            build_artifact.insert(config_artifact_id.clone(), config_artifact.clone());

            build::get_artifacts(&config_artifact, &mut build_artifact, &mut config_service)
                .await?;

            let build_order = build::get_order(&build_artifact).await?;

            let mut ready_artifacts = vec![];

            info!("-> building artifacts...");

            for artifact_id in &build_order {
                match build_artifact.get(artifact_id) {
                    None => bail!("Build artifact not found: {}", artifact_id.name),
                    Some(artifact) => {
                        for artifact in &artifact.artifacts {
                            if !ready_artifacts.contains(&artifact) {
                                bail!("Artifact not found: {}", artifact.name);
                            }
                        }

                        build(artifact, artifact_id, artifact_system, &registry, service).await?;

                        ready_artifacts.push(artifact_id);

                        if artifact_id.name == *name {
                            println!(
                                "{}",
                                get_artifact_path(&artifact_id.hash, &artifact_id.name).display()
                            );
                        }
                    }
                }
            }

            config_process
                .kill()
                .await
                .map_err(|_| anyhow!("failed to kill config server"))
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

            setup_paths().await?;

            let public_key_path = get_public_key_path();

            if !public_key_path.exists() {
                return Err(anyhow::anyhow!(
                    "public key not found - run 'vorpal keys generate' or copy from agent"
                ));
            }

            let (_, health_service) = tonic_health::server::health_reporter();

            let mut router = Server::builder().add_service(health_service);

            if services.contains("artifact") {
                let system = get_artifact_system(format!("{}-{}", ARCH, OS).as_str());
                let service = ArtifactServiceServer::new(ArtifactServer::new(registry, system));

                info!("artifact service: [::]:{}", port);

                router = router.add_service(service);
            }

            if services.contains("registry") {
                let backend = match registry_backend.as_str() {
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

                let service = RegistryServiceServer::new(RegistryServer::new(
                    backend,
                    registry_backend_s3_bucket.clone(),
                ));

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
