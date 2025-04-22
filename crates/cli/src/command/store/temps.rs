use crate::command::store::paths;
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tokio::fs::{create_dir_all, File};

pub async fn create_sandbox_dir() -> Result<PathBuf> {
    let dir_path = paths::get_sandbox_path();

    create_dir_all(&dir_path)
        .await
        .map_err(|e| anyhow!("failed to create temp dir: {}", e))?;

    Ok(dir_path)
}

pub async fn create_sandbox_file(extension: Option<&str>) -> Result<PathBuf> {
    let mut file_path = paths::get_sandbox_path();

    if let Some(extension) = extension {
        file_path = file_path.with_extension(extension);
    }

    File::create(&file_path)
        .await
        .map_err(|e| anyhow!("failed to create temp file: {}", e))?;

    Ok(file_path)
}
