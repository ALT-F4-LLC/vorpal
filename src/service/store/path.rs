use crate::api::{StorePath, StorePathKind, StorePathResponse};
use crate::store::paths::{get_package_path, get_package_source_archive_path};
use anyhow::Result;
use tonic::Request;

pub async fn get(request: Request<StorePath>) -> Result<StorePathResponse, anyhow::Error> {
    let req = request.into_inner();

    if req.kind == StorePathKind::Unknown as i32 {
        anyhow::bail!("invalid store path kind");
    }

    if req.kind == StorePathKind::Package as i32 {
        let package_path = get_package_path(&req.name, &req.hash);

        if !package_path.exists() {
            anyhow::bail!("package not found");
        }

        return Ok(StorePathResponse {
            uri: package_path.to_string_lossy().to_string(),
        });
    }

    let source_archive_path = get_package_source_archive_path(&req.name, &req.hash);

    if !source_archive_path.exists() {
        anyhow::bail!("package source tar not found");
    }

    Ok(StorePathResponse {
        uri: source_archive_path.to_string_lossy().to_string(),
    })
}
