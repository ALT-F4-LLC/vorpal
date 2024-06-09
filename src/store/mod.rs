use anyhow::Result;
use tokio::fs;
use tracing::info;

pub mod archives;
pub mod files;
pub mod hashes;
pub mod paths;
pub mod temps;

pub async fn init() -> Result<(), anyhow::Error> {
    let root_dir = paths::get_root();
    if !root_dir.exists() {
        fs::create_dir_all(&root_dir).await?;
    }

    info!("root directory: {}", root_dir.display());

    let store_dir = paths::get_store();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("store directory: {:?}", store_dir);

    Ok(())
}
