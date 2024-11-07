use anyhow::{bail, Error, Result};
use filetime::{set_file_times, FileTime};
use std::path::{Path, PathBuf};
use tokio::fs::{copy, create_dir_all, metadata, symlink};
use tracing::info;
use uuid::Uuid;
use walkdir::WalkDir;

// Store paths

pub fn get_store_dir_name(hash: &str, name: &str) -> String {
    format!("{}-{}", name, hash)
}

pub fn get_root_dir_path() -> PathBuf {
    Path::new("/vorpal").to_path_buf()
}

pub fn get_key_dir_path() -> PathBuf {
    get_root_dir_path().join("key")
}

pub fn get_sandbox_dir_path() -> PathBuf {
    get_root_dir_path().join("sandbox")
}

pub fn get_store_dir_path() -> PathBuf {
    get_root_dir_path().join("store")
}

// Key paths

pub fn get_private_key_path() -> PathBuf {
    get_key_dir_path().join("private").with_extension("pem")
}

pub fn get_public_key_path() -> PathBuf {
    get_key_dir_path().join("public").with_extension("pem")
}

// Input paths - "/vorpal/store/{hash}.input"

pub fn get_input_path(hash: &str, name: &str) -> PathBuf {
    get_store_dir_path()
        .join(get_store_dir_name(hash, name))
        .with_extension("input")
}

pub fn get_input_archive_path(hash: &str, name: &str) -> PathBuf {
    get_store_dir_path()
        .join(get_store_dir_name(hash, name))
        .with_extension("input.tar.zst")
}

// Package paths - "/vorpal/store/{hash}.package"

pub fn get_package_path(hash: &str, name: &str) -> PathBuf {
    get_store_dir_path()
        .join(get_store_dir_name(hash, name))
        .with_extension("package")
}

pub fn get_package_archive_path(hash: &str, name: &str) -> PathBuf {
    get_store_dir_path()
        .join(get_store_dir_name(hash, name))
        .with_extension("package.tar.zst")
}

pub fn get_package_lock_path(hash: &str, name: &str) -> PathBuf {
    get_store_dir_path()
        .join(get_store_dir_name(hash, name))
        .with_extension("package.lock")
}

// Source paths - "/vorpal/store/{hash}.source"

pub fn get_source_path(hash: &str, name: &str) -> PathBuf {
    get_store_dir_path()
        .join(get_store_dir_name(hash, name))
        .with_extension("source")
}

pub fn get_source_archive_path(hash: &str, name: &str) -> PathBuf {
    get_store_dir_path()
        .join(get_store_dir_name(hash, name))
        .with_extension("source.tar.zst")
}

// Temp paths

pub fn get_temp_path() -> PathBuf {
    get_sandbox_dir_path().join(Uuid::now_v7().to_string())
}

pub fn get_file_paths(
    source_path: &PathBuf,
    excludes: Vec<String>,
    includes: Vec<String>,
) -> Result<Vec<PathBuf>> {
    let excludes_paths = excludes
        .into_iter()
        .map(|i| Path::new(&i).to_path_buf())
        .collect::<Vec<PathBuf>>();

    let mut files: Vec<PathBuf> = WalkDir::new(source_path)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();

            if excludes_paths
                .iter()
                .any(|i| path.strip_prefix(source_path).unwrap().starts_with(i))
            {
                return None;
            }

            Some(path.to_path_buf())
        })
        .collect();

    let includes_paths = includes
        .into_iter()
        .map(|i| Path::new(&i).to_path_buf())
        .collect::<Vec<PathBuf>>();

    if !includes_paths.is_empty() {
        files.retain(|i| {
            includes_paths
                .iter()
                .any(|j| i.strip_prefix(source_path).unwrap().starts_with(j))
        });
    }

    files.sort();

    if files.is_empty() {
        bail!("no files found");
    }

    Ok(files)
}

pub async fn set_paths_timestamps(target_files: &[PathBuf]) -> Result<(), Error> {
    for path in target_files {
        let epoc = FileTime::from_unix_time(0, 0);
        set_file_times(path, epoc, epoc).expect("Failed to set file times");
    }

    Ok(())
}

pub async fn copy_files(
    source_path: &PathBuf,
    source_path_files: Vec<PathBuf>,
    destination_path: &Path,
) -> Result<()> {
    if source_path_files.is_empty() {
        bail!("no source files found");
    }

    for src in &source_path_files {
        if src.display().to_string().ends_with(".tar.zst") {
            bail!("source file is a tar.zst archive");
        }

        if !src.exists() {
            bail!("source file not found: {:?}", src);
        }

        let metadata = metadata(src).await.expect("failed to read metadata");

        let dest = destination_path.join(src.strip_prefix(source_path).unwrap());

        if metadata.is_dir() {
            create_dir_all(dest).await.expect("create directory fail");
        } else if metadata.is_file() {
            let parent = dest.parent().expect("failed to get parent directory");
            if !parent.exists() {
                create_dir_all(parent)
                    .await
                    .expect("create parent directory fail");
            }

            copy(src, dest).await.expect("copy file fail");
        } else if metadata.is_symlink() {
            symlink(src, dest).await.expect("symlink file fail");
        } else {
            bail!("source file is not a file or directory: {:?}", src);
        }
    }

    let package_paths = get_file_paths(&destination_path.to_path_buf(), vec![], vec![])?;

    set_paths_timestamps(&package_paths).await?;

    Ok(())
}

pub async fn setup_paths() -> Result<()> {
    let key_path = get_key_dir_path();
    if !key_path.exists() {
        create_dir_all(&key_path)
            .await
            .expect("failed to create key directory");
    }

    info!("keys path: {:?}", key_path);

    let sandbox_path = get_sandbox_dir_path();
    if !sandbox_path.exists() {
        create_dir_all(&sandbox_path)
            .await
            .expect("failed to create sandbox directory");
    }

    info!("sandbox path: {:?}", sandbox_path);

    let store_path = get_store_dir_path();
    if !store_path.exists() {
        create_dir_all(&store_path)
            .await
            .expect("failed to create store directory");
    }

    info!("store path: {:?}", store_path);

    Ok(())
}
