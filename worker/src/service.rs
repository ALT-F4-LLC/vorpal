use crate::{store::StoreServer, worker::WorkerServer};
use anyhow::Result;
use std::env::consts::{ARCH, OS};
use tonic::transport::Server;
use tracing::info;
use vorpal_schema::{
    get_package_system,
    vorpal::{
        store::v0::store_service_server::StoreServiceServer,
        worker::v0::worker_service_server::WorkerServiceServer,
    },
};
use vorpal_store::paths::{get_public_key_path, setup_paths};

pub async fn start(port: u16) -> Result<()> {
    setup_paths().await?;

    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let system = get_package_system(format!("{}-{}", ARCH, OS).as_str());

    info!("worker default target: {:?}", system);

    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    info!("worker address: {}", addr);

    let worker_service = WorkerServiceServer::new(WorkerServer::new(system));
    let store_service = StoreServiceServer::new(StoreServer::default());

    Server::builder()
        .add_service(store_service)
        .add_service(worker_service)
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}
