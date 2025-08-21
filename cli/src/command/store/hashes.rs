use anyhow::Result;
use sha256::{digest, try_digest};
use std::path::{Path, PathBuf};

pub fn get_file_digest<P: AsRef<Path> + Send>(path: P) -> Result<String> {
    if !path.as_ref().is_file() {
        return Err(anyhow::anyhow!("Path is not a file"));
    }

    Ok(try_digest(path).expect("Failed to get file hash"))
}

pub fn get_files_digests(files: &[PathBuf]) -> Result<Vec<String>> {
    let hashes = files
        .iter()
        .filter(|file| file.is_file())
        .map(|file| get_file_digest(file).unwrap())
        .collect();

    Ok(hashes)
}

pub fn get_digests_digest(hashes: Vec<String>) -> Result<String> {
    let mut combined = String::new();

    for hash in hashes {
        combined.push_str(&hash);
    }

    Ok(digest(combined))
}

pub fn get_source_digest(paths: Vec<PathBuf>) -> Result<String> {
    if paths.is_empty() {
        anyhow::bail!("no source files found")
    }

    let paths_digests = get_files_digests(&paths)?;
    let paths_digest = get_digests_digest(paths_digests)?;

    Ok(paths_digest)
}
