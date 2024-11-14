use crate::{artifact::ArtifactServer, store::StoreServer};
use anyhow::Result;
use std::env::consts::{ARCH, OS};
use tonic::transport::Server;
use tracing::info;
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::artifact_service_server::ArtifactServiceServer,
        store::v0::store_service_server::StoreServiceServer,
    },
};
use vorpal_store::paths::{get_public_key_path, setup_paths};

pub async fn listen(port: u16) -> Result<()> {
    setup_paths().await?;

    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let system = get_artifact_system(format!("{}-{}", ARCH, OS).as_str());

    info!("worker default target: {:?}", system);

    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    info!("worker address: {}", addr);

    let artifact_service = ArtifactServiceServer::new(ArtifactServer::new(system));
    let store_service = StoreServiceServer::new(StoreServer::default());

    Server::builder()
        .add_service(artifact_service)
        .add_service(store_service)
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}
