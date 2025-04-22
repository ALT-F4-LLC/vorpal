pub fn get_archive_key(hash: &str) -> String {
    format!("store/{hash}.tar.zst")
}

pub fn get_artifact_key(hash: &str) -> String {
    format!("store/{hash}.json")
}
