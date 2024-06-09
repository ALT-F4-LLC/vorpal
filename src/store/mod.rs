use anyhow::Result;
use async_compression::tokio::{bufread::GzipDecoder, write::GzipEncoder};
use sha256::{digest, try_digest};
use std::env;
use std::ffi::OsStr;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio_tar::Archive;
use tokio_tar::Builder;
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

pub fn get_temp_path() -> PathBuf {
    env::temp_dir().join(Uuid::now_v7().to_string())
}

pub fn get_store_dir_name(name: &str, hash: &str) -> String {
    format!("{}-{}", name, hash)
}

pub async fn create_temp_dir() -> Result<PathBuf> {
    let temp_dir = get_temp_path();
    fs::create_dir(&temp_dir).await?;
    Ok(temp_dir)
}

pub async fn create_temp_file(extension: &str) -> Result<PathBuf> {
    let temp_file = get_temp_path().with_extension(extension);
    File::create(&temp_file).await?;
    Ok(temp_file)
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

pub fn get_file_paths<'a, P, I, J>(source: P, ignore_paths: I) -> Result<Vec<PathBuf>>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = &'a J>,
    J: AsRef<OsStr> + 'a,
{
    let source_ignore_paths = ignore_paths
        .into_iter()
        .map(|i| Path::new(i).to_path_buf())
        .collect::<Vec<PathBuf>>();

    let mut files: Vec<PathBuf> = WalkDir::new(&source)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if source_ignore_paths
                .iter()
                .any(|i| path.strip_prefix(&source).unwrap().starts_with(i))
            {
                return None;
            }
            path.canonicalize().ok()
        })
        .collect();

    files.sort();

    Ok(files)
}

pub fn get_file_hash<P: AsRef<Path> + Send>(path: P) -> Result<String, anyhow::Error> {
    if !path.as_ref().is_file() {
        return Err(anyhow::anyhow!("Path is not a file"));
    }

    Ok(try_digest(path)?)
}

pub fn get_file_hashes<'a, P, I>(files: I) -> Result<Vec<(&'a Path, String)>>
where
    P: AsRef<Path> + Send + Sync + 'a,
    I: IntoIterator<Item = &'a P>,
{
    let hashes = files
        .into_iter()
        .filter(|file| file.as_ref().is_file())
        .map(|file| {
            let hash = get_file_hash(file).unwrap();
            (file.as_ref(), hash)
        })
        .collect();

    Ok(hashes)
}

pub fn get_source_hash<'a, P, I>(hashes: I) -> Result<String>
where
    P: AsRef<Path> + 'a,
    I: IntoIterator<Item = &'a (P, String)>,
{
    let mut combined = String::new();

    for (_, hash) in hashes {
        combined.push_str(hash);
    }

    Ok(digest(combined))
}

pub async fn compress_tar_gz<'a, P1, P2, P3, I>(
    source: P1,
    source_output: P2,
    source_files: I,
) -> Result<File, anyhow::Error>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    P3: AsRef<Path> + 'a,
    I: IntoIterator<Item = &'a P3>,
{
    let tar = File::create(source_output).await?;
    let tar_encoder = GzipEncoder::new(tar);
    let mut tar_builder = Builder::new(tar_encoder);

    let source = source.as_ref();
    info!("Compressing: {}", source.display());

    for path in source_files {
        let path = path.as_ref();

        if path == source {
            continue;
        }

        let relative_path = path.strip_prefix(source)?;

        info!("Adding: {}", relative_path.display());

        if path.is_file() {
            tar_builder
                .append_path_with_name(path, relative_path)
                .await?;
        } else if path.is_dir() {
            tar_builder.append_dir_all(relative_path, path).await?;
        }
    }

    let mut output = tar_builder.into_inner().await?;

    output.shutdown().await?;

    Ok(output.into_inner())
}

pub async fn unpack_tar_gz(target_dir: &PathBuf, source_tar: &Path) -> Result<(), anyhow::Error> {
    let tar_gz = File::open(source_tar).await?;
    let buf_reader = BufReader::new(tar_gz);
    let gz_decoder = GzipDecoder::new(buf_reader);
    let mut archive = Archive::new(gz_decoder);
    Ok(archive.unpack(target_dir).await?)
}

pub async fn set_files_permissions(files: &[PathBuf]) -> Result<(), anyhow::Error> {
    for file in files {
        let permissions = fs::metadata(file).await?;
        if permissions.mode() & 0o111 != 0 {
            fs::set_permissions(file, std::fs::Permissions::from_mode(0o555)).await?;
        } else {
            fs::set_permissions(file, std::fs::Permissions::from_mode(0o444)).await?;
        }
    }

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
