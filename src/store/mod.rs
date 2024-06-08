use anyhow::Result;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha256::{digest, try_digest};
use std::env;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tar::Archive;
use tar::Builder;
use tracing::info;
use uuid::Uuid;
use walkdir::WalkDir;

pub fn get_home_dir_path() -> PathBuf {
    dirs::home_dir()
        .expect("Home directory not found")
        .join(".vorpal")
}

pub fn get_key_dir_path() -> PathBuf {
    get_home_dir_path().join("key")
}

pub fn get_store_dir_path() -> PathBuf {
    get_home_dir_path().join("store")
}

pub fn get_database_path() -> PathBuf {
    get_home_dir_path().join("vorpal.db")
}

pub fn get_private_key_path() -> PathBuf {
    get_key_dir_path().join("private").with_extension("pem")
}

pub fn get_public_key_path() -> PathBuf {
    get_key_dir_path().join("public").with_extension("pem")
}

pub fn get_temp_dir_path() -> PathBuf {
    env::temp_dir().join(Uuid::now_v7().to_string())
}

pub fn get_store_dir_name(name: &str, hash: &str) -> String {
    format!("{}-{}", name, hash)
}

pub fn get_source_tar_path(source_name: &str, source_hash: &str) -> PathBuf {
    let store_dir = get_store_dir_path();
    let store_dir_name = get_store_dir_name(source_name, source_hash);
    store_dir
        .join(store_dir_name)
        .with_extension("source.tar.gz")
}

pub fn get_source_dir_path(source_name: &str, source_hash: &str) -> PathBuf {
    let store_dir = get_store_dir_path();
    store_dir
        .join(get_store_dir_name(source_name, source_hash))
        .with_extension("source")
        .to_path_buf()
}

pub fn get_file_paths(source: &PathBuf, ignore_paths: &[String]) -> Result<Vec<PathBuf>> {
    let source_ignore_paths = ignore_paths
        .iter()
        .map(|i| Path::new(i).to_path_buf())
        .collect::<Vec<PathBuf>>();

    let mut files: Vec<PathBuf> = WalkDir::new(source)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if source_ignore_paths
                .iter()
                .any(|i| path.strip_prefix(source).unwrap().starts_with(i))
            {
                return None;
            }
            path.canonicalize().ok()
        })
        .collect();

    files.sort();

    Ok(files)
}

pub fn get_file_hash(path: &Path) -> Result<String, anyhow::Error> {
    if !path.is_file() {
        return Err(anyhow::anyhow!("Path is not a file"));
    }

    Ok(try_digest(path)?)
}

pub fn get_file_hashes(files: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    let hashes: Vec<(PathBuf, String)> = files
        .iter()
        .filter(|file| file.is_file())
        .map(|file| {
            let hash = get_file_hash(file).unwrap();
            (file.clone(), hash)
        })
        .collect();

    Ok(hashes)
}

pub fn get_source_hash(hashes: &[(PathBuf, String)]) -> Result<String> {
    let mut combined = String::new();

    for (_, hash) in hashes {
        combined.push_str(hash);
    }

    Ok(digest(combined))
}

pub fn compress_files(
    source: &PathBuf,
    source_output: &Path,
    source_files: &[PathBuf],
) -> Result<File, anyhow::Error> {
    let tar = File::create(source_output)?;
    let tar_encoder = GzEncoder::new(tar.try_clone()?, Compression::default());
    let mut tar_builder = Builder::new(tar_encoder);

    info!("Compressing: {}", source.display());

    for path in source_files {
        if path == source {
            continue;
        }

        let relative_path = path.strip_prefix(source)?;

        info!("Adding: {}", relative_path.display());

        if path.is_file() {
            tar_builder.append_path_with_name(path.clone(), relative_path)?;
        } else if path.is_dir() {
            tar_builder.append_dir(relative_path, path.clone())?;
        }
    }

    tar_builder.finish()?;

    Ok(tar)
}

pub fn set_files_permissions(files: &[PathBuf]) -> Result<(), anyhow::Error> {
    for file in files {
        let permissions = fs::metadata(file)?.permissions();
        if permissions.mode() & 0o111 != 0 {
            fs::set_permissions(file, fs::Permissions::from_mode(0o555))?;
        } else {
            fs::set_permissions(file, fs::Permissions::from_mode(0o444))?;
        }
    }

    Ok(())
}

pub fn unpack_source(target_dir: &PathBuf, source_tar: &Path) -> Result<(), anyhow::Error> {
    let tar_gz = File::open(source_tar)?;
    let buf_reader = BufReader::new(tar_gz);
    let gz_decoder = GzDecoder::new(buf_reader);
    let mut archive = Archive::new(gz_decoder);
    archive.unpack(target_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_dir_path() {
        assert_eq!(get_home_dir_path().file_name().unwrap(), ".vorpal");
    }

    #[test]
    fn key_dir_path() {
        assert_eq!(get_key_dir_path().file_name().unwrap(), "key");
    }

    #[test]
    fn key_dir_path_home() {
        let home_dir = get_home_dir_path();
        let key_dir = get_key_dir_path();
        assert!(key_dir.starts_with(home_dir));
    }

    #[test]
    fn store_dir_path() {
        assert_eq!(get_store_dir_path().file_name().unwrap(), "store");
    }

    #[test]
    fn store_dir_path_home() {
        let home_dir = get_home_dir_path();
        let store_dir = get_key_dir_path();
        assert!(store_dir.starts_with(home_dir));
    }
}
