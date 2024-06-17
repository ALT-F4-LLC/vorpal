use anyhow::Result;
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

pub fn get_root_path() -> PathBuf {
    dirs::home_dir()
        .expect("Home directory not found")
        .join(".vorpal")
}

pub fn get_key_path() -> PathBuf {
    get_root_path().join("key")
}

pub fn get_store_path() -> PathBuf {
    get_root_path().join("store")
}

pub fn get_database_path() -> PathBuf {
    get_root_path().join("vorpal.db")
}

pub fn get_private_key_path() -> PathBuf {
    get_key_path().join("private").with_extension("pem")
}

pub fn get_public_key_path() -> PathBuf {
    get_key_path().join("public").with_extension("pem")
}

pub fn get_temp_path() -> PathBuf {
    env::temp_dir().join(Uuid::now_v7().to_string())
}

pub fn get_package_path(name: &str, hash: &str) -> PathBuf {
    let store_dir_name = format!("{}-{}", name, hash);
    get_store_path().join(store_dir_name)
}

pub fn get_package_tar_path(name: &str, hash: &str) -> PathBuf {
    get_package_path(name, hash).with_extension("tar.gz")
}

pub fn get_package_source_path(source_name: &str, source_hash: &str) -> PathBuf {
    get_package_path(source_name, source_hash).with_extension("source")
}

pub fn get_package_source_tar_path(source_name: &str, source_hash: &str) -> PathBuf {
    get_package_path(source_name, source_hash).with_extension("source.tar.gz")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_dir() {
        assert_eq!(get_root_path().file_name().unwrap(), ".vorpal");
    }

    #[test]
    fn key_dir() {
        assert_eq!(get_key_path().file_name().unwrap(), "key");
    }

    #[test]
    fn key_root() {
        assert!(get_key_path().starts_with(get_root_path()));
    }

    #[test]
    fn store_dir() {
        assert_eq!(get_store_path().file_name().unwrap(), "store");
    }

    #[test]
    fn store_dir_root() {
        assert!(get_store_path().starts_with(get_root_path()));
    }
}
