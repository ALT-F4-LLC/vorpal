use crate::command::{
    start::{
        agent::AgentServer,
        registry::{
            backend_archive, backend_artifact, ArchiveServer, ArtifactServer, ServerBackend,
        },
        worker::WorkerServer,
    },
    store::paths::get_key_public_path,
};
use anyhow::{bail, Result};
use tonic::transport::Server;
use tracing::info;
use vorpal_sdk::api::{
    agent::agent_service_server::AgentServiceServer,
    archive::archive_service_server::ArchiveServiceServer,
    artifact::artifact_service_server::ArtifactServiceServer,
    worker::worker_service_server::WorkerServiceServer,
};

mod agent;
mod registry;
mod worker;

pub async fn run(
    port: u16,
    registry: String,
    registry_backend: String,
    registry_backend_s3_bucket: Option<String>,
    services: Vec<String>,
) -> Result<()> {
    let public_key_path = get_key_public_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal system keys generate' or copy from agent"
        ));
    }

    let (_, health_service) = tonic_health::server::health_reporter();

    let mut router = Server::builder().add_service(health_service);

    if services.contains(&"agent".to_string()) {
        let service = AgentServiceServer::new(AgentServer::new(registry.clone()));

        info!("agent service: [::]:{}", port);

        router = router.add_service(service);
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

        info!("registry service: [::]:{}", port);

        router = router.add_service(ArchiveServiceServer::new(ArchiveServer::new(
            backend_archive,
        )));

        router = router.add_service(ArtifactServiceServer::new(ArtifactServer::new(
            backend_artifact,
        )));
    }

    if services.contains(&"worker".to_string()) {
        let service = WorkerServiceServer::new(WorkerServer::new(registry.to_owned()));

        info!("worker service: [::]:{}", port);

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
