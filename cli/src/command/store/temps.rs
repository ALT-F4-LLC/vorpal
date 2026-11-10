use crate::command::store::paths;
use anyhow::{anyhow, Result};
use std::{
    fs::{DirBuilder, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use tokio::fs::{create_dir_all, File};

/// Creates the directory `path`, owner-only (0700) from the creating call on, so it is
/// never reachable at a wider mode. Fails if `path` already exists.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    DirBuilder::new().mode(0o700).create(path)
}

/// Creates the file `path` for writing, owner-only (0600) from the creating call on.
/// Fails if `path` already exists, including as a symlink.
pub fn create_private_file(path: &Path) -> io::Result<std::fs::File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

pub async fn create_sandbox_dir() -> Result<PathBuf> {
    let dir_path = paths::get_sandbox_path();

    create_dir_all(&dir_path)
        .await
        .map_err(|e| anyhow!("failed to create temp dir: {e}"))?;

    Ok(dir_path)
}

pub async fn create_sandbox_file(extension: Option<&str>) -> Result<PathBuf> {
    let mut file_path = paths::get_sandbox_path();

    if let Some(extension) = extension {
        file_path = file_path.with_extension(extension);
    }

    File::create(&file_path)
        .await
        .map_err(|e| anyhow!("failed to create temp file: {e}"))?;

    Ok(file_path)
}
