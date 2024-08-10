use anyhow::Result;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tokio::fs::{copy, create_dir_all};
use tracing::info;
use uuid::Uuid;
use walkdir::WalkDir;

pub fn get_package_name(name: &str, hash: &str) -> String {
    format!("{}-{}", name, hash)
}

pub fn get_root_path() -> PathBuf {
    Path::new("/var/lib/vorpal").to_path_buf()
}

pub fn get_key_path() -> PathBuf {
    get_root_path().join("key")
}

pub fn get_sandbox_path() -> PathBuf {
    get_root_path().join("sandbox")
}

pub fn get_store_path() -> PathBuf {
    get_root_path().join("store")
}

pub fn get_private_key_path() -> PathBuf {
    get_key_path().join("private").with_extension("pem")
}

pub fn get_public_key_path() -> PathBuf {
    get_key_path().join("public").with_extension("pem")
}

pub fn get_package_path(name: &str, hash: &str) -> PathBuf {
    get_store_path().join(get_package_name(name, hash))
}

pub fn get_package_archive_path(name: &str, hash: &str) -> PathBuf {
    get_package_path(name, hash).with_extension("tar.zst")
}

pub fn get_source_path(name: &str, hash: &str) -> PathBuf {
    get_package_path(name, hash).with_extension("source")
}

pub fn get_source_archive_path(name: &str, hash: &str) -> PathBuf {
    get_package_path(name, hash).with_extension("source.tar.zst")
}

pub fn get_temp_path() -> PathBuf {
    get_sandbox_path().join(Uuid::now_v7().to_string())
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
            Some(path.to_path_buf())
        })
        .collect();

    files.sort();

    Ok(files)
}

pub async fn copy_files(
    source_path: &PathBuf,
    destination_path: &Path,
) -> Result<(), anyhow::Error> {
    let file_paths = get_file_paths(source_path, &Vec::<&str>::new())
        .map_err(|e| anyhow::anyhow!("failed to get source files: {:?}", e))?;

    if file_paths.is_empty() {
        return Err(anyhow::anyhow!("no source files found"));
    }

    for src in &file_paths {
        if src.is_dir() {
            let dest = destination_path.join(src.strip_prefix(source_path).unwrap());
            create_dir_all(dest).await?;
            continue;
        }

        let dest = destination_path.join(src.strip_prefix(source_path).unwrap());

        copy(src, dest).await?;
    }

    Ok(())
}

pub async fn setup_paths() -> Result<(), anyhow::Error> {
    let key_path = get_key_path();
    if !key_path.exists() {
        create_dir_all(&key_path).await?;
    }

    info!("keys path: {:?}", key_path);

    let sandbox_path = get_sandbox_path();
    if !sandbox_path.exists() {
        create_dir_all(&sandbox_path).await?;
    }

    info!("sandbox path: {:?}", sandbox_path);

    let store_path = get_store_path();
    if !store_path.exists() {
        create_dir_all(&store_path).await?;
    }

    info!("store path: {:?}", store_path);

    Ok(())
}
