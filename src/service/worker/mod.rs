use crate::api::package_service_server::PackageServiceServer;
use crate::api::store_service_server::StoreServiceServer;
use crate::notary;
use crate::store;
use anyhow::Result;
use tonic::transport::Server;
use tracing::info;

mod sandbox_default;
mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    store::check_dirs().await?;

    notary::check_keys()?;

    let addr = format!("[::1]:{}", port).parse()?;
    let package = service::Package::default();
    let store = service::Store::default();

    info!("service listening on: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(package))
        .add_service(StoreServiceServer::new(store))
        .serve(addr)
        .await?;

    Ok(())
}
