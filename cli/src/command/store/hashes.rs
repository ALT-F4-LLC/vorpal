use anyhow::Result;
use sha256::{digest, try_digest};
use std::path::{Path, PathBuf};

pub fn get_file_digest<P: AsRef<Path> + Send>(path: P) -> Result<String> {
    if !path.as_ref().is_file() {
        return Err(anyhow::anyhow!("Path is not a file"));
    }

    Ok(try_digest(path).expect("Failed to get file hash"))
}

pub fn get_files_digests(files: &[PathBuf]) -> Result<Vec<String>> {
    let hashes = files
        .iter()
        .filter(|file| file.is_file())
        .map(|file| get_file_digest(file).unwrap())
        .collect();

    Ok(hashes)
}

pub fn get_digests_digest(hashes: Vec<String>) -> Result<String> {
    let mut combined = String::new();

    for hash in hashes {
        combined.push_str(&hash);
    }

    Ok(digest(combined))
}

pub fn get_source_digest(paths: Vec<PathBuf>) -> Result<String> {
    if paths.is_empty() {
        anyhow::bail!("no source files found")
    }

    let paths_digests = get_files_digests(&paths)?;
    let paths_digest = get_digests_digest(paths_digests)?;

    Ok(paths_digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::store::paths::get_file_paths;
    use std::fs::write;
    use tempfile::TempDir;

    fn dir_with(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (name, contents) in files {
            write(dir.path().join(name), contents).unwrap();
        }
        dir
    }

    fn source_digest_of(dir: &TempDir) -> String {
        let paths = get_file_paths(&dir.path().to_path_buf(), vec![], vec![]).unwrap();
        get_source_digest(paths).unwrap()
    }

    // The combined digest concatenates per-file hashes in the order given, so a
    // divergent path sort across hosts would silently change the source digest.
    // This pins that order-sensitivity so the paths.rs sort-determinism guards
    // are load-bearing, not incidental.
    #[test]
    fn get_digests_digest_is_order_sensitive() {
        let forward = get_digests_digest(vec!["aaa".to_string(), "bbb".to_string()]).unwrap();
        let reversed = get_digests_digest(vec!["bbb".to_string(), "aaa".to_string()]).unwrap();

        assert_ne!(forward, reversed);
    }

    // Cross-producer digest equality (unit level): the same file set produced on
    // two hosts that happened to create the files in different orders yields a
    // byte-identical source digest — the digest is a pure function of content +
    // sorted path order, with no host arch/OS input.
    #[test]
    fn get_source_digest_identical_for_same_content_regardless_of_creation_order() {
        let host_a = dir_with(&[("a.txt", "alpha"), ("b.txt", "bravo"), ("c.txt", "charlie")]);
        let host_b = dir_with(&[("c.txt", "charlie"), ("a.txt", "alpha"), ("b.txt", "bravo")]);

        assert_eq!(source_digest_of(&host_a), source_digest_of(&host_b));
    }

    #[test]
    fn get_source_digest_changes_when_content_changes() {
        let original = dir_with(&[("a.txt", "alpha")]);
        let tampered = dir_with(&[("a.txt", "ALPHA")]);

        assert_ne!(source_digest_of(&original), source_digest_of(&tampered));
    }
}
