use anyhow::Result;
use tokio::fs;
use tracing::info;

pub mod archives;
pub mod hashes;
pub mod paths;
pub mod temps;

pub async fn check() -> Result<(), anyhow::Error> {
    let key_dir = paths::get_key_path();
    if !key_dir.exists() {
        fs::create_dir_all(&key_dir).await?;
    }

    info!("key directory: {:?}", key_dir);

    let store_dir = paths::get_store_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("store directory: {:?}", store_dir);

    Ok(())
}
