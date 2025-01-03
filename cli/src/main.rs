use crate::{artifact::build, rust::get_rust_toolchain_version};
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use port_selector::random_free_port;
use std::{
    collections::HashMap,
    env::{
        consts::{ARCH, OS},
        var,
    },
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
use vorpal_store::paths::{get_artifact_path, get_public_key_path};
use vorpal_worker::artifact::ArtifactServer;

mod artifact;
mod build;

#[derive(Subcommand)]
enum Command {
    Artifact {
        #[arg(default_value_t = false, long)]
        export: bool,

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

async fn get_config_file_path(
    artifact_system: ArtifactSystem,
    language: String,
    registry: String,
    rust_bin: Option<String>,
    rust_path: Option<String>,
    service: String,
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

            let mut build_context = ConfigContext::new(0, registry.clone(), artifact_system);

            // Setup toolchain artifacts

            let protoc = protoc::artifact(&mut build_context).await?;
            let toolchain = rust::toolchain_artifact(&mut build_context, "vorpal").await?;

            // Setup build

            let build_order = build::get_order(&build_context.artifact_id).await?;

            let mut ready_artifacts = vec![];

            for artifact_id in &build_order {
                match build_context.artifact_id.get(artifact_id) {
                    None => bail!("build artifact not found: {}", artifact_id.name),
                    Some(artifact) => {
                        for artifact in &artifact.artifacts {
                            if !ready_artifacts.contains(&artifact) {
                                bail!("Artifact not found: {}", artifact.name);
                            }
                        }

                        build(artifact, artifact_id, artifact_system, &registry, &service).await?;

                        ready_artifacts.push(artifact_id);
                    }
                }
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

            // Get toolchain

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

            // Build configuration with toolchain

            let mut command = process::Command::new(toolchain_cargo_path);

            // Setup environment variables

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

            // Setup command

            let config_bin = rust_bin.as_ref().unwrap();

            command.args(["build", "--bin", config_bin]);

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

            let config_file_path = format!(
                "{}/target/debug/{}",
                rust_path.as_ref().unwrap(),
                config_bin
            );

            Ok(Path::new(&config_file_path).to_path_buf())
        }

        _ => bail!("unsupported language: {}", language),
    }
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
            export: export_artifact,
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

            if service.is_empty() {
                bail!("no `--artifact-service` specified");
            }

            let system: ArtifactSystem = get_artifact_system(system);

            if system == UnknownSystem {
                bail!("unknown target: {}", system.as_str_name());
            }

            let config_file = get_config_file_path(
                system,
                language,
                registry.clone(),
                rust_bin.clone(),
                rust_path.clone(),
                service.clone(),
            )
            .await?;

            if !config_file.exists() {
                bail!("config file not found: {}", config_file.display());
            }

            let (mut config_process, mut config_service) =
                start_config(config_file.display().to_string(), registry.clone()).await?;

            let config_response = match config_service.get_config(ConfigRequest {}).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to evaluate config: {}", error);
                }
            };

            let config_response = config_response.into_inner();

            let artifact_id_selected = config_response
                .clone()
                .artifacts
                .into_iter()
                .find(|a| a.name == *name)
                .ok_or_else(|| anyhow!("artifact not found: {}", name))?;

            // Get the artifact

            let artifact_request = tonic::Request::new(artifact_id_selected.clone());

            let artifact_response = match config_service.get_artifact(artifact_request).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to evaluate artifact: {}", error);
                }
            };

            let artifact_selected = artifact_response.into_inner();

            let mut artifact = HashMap::<ArtifactId, Artifact>::new();

            artifact.insert(artifact_id_selected.clone(), artifact_selected.clone());

            build::get_artifacts(&artifact_selected, &mut artifact, &mut config_service).await?;

            if *export_artifact {
                let mut artifacts = vec![];

                for a in artifact.values() {
                    artifacts.push(a.clone());
                }

                artifacts.sort_by(|a, b| a.name.cmp(&b.name));

                let export_json = serde_json::to_string_pretty(&artifacts).unwrap();

                println!("{}", export_json);

                return Ok(());
            }

            // Create the artifact graph and map

            let build_order = build::get_order(&artifact).await?;

            let mut ready_artifacts = vec![];

            for artifact_id in &build_order {
                match artifact.get(artifact_id) {
                    None => bail!("Build artifact not found: {}", artifact_id.name),
                    Some(artifact) => {
                        for artifact in &artifact.artifacts {
                            if !ready_artifacts.contains(&artifact) {
                                bail!("Artifact not found: {}", artifact.name);
                            }
                        }

                        build(artifact, artifact_id, system, &registry, service).await?;

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
