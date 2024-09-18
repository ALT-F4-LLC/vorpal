use anyhow::{bail, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::fs::set_permissions;
use tokio::fs::{copy, create_dir_all, metadata, write, File};
use tokio::io::AsyncReadExt;
use tracing::info;
use uuid::Uuid;
use walkdir::WalkDir;

// Store paths

pub fn get_store_dir_name(hash: &str, name: &str) -> String {
    format!("{}-{}", name, hash)
}

pub fn get_root_dir_path() -> PathBuf {
    Path::new("/var/lib/vorpal").to_path_buf()
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

// Input paths - "/var/lib/vorpal/store/{hash}.input"

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

// Package paths - "/var/lib/vorpal/store/{hash}.package"

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

// Source paths - "/var/lib/vorpal/store/{hash}.source"

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
            copy(src, dest).await.expect("copy symlink fail");
        } else {
            bail!("source file is not a file or directory: {:?}", src);
        }
    }

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

async fn update_file(path: &str, next: &str) -> Result<()> {
    // Get current file permissions
    let metadata = metadata(path).await.expect("failed to get file metadata");
    let mut permissions = metadata.permissions();
    let was_read_only = permissions.mode() & 0o222 == 0;

    // If read-only, set to writable
    if was_read_only {
        permissions.set_mode(permissions.mode() | 0o200); // Add write permission
        set_permissions(path, permissions.clone())
            .await
            .expect("failed to set file permissions");
    }

    // Write to the file
    write(path, next).await.expect("failed to write file");

    // Set permissions back to read-only if they were initially read-only
    if was_read_only {
        permissions.set_mode(permissions.mode() & !0o200); // Remove write permission
        set_permissions(path, permissions)
            .await
            .expect("failed to set file permissions");
    }

    Ok(())
}

pub async fn replace_path_in_files(from_path: &Path, to_path: &Path) -> Result<()> {
    let from = from_path.display().to_string();
    let to = to_path.display().to_string();

    for entry in WalkDir::new(from_path) {
        let entry = entry.expect("failed to read entry");

        // TODO: handle rpath changes for binaries

        if entry.file_type().is_file() {
            let path = entry.path();
            let mut file = File::open(path).await.expect("failed to open file");
            let mut content = Vec::new();

            file.read_to_end(&mut content)
                .await
                .expect("failed to read file");

            if let Ok(prev) = String::from_utf8(content) {
                let next = prev.replace(&from, &to);

                if next != prev {
                    update_file(path.display().to_string().as_str(), next.as_str())
                        .await
                        .expect("failed to update file");
                }
            } else {
                continue;
            }
        }
    }

    Ok(())
}
