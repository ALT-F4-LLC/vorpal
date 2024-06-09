use crate::api::package_service_server::PackageServiceServer;
use crate::database;
use crate::notary;
use crate::store;
use anyhow::Result;
use tokio::fs;
use tonic::transport::Server;

mod run_build;
mod run_prepare;
mod sandbox_default;
pub mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    let vorpal_dir = store::get_home_dir_path();
    if !vorpal_dir.exists() {
        fs::create_dir_all(&vorpal_dir).await?;
    }

    println!("Vorpal directory: {:?}", vorpal_dir);

    let store_dir = store::get_store_dir_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    println!("Store directory: {:?}", store_dir);

    let private_key_path = store::get_private_key_path();
    let public_key_path = store::get_public_key_path();
    if !private_key_path.exists() && !public_key_path.exists() {
        let key_dir = store::get_key_dir_path();
        fs::create_dir_all(&key_dir).await?;
        println!("Key directory: {:?}", key_dir);
        notary::generate_keys()?;
    }

    println!("Private key: {:?}", private_key_path);
    println!("Public key: {:?}", public_key_path);

    let db_path = store::get_database_path();
    let db = database::connect(db_path.clone())?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS source (
                id  INTEGER PRIMARY KEY,
                hash TEXT NOT NULL,
                name TEXT NOT NULL
            )",
        [],
    )?;

    println!("Database path: {:?}", db_path.display());

    match db.close() {
        Ok(_) => (),
        Err(e) => eprintln!("Failed to close database: {:?}", e),
    }

    let addr = format!("[::1]:{}", port).parse()?;
    let packager = service::Package::default();

    println!("Build listening on: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(packager))
        .serve(addr)
        .await?;

    Ok(())
}
