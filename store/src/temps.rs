use crate::paths;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs::{create_dir_all, File};

pub async fn create_temp_dir() -> Result<PathBuf> {
    let dir_path = paths::get_temp_path();

    create_dir_all(&dir_path)
        .await
        .expect("Failed to create temp dir");

    Ok(dir_path)
}

pub async fn create_temp_file(extension: Option<&str>) -> Result<PathBuf> {
    let mut file_path = paths::get_temp_path();

    if let Some(extension) = extension {
        file_path = file_path.with_extension(extension);
    }

    File::create(&file_path)
        .await
        .expect("Failed to create temp file");

    Ok(file_path)
}
