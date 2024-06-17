use crate::api::package_service_server::PackageServiceServer;
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
    let packager = service::Package::default();

    info!("service listening on: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(packager))
        .serve(addr)
        .await?;

    Ok(())
}
