use crate::api::package_service_server::PackageServiceServer;
use crate::api::store_service_server::StoreServiceServer;
use crate::notary;
use crate::service::get_build_system;
use crate::service::package::Package;
use crate::service::store::Store;
use crate::store;
use anyhow::Result;
use std::env::consts::{ARCH, OS};
use tonic::transport::Server;
use tracing::info;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    store::check().await?;

    notary::check_worker()?;

    let system = get_build_system(format!("{}-{}", ARCH, OS).as_str());

    info!("service system: {:?}", system);

    let addr = format!("[::]:{}", port).parse()?;

    info!("service address: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(Package::new(system)))
        .add_service(StoreServiceServer::new(Store::default()))
        .serve(addr)
        .await?;

    Ok(())
}
