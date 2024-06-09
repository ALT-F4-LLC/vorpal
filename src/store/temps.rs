use crate::store::paths;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;
use tokio::fs::File;

pub async fn create_dir() -> Result<PathBuf> {
    let temp_dir = paths::get_temp();
    fs::create_dir(&temp_dir).await?;
    Ok(temp_dir)
}

pub async fn create_file(extension: &str) -> Result<PathBuf> {
    let temp_file = paths::get_temp().with_extension(extension);
    File::create(&temp_file).await?;
    Ok(temp_file)
}
