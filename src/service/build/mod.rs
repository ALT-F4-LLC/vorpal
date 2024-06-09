use crate::api::package_service_server::PackageServiceServer;
use crate::database;
use crate::notary;
use crate::store;
use anyhow::Result;
use tokio::fs;
use tonic::transport::Server;
use tracing::info;

mod run_build;
mod run_prepare;
mod sandbox_default;
pub mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    let vorpal_dir = store::get_home_dir_path();
    if !vorpal_dir.exists() {
        fs::create_dir_all(&vorpal_dir).await?;
    }

    info!("resolved vorpal directory: {}", vorpal_dir.display());

    let store_dir = store::get_store_dir_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("Store directory: {:?}", store_dir);

    let private_key_path = store::get_private_key_path();
    let public_key_path = store::get_public_key_path();
    if !private_key_path.exists() && !public_key_path.exists() {
        let key_dir = store::get_key_dir_path();
        fs::create_dir_all(&key_dir).await?;
        info!("Key directory: {:?}", key_dir);
        notary::generate_keys()?;
    }

    info!("Private key: {:?}", private_key_path);
    info!("Public key: {:?}", public_key_path);

    let db_path = store::get_database_path();
    let db = database::connect(&db_path)?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS source (
                id  INTEGER PRIMARY KEY,
                hash TEXT NOT NULL,
                name TEXT NOT NULL
            )",
        [],
    )?;

    info!("Database path: {:?}", db_path.display());

    if let Err(e) = db.close() {
        return Err(e.1.into());
    }

    let addr = format!("[::1]:{}", port).parse()?;
    let packager = service::Package::default();

    info!("Build listening on: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(packager))
        .serve(addr)
        .await?;

    Ok(())
}
