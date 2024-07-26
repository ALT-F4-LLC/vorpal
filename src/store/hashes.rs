use anyhow::Result;
use sha256::{digest, try_digest};
use std::path::Path;

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
