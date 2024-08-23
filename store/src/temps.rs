use crate::paths;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;
use tokio::fs::File;

pub async fn create_temp_dir() -> Result<PathBuf> {
    let temp_dir_path = paths::get_temp_dir_path();
    fs::create_dir(&temp_dir_path).await?;
    Ok(temp_dir_path)
}

pub async fn create_temp_file(extension: &str) -> Result<PathBuf> {
    let temp_file_path = paths::get_temp_dir_path().with_extension(extension);
    File::create(&temp_file_path).await?;
    Ok(temp_file_path)
}
