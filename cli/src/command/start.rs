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

pub async fn run(
    issuer: Option<String>,
    port: u16,
    registry_backend: String,
    registry_backend_s3_bucket: Option<String>,
    services: Vec<String>,
) -> Result<()> {
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

    let key = read_to_string(private_key_path)
        .await
        .expect("failed to read private key");

    let (_, health_service) = tonic_health::server::health_reporter();

    let identity = Identity::from_pem(cert, key);

    let mut router = Server::builder()
        .tls_config(ServerTlsConfig::new().identity(identity))?
        .add_service(health_service);

    // let service_secret = auth::load_service_secret().await?;

    let mut auth_interceptor = None;

    if let Some(issuer) = issuer {
        auth_interceptor = Some(auth::create_interceptor(issuer));
    }

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

        if let Some(intercepter) = auth_interceptor.clone() {
            router = router.add_service(ArchiveServiceServer::with_interceptor(
                ArchiveServer::new(backend_archive),
                intercepter.clone(),
            ));

            router = router.add_service(ArtifactServiceServer::with_interceptor(
                ArtifactServer::new(backend_artifact),
                intercepter.clone(),
            ));
        } else {
            router = router.add_service(ArchiveServiceServer::new(ArchiveServer::new(
                backend_archive,
            )));

            router = router.add_service(ArtifactServiceServer::new(ArtifactServer::new(
                backend_artifact,
            )));
        }

        info!("archive |> service: [::]:{}", port);
        info!("artifact |> service: [::]:{}", port);
    }

    if services.contains(&"worker".to_string()) {
        if let Some(intercepter) = auth_interceptor {
            router = router.add_service(WorkerServiceServer::with_interceptor(
                WorkerServer::new(),
                intercepter,
            ));
        } else {
            router = router.add_service(WorkerServiceServer::new(WorkerServer::new()));
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
