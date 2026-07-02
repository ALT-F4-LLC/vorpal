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

pub fn get_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("VORPAL_SOCKET_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    get_root_dir_path().join("vorpal.sock")
}

pub fn get_lock_path() -> PathBuf {
    let socket_path = get_socket_path();
    let lock_name = socket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vorpal");
    socket_path.with_file_name(format!("{}.lock", lock_name))
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

    if path.is_symlink() {
        set_symlink_file_times(path, epoc, epoc).map_err(|e| {
            anyhow::anyhow!("failed to set symlink file times for {:?}: {}", path, e)
        })?;
    } else {
        // Ensure the file/directory is writable before modifying timestamps.
        // Extracted tar entries (e.g. Rust toolchain) may have read-only
        // permissions which cause set_file_times to fail with PermissionDenied.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(path) {
                let mode = meta.permissions().mode();
                if mode & 0o200 == 0 {
                    let mut perms = meta.permissions();
                    perms.set_mode(mode | 0o200);
                    std::fs::set_permissions(path, perms).map_err(|e| {
                        anyhow::anyhow!("failed to add write permission for {:?}: {}", path, e)
                    })?;
                }
            }
        }

        set_file_times(path, epoc, epoc)
            .map_err(|e| anyhow::anyhow!("failed to set file times for {:?}: {}", path, e))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    fn file_basenames(root: &Path, paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .filter(|p| p.is_file())
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn make_dir_with_files(names: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for name in names {
            File::create(dir.path().join(name)).unwrap();
        }
        dir
    }

    // A case-insensitive or locale-aware sort would order these ["a", "B", "Z"];
    // cross-host digest stability requires the raw byte order 'B'(0x42) < 'Z'(0x5A)
    // < 'a'(0x61), independent of the host's collation.
    #[test]
    fn get_file_paths_sorts_bytewise_not_case_folded() {
        let dir = make_dir_with_files(&["a.txt", "B.txt", "Z.txt"]);

        let paths = get_file_paths(&dir.path().to_path_buf(), vec![], vec![]).unwrap();

        assert_eq!(
            file_basenames(dir.path(), &paths),
            vec!["B.txt", "Z.txt", "a.txt"]
        );
    }

    // Simulates two host filesystems enumerating the identical file set in different
    // orders (APFS vs ext4 dirent order): the final sort must normalize both to the
    // same sequence, otherwise the combined source digest diverges across producers.
    #[test]
    fn get_file_paths_order_independent_of_creation_order() {
        let forward = make_dir_with_files(&["alpha", "bravo", "charlie"]);
        let reversed = make_dir_with_files(&["charlie", "bravo", "alpha"]);

        let forward_paths = get_file_paths(&forward.path().to_path_buf(), vec![], vec![]).unwrap();
        let reversed_paths =
            get_file_paths(&reversed.path().to_path_buf(), vec![], vec![]).unwrap();

        assert_eq!(
            file_basenames(forward.path(), &forward_paths),
            file_basenames(reversed.path(), &reversed_paths)
        );
    }

    // Non-decomposable codepoints (Greek alpha U+03B1, Euro U+20AC) avoid APFS
    // NFC/NFD rewriting; their UTF-8 encodings sort by raw byte value
    // 'z'(0x7A) < α(0xCE..) < €(0xE2..) on any host.
    #[test]
    fn get_file_paths_orders_unicode_bytewise() {
        let dir = make_dir_with_files(&["z_ascii", "\u{03b1}_alpha", "\u{20ac}_euro"]);

        let paths = get_file_paths(&dir.path().to_path_buf(), vec![], vec![]).unwrap();

        assert_eq!(
            file_basenames(dir.path(), &paths),
            vec!["z_ascii", "\u{03b1}_alpha", "\u{20ac}_euro"]
        );
    }
}
