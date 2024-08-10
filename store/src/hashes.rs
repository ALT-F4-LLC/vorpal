use crate::paths::get_file_paths;
use anyhow::Result;
use sha256::{digest, try_digest};
use std::path::{Path, PathBuf};

pub fn get_file_hash<P: AsRef<Path> + Send>(path: P) -> Result<String, anyhow::Error> {
    if !path.as_ref().is_file() {
        return Err(anyhow::anyhow!("Path is not a file"));
    }

    Ok(try_digest(path)?)
}

pub fn get_file_hashes(files: &Vec<PathBuf>) -> Result<Vec<String>> {
    let hashes = files
        .into_iter()
        .filter(|file| file.is_file())
        .map(|file| get_file_hash(file).unwrap())
        .collect();

    Ok(hashes)
}

pub fn get_hashes_digest(hashes: Vec<String>) -> Result<String> {
    let mut combined = String::new();

    for hash in hashes {
        combined.push_str(&hash);
    }

    Ok(digest(combined))
}

pub async fn hash_files(
    path: &Path,
    ignores: &Vec<String>,
) -> Result<(String, Vec<PathBuf>), anyhow::Error> {
    let workdir_files = get_file_paths(path, ignores)?;

    if workdir_files.is_empty() {
        anyhow::bail!("no source files found")
    }

    let workdir_files_hashes = get_file_hashes(&workdir_files)?;
    let workdir_hash = get_hashes_digest(workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        anyhow::bail!("no source hash found")
    }

    Ok((workdir_hash, workdir_files))
}
