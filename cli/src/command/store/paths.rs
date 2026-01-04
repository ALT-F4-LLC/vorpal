use anyhow::{bail, Error, Result};
use filetime::{set_file_times, set_symlink_file_times, FileTime};
use std::path::{Path, PathBuf};
use tokio::fs::{copy, create_dir_all, metadata, symlink};
use uuid::Uuid;
use vorpal_sdk::api::artifact::ArtifactSystem;
use walkdir::WalkDir;

// Root paths

pub fn get_root_dir_path() -> PathBuf {
    Path::new("/var/lib/vorpal").to_path_buf()
}

pub fn get_root_key_dir_path() -> PathBuf {
    get_root_dir_path().join("key")
}

pub fn get_root_sandbox_dir_path() -> PathBuf {
    get_root_dir_path().join("sandbox")
}

pub fn get_root_store_dir_path() -> PathBuf {
    get_root_dir_path().join("store")
}

// Key paths

pub fn get_key_ca_path() -> PathBuf {
    get_root_key_dir_path().join("ca").with_extension("pem")
}

pub fn get_key_ca_key_path() -> PathBuf {
    get_root_key_dir_path().join("ca").with_extension("key.pem")
}

pub fn get_key_credentials_path() -> PathBuf {
    get_root_key_dir_path()
        .join("credentials")
        .with_extension("json")
}

pub fn get_key_service_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("pem")
}

pub fn get_key_service_key_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("key.pem")
}

pub fn get_key_service_public_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("public.pem")
}

pub fn get_key_service_secret_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("secret")
}

// Artifact paths

pub fn get_artifact_dir_path() -> PathBuf {
    get_root_store_dir_path().join("artifact")
}

pub fn get_root_artifact_alias_dir_path() -> PathBuf {
    get_artifact_dir_path().join("alias")
}

pub fn get_artifact_alias_dir_path(namespace: &str, system: ArtifactSystem) -> PathBuf {
    get_root_artifact_alias_dir_path()
        .join(namespace)
        .join(system.as_str_name())
}

pub fn get_artifact_alias_path(
    name: &str,
    namespace: &str,
    system: ArtifactSystem,
    tag: &str,
) -> Result<PathBuf> {
    Ok(get_artifact_alias_dir_path(namespace, system)
        .join(name)
        .join(tag))
}

pub fn get_root_artifact_archive_dir_path() -> PathBuf {
    get_artifact_dir_path().join("archive")
}

pub fn get_artifact_archive_dir_path(namespace: &str) -> PathBuf {
    get_root_artifact_archive_dir_path().join(namespace)
}

pub fn get_artifact_archive_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_archive_dir_path(namespace)
        .join(digest)
        .with_extension("tar.zst")
}

pub fn get_root_artifact_config_dir_path() -> PathBuf {
    get_artifact_dir_path().join("config")
}

pub fn get_artifact_config_dir_path(namespace: &str) -> PathBuf {
    get_root_artifact_config_dir_path().join(namespace)
}

pub fn get_artifact_config_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_config_dir_path(namespace)
        .join(digest)
        .with_extension("json")
}

pub fn get_root_artifact_function_dir_path() -> PathBuf {
    get_artifact_dir_path().join("function")
}

pub fn get_artifact_function_dir_path(namespace: &str) -> PathBuf {
    get_root_artifact_function_dir_path().join(namespace)
}

pub fn get_artifact_function_path(name: &str, namespace: &str, tag: &str) -> PathBuf {
    get_artifact_function_dir_path(namespace)
        .join(name)
        .join(tag)
        .with_extension("json")
}

pub fn get_root_artifact_output_dir_path() -> PathBuf {
    get_artifact_dir_path().join("output")
}

pub fn get_artifact_output_dir_path(namespace: &str) -> PathBuf {
    get_root_artifact_output_dir_path().join(namespace)
}

pub fn get_artifact_output_lock_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_output_dir_path(namespace)
        .join(digest)
        .with_extension("lock.json")
}

pub fn get_artifact_output_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_output_dir_path(namespace).join(digest)
}

// Temp paths

pub fn get_sandbox_path() -> PathBuf {
    get_root_sandbox_dir_path().join(Uuid::now_v7().to_string())
}

// Functions

pub fn get_file_paths(
    source_path: &PathBuf,
    excludes: Vec<String>,
    includes: Vec<String>,
) -> Result<Vec<PathBuf>> {
    let mut excludes_paths = excludes
        .into_iter()
        .map(|i| Path::new(&i).to_path_buf())
        .collect::<Vec<PathBuf>>();

    // Exclude git directory

    excludes_paths.push(Path::new(".git").to_path_buf());

    // Resolve full path

    let walker = WalkDir::new(source_path);

    let mut files: Vec<PathBuf> = walker
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

pub async fn set_timestamps(path: &PathBuf) -> Result<(), Error> {
    let epoc = FileTime::from_unix_time(0, 0);

    if !path.is_symlink() {
        set_file_times(path, epoc, epoc).expect("Failed to set file times");
    }

    if path.is_symlink() {
        set_symlink_file_times(path, epoc, epoc).expect("Failed to set symlink file times");
    }

    Ok(())
}

pub async fn copy_files(
    source_path: &PathBuf,
    source_path_files: Vec<PathBuf>,
    target_path: &Path,
) -> Result<Vec<PathBuf>> {
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

        let dest = target_path.join(src.strip_prefix(source_path).unwrap());

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

    let target_path_files = get_file_paths(&target_path.to_path_buf(), vec![], vec![])?;

    Ok(target_path_files)
}
