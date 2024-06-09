use crate::store::paths;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;
use tokio::fs::File;

pub async fn create_dir() -> Result<PathBuf> {
    let temp_dir_path = paths::get_temp_path();
    fs::create_dir(&temp_dir_path).await?;
    Ok(temp_dir_path)
}

pub async fn create_file(extension: &str) -> Result<PathBuf> {
    let temp_file_path = paths::get_temp_path().with_extension(extension);
    File::create(&temp_file_path).await?;
    Ok(temp_file_path)
}
