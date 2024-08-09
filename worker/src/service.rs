use crate::package::PackageServer;
use crate::store::StoreServer;
use anyhow::Result;
use std::env::consts::{ARCH, OS};
use tokio::fs;
use tonic::transport::Server;
use tracing::info;
use vorpal_schema::{
    api::{
        package::package_service_server::PackageServiceServer,
        store::store_service_server::StoreServiceServer,
    },
    get_package_target,
};
use vorpal_store::paths::{get_key_path, get_public_key_path, get_sandbox_path, get_store_path};

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    let key_path = get_key_path();
    if !key_path.exists() {
        fs::create_dir_all(&key_path).await?;
    }

    info!("keys path: {:?}", key_path);

    let sandbox_path = get_sandbox_path();
    if !sandbox_path.exists() {
        fs::create_dir_all(&sandbox_path).await?;
    }

    info!("sandbox path: {:?}", sandbox_path);

    let store_dir = get_store_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("store path: {:?}", store_dir);

    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let target = get_package_target(format!("{}-{}", ARCH, OS).as_str());

    info!("service target: {:?}", target);

    let addr = format!("[::]:{}", port).parse()?;

    info!("service address: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(PackageServer::new(target)))
        .add_service(StoreServiceServer::new(StoreServer::default()))
        .serve(addr)
        .await?;

    Ok(())
}
