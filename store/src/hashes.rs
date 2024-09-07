use anyhow::Result;
use sha256::{digest, try_digest};
use std::path::{Path, PathBuf};

pub fn get_file_hash<P: AsRef<Path> + Send>(path: P) -> Result<String> {
    if !path.as_ref().is_file() {
        return Err(anyhow::anyhow!("Path is not a file"));
    }

    Ok(try_digest(path).expect("Failed to get file hash"))
}

pub fn get_file_hashes(files: &[PathBuf]) -> Result<Vec<String>> {
    let hashes = files
        .iter()
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

pub async fn hash_files(paths: Vec<PathBuf>) -> Result<(String, Vec<PathBuf>)> {
    if paths.is_empty() {
        anyhow::bail!("no source files found")
    }

    let paths_hashes = get_file_hashes(&paths)?;

    let paths_hashes_joined = get_hashes_digest(paths_hashes)?;

    Ok((paths_hashes_joined, paths))
}
