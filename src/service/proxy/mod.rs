use crate::api::build_service_server::BuildServiceServer;
use crate::notary;
use crate::store;
use tokio::fs;
use tonic::transport::Server;

mod package;
mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    let vorpal_dir = store::get_home_path();
    if !vorpal_dir.exists() {
        fs::create_dir_all(&vorpal_dir).await?;
    }

    println!("Vorpal directory: {:?}", vorpal_dir);

    let package_dir = store::get_package_path();
    if !package_dir.exists() {
        fs::create_dir_all(&package_dir).await?;
    }

    println!("Package directory: {:?}", package_dir);

    let store_dir = store::get_store_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    println!("Store directory: {:?}", store_dir);

    let private_key_path = store::get_private_key_path();
    let public_key_path = store::get_public_key_path();
    if !private_key_path.exists() && !public_key_path.exists() {
        let key_dir = store::get_key_path();
        fs::create_dir_all(&key_dir).await?;
        println!("Key directory: {:?}", key_dir);
        notary::generate_keys()?;
    }

    println!("Private key: {:?}", private_key_path);
    println!("Public key: {:?}", public_key_path);

    let addr = format!("[::1]:{}", port).parse()?;
    let proxy = service::Proxy::default();

    println!("Proxy listening on: {}", addr);

    Server::builder()
        .add_service(BuildServiceServer::new(proxy))
        .serve(addr)
        .await?;

    Ok(())
}
