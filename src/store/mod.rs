use anyhow::Result;
use tokio::fs;
use tracing::info;

pub mod archives;
pub mod hashes;
pub mod paths;
pub mod temps;

pub async fn check() -> Result<(), anyhow::Error> {
    let key_path = paths::get_key_path();
    if !key_path.exists() {
        fs::create_dir_all(&key_path).await?;
    }

    info!("keys path: {:?}", key_path);

    let sandbox_path = paths::get_sandbox_path();
    if !sandbox_path.exists() {
        fs::create_dir_all(&sandbox_path).await?;
    }

    info!("sandbox path: {:?}", sandbox_path);

    let store_dir = paths::get_store_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("store path: {:?}", store_dir);

    Ok(())
}
