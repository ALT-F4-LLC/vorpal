use anyhow::Result;
use std::fs;
use tonic::transport::Server;
use vorpal::api::package_service_server::PackageServiceServer;
use vorpal::database;
use vorpal::notary;
use vorpal::service::Packager;
use vorpal::store;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:15323".parse()?;
    let packager = Packager::default();

    let vorpal_dir = store::get_home_dir();
    if !vorpal_dir.exists() {
        std::fs::create_dir_all(&vorpal_dir)?;
    }

    let private_key = store::get_private_key_path();
    let public_key = store::get_public_key_path();
    if !private_key.exists() && !public_key.exists() {
        let key_dir = store::get_key_dir();
        fs::create_dir_all(&key_dir)?;
        notary::generate_keys(private_key.clone(), public_key.clone())?;
    }

    let db_path = store::get_database_path();
    let db = database::connect(db_path)?;

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

    println!("Server listening on: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(packager))
        .serve(addr)
        .await?;

    Ok(())
}
