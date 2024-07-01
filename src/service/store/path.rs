use crate::api::{StorePath, StorePathKind, StorePathResponse};
use crate::store::paths::{get_package_path, get_package_source_tar_path};
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

    let package_source_tar_path = get_package_source_tar_path(&req.name, &req.hash);

    if !package_source_tar_path.exists() {
        anyhow::bail!("package source tar not found");
    }

    Ok(StorePathResponse {
        uri: package_source_tar_path.to_string_lossy().to_string(),
    })
}
