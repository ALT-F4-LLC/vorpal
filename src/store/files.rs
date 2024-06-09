use anyhow::Result;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tokio::fs;

pub async fn set_files_permissions(files: &[PathBuf]) -> Result<(), anyhow::Error> {
    for file in files {
        let permissions = fs::metadata(file).await?;
        if permissions.mode() & 0o111 != 0 {
            fs::set_permissions(file, std::fs::Permissions::from_mode(0o555)).await?;
        } else {
            fs::set_permissions(file, std::fs::Permissions::from_mode(0o444)).await?;
        }
    }

    Ok(())
}
