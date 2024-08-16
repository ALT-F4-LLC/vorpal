use crate::package::PackageServer;
use crate::store::StoreServer;
use anyhow::Result;
use std::env::consts::{ARCH, OS};
use tonic::transport::Server;
use tracing::info;
use vorpal_schema::{
    api::{
        package::{package_service_server::PackageServiceServer, PackageSystem},
        store::store_service_server::StoreServiceServer,
    },
    get_package_target,
};
use vorpal_store::paths::{get_public_key_path, setup_paths};

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    setup_paths().await?;

    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let mut default_target = get_package_target(format!("{}-{}", ARCH, OS).as_str());

    if default_target == PackageSystem::Aarch64Macos {
        default_target = PackageSystem::Aarch64Linux; // docker uses linux on macos
    }

    info!("worker default target: {:?}", default_target);

    let addr = format!("[::]:{}", port).parse()?;

    info!("worker address: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(PackageServer::new(
            default_target,
        )))
        .add_service(StoreServiceServer::new(StoreServer::default()))
        .serve(addr)
        .await?;

    Ok(())
}
