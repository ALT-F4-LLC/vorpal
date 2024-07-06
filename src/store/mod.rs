use anyhow::Result;
use tokio::fs;
use tracing::info;

pub mod archives;
pub mod hashes;
pub mod paths;
pub mod temps;

pub async fn check() -> Result<(), anyhow::Error> {
    let root_dir = paths::get_root_dir_path();
    if !root_dir.exists() {
        fs::create_dir_all(&root_dir).await?;
    }

    info!("root directory: {}", root_dir.display());

    let store_dir = paths::get_store_dir_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("store directory: {:?}", store_dir);

    let key_dir = paths::get_key_dir_path();
    if !key_dir.exists() {
        fs::create_dir_all(&key_dir).await?;
    }

    info!("key directory: {:?}", key_dir);

    let image_dir = paths::get_image_dir_path();
    if !image_dir.exists() {
        fs::create_dir_all(&image_dir).await?;
    }

    info!("image directory: {:?}", image_dir);

    let package_dir = paths::get_package_dir_path();
    if !package_dir.exists() {
        fs::create_dir_all(&package_dir).await?;
    }

    info!("package directory: {:?}", package_dir);

    let store_dir = paths::get_store_dir_path();
    if !store_dir.exists() {
        fs::create_dir_all(&store_dir).await?;
    }

    info!("store directory: {:?}", store_dir);

    Ok(())
}
