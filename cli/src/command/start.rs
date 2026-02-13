use crate::command::{
    start::{
        agent::AgentServer,
        registry::{
            backend_archive, backend_artifact, ArchiveServer, ArtifactServer, ServerBackend,
        },
        worker::WorkerServer,
    },
    store::paths::{get_key_service_key_path, get_key_service_path},
};
use anyhow::{bail, Result};
use std::sync::Arc;
use tokio::fs::read_to_string;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
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
    pub archive_check_cache_ttl: u64,
    pub health_check: bool,
    pub health_check_port: u16,
    pub issuer: Option<String>,
    pub issuer_audience: Option<String>,
    pub issuer_client_id: Option<String>,
    pub issuer_client_secret: Option<String>,
    pub port: u16,
    pub registry_backend: String,
    pub registry_backend_s3_bucket: Option<String>,
    pub registry_backend_s3_force_path_style: bool,
    pub services: Vec<String>,
    pub tls: bool,
}

async fn new_tls_config() -> Result<ServerTlsConfig> {
    let cert_path = get_key_service_path();

    if !cert_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal system keys generate' or copy from agent"
        ));
    }

    let private_key_path = get_key_service_key_path();

    if !private_key_path.exists() {
        return Err(anyhow::anyhow!(
            "private key not found - run 'vorpal system keys generate' or copy from agent"
        ));
    }

    let cert = read_to_string(cert_path)
        .await
        .expect("failed to read public key");

    let private_key = read_to_string(private_key_path)
        .await
        .expect("failed to read private key");

    let config_identity = Identity::from_pem(cert, private_key);

    let config = ServerTlsConfig::new().identity(config_identity);

    Ok(config)
}

pub async fn run(args: RunArgs) -> Result<()> {
    if args.health_check && args.health_check_port == args.port {
        bail!(
            "health check port ({}) must differ from the main service port ({})",
            args.health_check_port,
            args.port
        );
    }

    let (health_reporter, health_service) = tonic_health::server::health_reporter();

    let mut router = if args.tls {
        info!("TLS enabled for main listener");
        let tls_config = new_tls_config().await?;
        Server::builder()
            .tls_config(tls_config)?
            .add_service(health_service)
    } else {
        info!("TLS disabled, using plaintext for main listener");
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

    let has_agent = args.services.contains(&"agent".to_string());

    if has_agent {
        let service = AgentServiceServer::new(AgentServer::new());

        router = router.add_service(service);

        info!("agent |> service: [::]:{}", args.port);
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

        let archive_server = ArchiveServer::new(backend_archive, args.archive_check_cache_ttl);
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

        info!("archive |> service: [::]:{}", args.port);
        info!("artifact |> service: [::]:{}", args.port);
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

        info!("worker |> service: [::]:{}", args.port);
    }

    let address = format!("[::]:{}", args.port);

    let main_listener = TcpListener::bind(&address)
        .await
        .map_err(|err| anyhow::anyhow!("failed to bind main server on {}: {}", address, err))?;

    let main_incoming = TcpListenerStream::new(main_listener);

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

    if let Some((health_router, health_listener, health_address)) = health_prepared {
        let health_incoming = TcpListenerStream::new(health_listener);

        info!("health |> service: {}", health_address);

        let health_handle = tokio::spawn(async move {
            if let Err(err) = health_router.serve_with_incoming(health_incoming).await {
                warn!("health server failed: {}", err);
            }
        });

        let result = router
            .serve_with_incoming(main_incoming)
            .await
            .map_err(|err| anyhow::anyhow!("main server failed: {}", err));

        health_handle.abort();

        result
    } else {
        router
            .serve_with_incoming(main_incoming)
            .await
            .map_err(|err| anyhow::anyhow!("main server failed: {}", err))
    }
}
