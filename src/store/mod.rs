use anyhow::Result;
use tokio::fs;
use tracing::info;

pub mod archives;
pub mod hashes;
pub mod paths;
pub mod temps;

pub async fn check_dirs() -> Result<(), anyhow::Error> {
    let root_dir_path = paths::get_root_path();
    if !root_dir_path.exists() {
        fs::create_dir_all(&root_dir_path).await?;
    }

    info!("root directory: {}", root_dir_path.display());

    let store_dir_path = paths::get_store_path();
    if !store_dir_path.exists() {
        fs::create_dir_all(&store_dir_path).await?;
    }

    info!("store directory: {:?}", store_dir_path);

    Ok(())
}
