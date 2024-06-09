use crate::api::build_service_server::BuildServiceServer;
use crate::notary;
use crate::store;
use tokio::fs;
use tonic::transport::Server;
use tracing::info;

mod package;
mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    let vorpal_dir = store::get_home_dir_path();
    if !vorpal_dir.exists() {
        fs::create_dir_all(&vorpal_dir).await?;
    }

    info!("Vorpal directory: {:?}", vorpal_dir);

    let store_dir = store::get_store_dir_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("Store directory: {:?}", store_dir);

    let private_key_path = store::get_private_key_path();
    let public_key_path = store::get_public_key_path();
    if !private_key_path.exists() && !public_key_path.exists() {
        let key_dir = store::get_private_key_path();
        fs::create_dir_all(&key_dir).await?;
        info!("Key directory: {:?}", key_dir);
        notary::generate_keys()?;
    }

    info!("Private key: {:?}", private_key_path);
    info!("Public key: {:?}", public_key_path);

    let addr = format!("[::1]:{}", port).parse()?;
    let proxy = service::Proxy::default();

    println!("Proxy listening on: {}", addr);

    Server::builder()
        .add_service(BuildServiceServer::new(proxy))
        .serve(addr)
        .await?;

    Ok(())
}
