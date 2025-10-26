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
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tracing::info;
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

    let priavate_key = read_to_string(private_key_path)
        .await
        .expect("failed to read private key");

    let config_identity = Identity::from_pem(cert, priavate_key);

    let config = ServerTlsConfig::new().identity(config_identity);

    Ok(config)
}

pub async fn run(
    issuer: Option<String>,
    issuer_client_id: Option<String>,
    issuer_client_secret: Option<String>,
    port: u16,
    registry_backend: String,
    registry_backend_s3_bucket: Option<String>,
    services: Vec<String>,
) -> Result<()> {
    let tls_config = new_tls_config().await?;

    let (_, health_service) = tonic_health::server::health_reporter();

    let mut router = Server::builder()
        .tls_config(tls_config)?
        .add_service(health_service);

    if services.contains(&"agent".to_string()) {
        let service = AgentServiceServer::new(AgentServer::new());

        router = router.add_service(service);

        info!("agent |> service: [::]:{}", port);
    }

    if services.contains(&"registry".to_string()) {
        let backend = match registry_backend.as_str() {
            "local" => ServerBackend::Local,
            "s3" => ServerBackend::S3,
            _ => ServerBackend::Unknown,
        };

        if backend == ServerBackend::Unknown {
            bail!("unknown registry backend: {}", registry_backend);
        }

        if backend == ServerBackend::S3 && registry_backend_s3_bucket.is_none() {
            bail!("s3 backend requires '--registry-backend-s3-bucket' parameter");
        }

        let backend_archive =
            backend_archive(registry_backend.clone(), registry_backend_s3_bucket.clone()).await?;

        let backend_artifact =
            backend_artifact(&registry_backend, registry_backend_s3_bucket).await?;

        let archive_server = ArchiveServer::new(backend_archive);
        let artifact_server = ArtifactServer::new(backend_artifact);

        if let Some(issuer) = &issuer {
            let validator_audiences = vec!["cli".to_string(), "worker".to_string()];
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

        info!("archive |> service: [::]:{}", port);
        info!("artifact |> service: [::]:{}", port);
    }

    if services.contains(&"worker".to_string()) {
        let worker_server =
            WorkerServer::new(issuer.clone(), issuer_client_id, issuer_client_secret);

        if let Some(issuer) = &issuer {
            let validator_audiences = vec!["cli".to_string()];
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

        info!("worker |> service: [::]:{}", port);
    }

    let address = format!("[::]:{port}")
        .parse()
        .expect("failed to parse address");

    router
        .serve(address)
        .await
        .expect("failed to start worker server");

    Ok(())
}
