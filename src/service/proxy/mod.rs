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

    info!("root directory: {}", vorpal_dir.display());

    let store_dir = store::get_store_dir_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("store directory: {:?}", store_dir);

    let private_key_path = store::get_private_key_path();
    let public_key_path = store::get_public_key_path();
    if !private_key_path.exists() && !public_key_path.exists() {
        let key_dir = store::get_key_dir_path();
        fs::create_dir_all(&key_dir).await?;
        info!("key directory: {:?}", key_dir);
        notary::generate_keys()?;
    }

    info!("private key: {:?}", private_key_path);
    info!("public key: {:?}", public_key_path);

    let addr = format!("[::1]:{}", port).parse()?;
    let proxy = service::Proxy::default();

    info!("service listening on: {}", addr);

    Server::builder()
        .add_service(BuildServiceServer::new(proxy))
        .serve(addr)
        .await?;

    Ok(())
}
