use anyhow::Result;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha256::{digest, try_digest};
use std::fs;
use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tar::Builder;
use walkdir::WalkDir;

pub fn get_home_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Home directory not found")
        .join(".vorpal")
}

pub fn get_key_dir() -> PathBuf {
    get_home_dir().join("key")
}

pub fn get_store_dir() -> PathBuf {
    get_home_dir().join("store")
}

pub fn get_database_path() -> PathBuf {
    get_home_dir().join("vorpal.db")
}

pub fn get_private_key_path() -> PathBuf {
    get_key_dir().join("private").with_extension("pem")
}

pub fn get_public_key_path() -> PathBuf {
    get_key_dir().join("public").with_extension("pem")
}

pub fn get_file_paths(source: &PathBuf, ignore_paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = WalkDir::new(&source)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if ignore_paths
                .iter()
                .any(|i| path.strip_prefix(&source).unwrap().starts_with(i))
            {
                return None;
            }
            Some(path.canonicalize().ok()?)
        })
        .collect();

    files.sort();

    Ok(files)
}

fn get_file_hash(path: PathBuf) -> Result<String, anyhow::Error> {
    if !path.is_file() {
        return Err(anyhow::anyhow!("Path is not a file"));
    }

    Ok(try_digest(path)?)
}

pub fn get_file_hashes(files: Vec<PathBuf>) -> Result<Vec<(PathBuf, String)>> {
    let hashes: Vec<(PathBuf, String)> = files
        .iter()
        .filter(|file| file.is_file())
        .map(|file| {
            let hash = get_file_hash(file.clone()).unwrap();
            (file.clone(), hash)
        })
        .collect();

    Ok(hashes)
}

pub fn get_source_hash(hashes: Vec<(PathBuf, String)>) -> Result<String> {
    let mut combined = String::new();

    for (_, hash) in hashes {
        combined.push_str(&hash);
    }

    Ok(digest(combined))
}

pub fn copy_files(
    source: PathBuf,
    source_path: PathBuf,
    files: Vec<PathBuf>,
) -> Result<(), anyhow::Error> {
    for path in files.clone() {
        if path == source {
            continue;
        }

        let p = path.strip_prefix(&source).unwrap();

        if !p.is_file() {
            let dest = &format!("{}/{}", source_path.display(), p.display());
            fs::create_dir_all(dest)?;
            continue;
        }

        let dest = &format!("{}/{}", source_path.display(), p.display());

        fs::copy(p, dest)?;
    }

    Ok(())
}

pub fn compress_files(
    source: PathBuf,
    source_tar: PathBuf,
    source_files: Vec<PathBuf>,
) -> Result<(), anyhow::Error> {
    let tar = File::create(source_tar)?;
    let tar_encoder = GzEncoder::new(tar, Compression::default());
    let mut tar_builder = Builder::new(tar_encoder);

    for path in source_files {
        if path == source {
            continue;
        }

        let relative_path = path.strip_prefix(&source).unwrap();

        println!("Adding: {}", relative_path.display());

        if path.is_file() {
            tar_builder.append_path_with_name(path.clone(), relative_path)?;
        } else if path.is_dir() {
            tar_builder.append_dir(relative_path, path.clone())?;
        }
    }

    tar_builder.finish()?;

    Ok(())
}

pub fn get_package_dir_name(name: &str, hash: &str) -> String {
    format!("{}-{}", name, hash)
}

pub fn set_files_permissions(files: &Vec<PathBuf>) -> Result<(), anyhow::Error> {
    for file in files {
        let permissions = fs::metadata(&file)?.permissions();

        if permissions.mode() & 0o111 != 0 {
            fs::set_permissions(file, fs::Permissions::from_mode(0o555))?;
        } else {
            fs::set_permissions(file, fs::Permissions::from_mode(0o444))?;
        }
    }

    Ok(())
}
