use crate::command::{
    start::{
        agent::AgentServer,
        registry::{
            backend_archive, backend_artifact, ArchiveServer, ArtifactServer, ServerBackend,
        },
        worker::WorkerServer,
    },
    store::paths::{
        get_key_service_key_path, get_key_service_path, get_lock_path, get_socket_path,
    },
};
use anyhow::{bail, Result};
use fs4::fs_std::FileExt;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tokio::fs::read_to_string;
use tokio::net::{TcpListener, UnixListener};
use tokio_stream::wrappers::{TcpListenerStream, UnixListenerStream};
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic_health::{pb::health_server::HealthServer, server::HealthService};
use tracing::{info, warn};
use vorpal_sdk::api::{
    agent::agent_service_server::AgentServiceServer,
    archive::archive_service_server::ArchiveServiceServer,
    artifact::artifact_service_server::ArtifactServiceServer,
    worker::worker_service_server::WorkerServiceServer,
};

mod agent;
pub mod auth;
mod registry;
mod worker;

pub struct RunArgs {
    pub archive_cache_ttl: u64,
    pub health_check: bool,
    pub health_check_port: u16,
    pub issuer: Option<String>,
    pub issuer_audience: Option<String>,
    pub issuer_client_id: Option<String>,
    pub issuer_client_secret: Option<String>,
    pub port: Option<u16>,
    pub registry_backend: String,
    pub registry_backend_s3_bucket: Option<String>,
    pub registry_backend_s3_force_path_style: bool,
    pub services: Vec<String>,
    pub tls: bool,
}

async fn new_tls_config() -> Result<ServerTlsConfig> {
    let service_key_path = get_key_service_path();

    if !service_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal system keys generate' or copy from agent"
        ));
    }

    let service_private_key_path = get_key_service_key_path();

    if !service_private_key_path.exists() {
        return Err(anyhow::anyhow!(
            "private key not found - run 'vorpal system keys generate' or copy from agent"
        ));
    }

    let cert = read_to_string(&cert_path).await.map_err(|err| {
        anyhow::anyhow!("failed to read public key {}: {}", cert_path.display(), err)
    })?;

    let private_key = read_to_string(&private_key_path).await.map_err(|err| {
        anyhow::anyhow!(
            "failed to read private key {}: {}",
            private_key_path.display(),
            err
        )
    })?;

    let config_identity = Identity::from_pem(service_key, service_private_key);

    let config = ServerTlsConfig::new().identity(config_identity);

    Ok(config)
}

async fn serve_with_shutdown(
    serve_future: impl std::future::Future<Output = Result<(), tonic::transport::Error>>,
    sigterm: &mut tokio::signal::unix::Signal,
) -> Result<()> {
    tokio::select! {
        res = serve_future => {
            res.map_err(|err| anyhow::anyhow!("main server failed: {}", err))
        }
        _ = tokio::signal::ctrl_c() => {
            info!("received SIGINT, shutting down");
            Ok(())
        }
        _ = sigterm.recv() => {
            info!("received SIGTERM, shutting down");
            Ok(())
        }
    }
}

