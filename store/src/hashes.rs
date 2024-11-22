use crate::paths::get_file_paths;
use anyhow::{bail, Result};
use sha256::{digest, try_digest};
use std::path::{Path, PathBuf};
use vorpal_schema::vorpal::artifact::v0::ArtifactSource;

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

pub async fn hash_files(paths: Vec<PathBuf>) -> Result<String> {
    if paths.is_empty() {
        anyhow::bail!("no source files found")
    }

    let paths_hashes = get_file_hashes(&paths)?;

    let paths_hashes_joined = get_hashes_digest(paths_hashes)?;

    Ok(paths_hashes_joined)
}

pub async fn get_artifact_hash(config_hash: &str, source: &[ArtifactSource]) -> Result<String> {
    let mut source_hashes = vec![config_hash.to_string()];

    for source in source.iter() {
        let path = Path::new(&source.path).to_path_buf();

        if !path.exists() {
            bail!(
                "Artifact `source.{}.path` not found: {:?}",
                source.name,
                path
            );
        }

        let source_files = get_file_paths(&path, source.excludes.clone(), source.includes.clone())?;

        let source_hash = hash_files(source_files).await?;

        // if let Some(hash) = source.hash.clone() {
        //     if hash != source_hash {
        //         bail!(
        //             "Artifact `source.{}.hash` mismatch: {} != {}",
        //             source.name,
        //             hash,
        //             source_hash
        //         );
        //     }
        // }

        source_hashes.push(source_hash);
    }

    let artifact_hash = get_hashes_digest(source_hashes)?;

    Ok(artifact_hash)
}

pub fn get_hash_digest(hash: &str) -> String {
    digest(hash)
}
