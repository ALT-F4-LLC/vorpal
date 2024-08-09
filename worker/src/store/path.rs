use anyhow::Result;
use tonic::Request;
use vorpal_schema::api::store::{StoreExistsResponse, StoreKind, StoreRequest};
use vorpal_store::paths::{get_package_archive_path, get_source_archive_path};

pub async fn get(request: Request<StoreRequest>) -> Result<StoreExistsResponse, anyhow::Error> {
    let req = request.into_inner();

    match req.kind() {
        StoreKind::Package => {
            let package_path = get_package_archive_path(&req.name, &req.hash);
            if !package_path.exists() {
                anyhow::bail!("package not found");
            }

            Ok(StoreExistsResponse { exists: true })
        }

        StoreKind::Source => {
            let source_path = get_source_archive_path(&req.name, &req.hash);
            if !source_path.exists() {
                anyhow::bail!("source not found");
            }

            Ok(StoreExistsResponse { exists: true })
        }

        _ => anyhow::bail!("unsupported store path kind"),
    }
}