pub async fn run(args: RunArgs) -> Result<()> {
    // Determine the effective port: TLS implies TCP (default 23151), explicit --port uses TCP
    let effective_port = match (args.port, args.tls) {
        (Some(port), _) => Some(port),
        (None, true) => Some(23151),
        (None, false) => None, // UDS mode
    };

    if let Some(port) = effective_port {
        if args.health_check && args.health_check_port == port {
            bail!(
                "health check port ({}) must differ from the main service port ({})",
                args.health_check_port,
                port
            );
        }
    }

    let (health_reporter, health_service) = tonic_health::server::health_reporter();

    let mut router = if args.tls {
        info!("TLS enabled for main listener");
        let tls_config = new_tls_config().await?;
        Server::builder()
            .tls_config(tls_config)?
            .add_service(health_service)
    } else {
        let transport = if effective_port.is_some() {
            "plaintext TCP"
        } else {
            "Unix domain socket"
        };
        info!("TLS disabled, using {} for main listener", transport);
        Server::builder().add_service(health_service)
    };

    let health_prepared = if args.health_check {
        let health_service_plaintext =
            HealthServer::new(HealthService::from_health_reporter(health_reporter.clone()));

        let health_address = format!("[::]:{}", args.health_check_port);

        let health_listener = TcpListener::bind(&health_address).await.map_err(|err| {
            anyhow::anyhow!(
                "failed to bind health server on {}: {}",
                health_address,
                err
            )
        })?;

        let health_router = Server::builder().add_service(health_service_plaintext);

        Some((health_router, health_listener, health_address))
    } else {
        None
    };

    let transport_label = match effective_port {
        Some(port) => format!("[::]:{}", port),
        None => get_socket_path().display().to_string(),
    };

    let has_agent = args.services.contains(&"agent".to_string());

    if has_agent {
        let service = AgentServiceServer::new(AgentServer::new());

        router = router.add_service(service);

        info!("agent |> service: {}", transport_label);
    }

    let has_registry = args.services.contains(&"registry".to_string());

    if has_registry {
        let backend = match args.registry_backend.as_str() {
            "local" => ServerBackend::Local,
            "s3" => ServerBackend::S3,
            _ => ServerBackend::Unknown,
        };

        if backend == ServerBackend::Unknown {
            bail!("unknown registry backend: {}", args.registry_backend);
        }

        if backend == ServerBackend::S3 && args.registry_backend_s3_bucket.is_none() {
            bail!("s3 backend requires '--registry-backend-s3-bucket' parameter");
        }

        let backend_archive = backend_archive(
            args.registry_backend.clone(),
            args.registry_backend_s3_bucket.clone(),
            args.registry_backend_s3_force_path_style,
        )
        .await?;

        let backend_artifact = backend_artifact(
            &args.registry_backend,
            args.registry_backend_s3_bucket,
            args.registry_backend_s3_force_path_style,
        )
        .await?;

        let archive_server = ArchiveServer::new(backend_archive, args.archive_cache_ttl);
        let artifact_server = ArtifactServer::new(backend_artifact);

        if let Some(issuer) = &args.issuer {
            let mut validator_audiences = vec![];

            if let Some(audience) = &args.issuer_audience {
                validator_audiences.push(audience.clone());
            }

            let validator =
                Arc::new(auth::OidcValidator::new(issuer.clone(), validator_audiences).await?);
            let validator_intercepter = auth::new_interceptor(validator);

            router = router.add_service(ArchiveServiceServer::with_interceptor(
                archive_server,
                validator_intercepter.clone(),
            ));

            router = router.add_service(ArtifactServiceServer::with_interceptor(
                artifact_server,
                validator_intercepter,
            ));
        } else {
            router = router.add_service(ArchiveServiceServer::new(archive_server));
            router = router.add_service(ArtifactServiceServer::new(artifact_server));
        }

        info!("archive |> service: {}", transport_label);
        info!("artifact |> service: {}", transport_label);
    }

    let has_worker = args.services.contains(&"worker".to_string());

    if has_worker {
        let worker_server = WorkerServer::new(
            args.issuer.clone(),
            args.issuer_audience.clone(),
            args.issuer_client_id,
            args.issuer_client_secret,
        );

        if let Some(issuer) = &args.issuer {
            let mut validator_audiences = vec![];

            if let Some(audience) = &args.issuer_audience {
                validator_audiences.push(audience.clone());
            }

            let validator =
                Arc::new(auth::OidcValidator::new(issuer.clone(), validator_audiences).await?);
            let validator_intercepter = auth::new_interceptor(validator);

            router = router.add_service(WorkerServiceServer::with_interceptor(
                worker_server,
                validator_intercepter,
            ));
        } else {
            router = router.add_service(WorkerServiceServer::new(worker_server));
        }

        info!("worker |> service: {}", transport_label);
    }

    tokio::spawn(async move {
        tokio::task::yield_now().await;

        if has_agent {
            health_reporter
                .set_serving::<AgentServiceServer<AgentServer>>()
                .await;
        }

        if has_registry {
            health_reporter
                .set_serving::<ArchiveServiceServer<ArchiveServer>>()
                .await;
            health_reporter
                .set_serving::<ArtifactServiceServer<ArtifactServer>>()
                .await;
        }

        if has_worker {
            health_reporter
                .set_serving::<WorkerServiceServer<WorkerServer>>()
                .await;
        }
    });

    let health_handle =
        if let Some((health_router, health_listener, health_address)) = health_prepared {
            let health_incoming = TcpListenerStream::new(health_listener);

            if effective_port.is_none() {
                info!(
                    "health check using TCP (port {}) while main services use UDS",
                    args.health_check_port
                );
            }

            info!("health |> service: {}", health_address);

            Some(tokio::spawn(async move {
                if let Err(err) = health_router.serve_with_incoming(health_incoming).await {
                    warn!("health server failed: {}", err);
                }
            }))
        } else {
            None
        };

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|err| anyhow::anyhow!("failed to register SIGTERM handler: {}", err))?;

    // Bind and serve: UDS when no port specified, TCP otherwise
    let result = if let Some(port) = effective_port {
        let address = format!("[::]:{}", port);
        let main_listener = TcpListener::bind(&address)
            .await
            .map_err(|err| anyhow::anyhow!("failed to bind main server on {}: {}", address, err))?;
        info!("listening on TCP: {}", address);
        let main_incoming = TcpListenerStream::new(main_listener);
        serve_with_shutdown(router.serve_with_incoming(main_incoming), &mut sigterm).await
    } else {
        let socket_path = get_socket_path();

        // Ensure parent directories exist (shared by socket and lock file)
        let lock_path = get_lock_path();
        if let Some(parent) = lock_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|err| {
                anyhow::anyhow!(
                    "failed to create lock directory {}: {}",
                    parent.display(),
                    err
                )
            })?;
        }

        // Acquire advisory lock to prevent TOCTOU races with stale socket detection.
        // The lock is held for the lifetime of the server (released on drop).
        let _lock_file = std::fs::File::create(&lock_path).map_err(|err| {
            anyhow::anyhow!(
                "failed to create lock file {}: {}",
                lock_path.display(),
                err
            )
        })?;
        let acquired = _lock_file.try_lock_exclusive().map_err(|err| {
            anyhow::anyhow!("failed to acquire lock on {}: {}", lock_path.display(), err)
        })?;
        if !acquired {
            bail!(
                "another instance is already running (lock file: {})",
                lock_path.display()
            );
        }

        // Check if an existing socket is still alive before removing it
        if socket_path.exists() {
            match tokio::net::UnixStream::connect(&socket_path).await {
                Ok(_) => {
                    bail!(
                        "socket {} is already in use by a running instance",
                        socket_path.display()
                    );
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::ConnectionRefused
                        || e.kind() == std::io::ErrorKind::NotConnected =>
                {
                    info!(
                        "removing stale socket file ({}): {}",
                        e.kind(),
                        socket_path.display()
                    );
                    tokio::fs::remove_file(&socket_path).await.map_err(|err| {
                        anyhow::anyhow!(
                            "failed to remove stale socket {}: {}",
                            socket_path.display(),
                            err
                        )
                    })?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    bail!(
                        "socket {} exists but permission denied — it may belong to another user",
                        socket_path.display()
                    );
                }
                Err(e) => {
                    bail!(
                        "failed to check existing socket {}: {}",
                        socket_path.display(),
                        e
                    );
                }
            }
        }
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|err| {
                anyhow::anyhow!(
                    "failed to create socket directory {}: {}",
                    parent.display(),
                    err
                )
            })?;
        }
        let uds_listener = UnixListener::bind(&socket_path).map_err(|err| {
            anyhow::anyhow!(
                "failed to bind unix socket {}: {}",
                socket_path.display(),
                err
            )
        })?;
        tokio::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o660))
            .await
            .map_err(|err| {
                anyhow::anyhow!(
                    "failed to set socket permissions on {}: {}",
                    socket_path.display(),
                    err
                )
            })?;
        info!("listening on unix socket: {}", socket_path.display());
        let main_incoming = UnixListenerStream::new(uds_listener);
        let cleanup_path = socket_path.clone();
        let result =
            serve_with_shutdown(router.serve_with_incoming(main_incoming), &mut sigterm).await;
        // Clean up socket file on shutdown (covers both clean exit and signal).
        // The lock file is intentionally left on disk — the advisory lock is
        // released automatically when _lock_file is dropped at the end of this
        // block. Deleting it here would race with a new instance creating a
        // fresh inode and acquiring its own lock.
        if let Err(err) = tokio::fs::remove_file(&cleanup_path).await {
            warn!("failed to remove socket file on shutdown: {}", err);
        }
        result
    };

    if let Some(handle) = health_handle {
        handle.abort();
    }

    result
}
