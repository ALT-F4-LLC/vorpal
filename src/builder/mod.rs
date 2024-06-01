use crate::api::package_service_server::PackageServiceServer;
use crate::database;
use crate::notary;
use crate::store;
use anyhow::Result;
use std::fs;
use tonic::transport::Server;
mod build;
mod prepare;
pub mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    let vorpal_dir = store::get_home_dir();
    if !vorpal_dir.exists() {
        std::fs::create_dir_all(&vorpal_dir)?;
    }

    let store_dir = store::get_store_dir();
    if !store_dir.exists() {
        std::fs::create_dir_all(&store_dir)?;
    }

    if !store::get_private_key_path().exists() && !store::get_public_key_path().exists() {
        let key_dir = store::get_key_dir();
        fs::create_dir_all(&key_dir)?;
        notary::generate_keys()?;
    }

    let db = database::connect(store::get_database_path())?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS source (
                id  INTEGER PRIMARY KEY,
                uri TEXT NOT NULL
            )",
        [],
    )?;

    match db.close() {
        Ok(_) => (),
        Err(e) => eprintln!("Failed to close database: {:?}", e),
    }

    let addr = format!("[::1]:{}", port).parse()?;
    let packager = service::Packager::default();

    println!("Server listening on: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(packager))
        .serve(addr)
        .await?;

    Ok(())
}
