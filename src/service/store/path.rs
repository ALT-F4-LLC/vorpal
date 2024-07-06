use crate::api::{StorePath, StorePathKind, StorePathResponse};
use crate::store::paths::{get_package_archive_path, get_source_archive_path};
use anyhow::Result;
use tonic::Request;

pub async fn get(request: Request<StorePath>) -> Result<StorePathResponse, anyhow::Error> {
    let req = request.into_inner();

    match req.kind() {
        StorePathKind::Package => {
            let package_path = get_package_archive_path(&req.name, &req.hash);
            if !package_path.exists() {
                anyhow::bail!("package not found");
            }

            Ok(StorePathResponse {
                uri: package_path.to_string_lossy().to_string(),
            })
        }

        StorePathKind::Source => {
            let source_path = get_source_archive_path(&req.name, &req.hash);
            if !source_path.exists() {
                anyhow::bail!("source not found");
            }

            Ok(StorePathResponse {
                uri: source_path.to_string_lossy().to_string(),
            })
        }

        _ => anyhow::bail!("unsupported store path kind"),
    }
}
