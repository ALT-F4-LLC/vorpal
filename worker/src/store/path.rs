use anyhow::Result;
use tonic::Request;
use vorpal_schema::vorpal::store::v0::{StoreExistsResponse, StoreKind, StoreRequest};
use vorpal_store::paths::{get_artifact_archive_path, get_source_archive_path};

pub async fn get(request: Request<StoreRequest>) -> Result<StoreExistsResponse> {
    let req = request.into_inner();

    match req.kind() {
        StoreKind::Artifact => {
            let artifact_path = get_artifact_archive_path(&req.hash, &req.name);

            if !artifact_path.exists() {
                anyhow::bail!("artifact not found");
            }

            Ok(StoreExistsResponse { exists: true })
        }

        StoreKind::ArtifactSource => {
            let source_path = get_source_archive_path(&req.hash, &req.name);

            if !source_path.exists() {
                anyhow::bail!("source not found");
            }

            Ok(StoreExistsResponse { exists: true })
        }

        _ => anyhow::bail!("unsupported store path kind"),
    }
}
