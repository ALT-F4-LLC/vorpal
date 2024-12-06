use anyhow::Result;
use tokio::fs::read;
use tokio::sync::mpsc::Sender;
use tonic::{Request, Status};
use tracing::info;
use vorpal_schema::vorpal::store::v0::{StoreKind, StorePullResponse, StoreRequest};
use vorpal_store::paths::{get_artifact_archive_path, get_source_archive_path};

pub async fn stream(
    tx: &Sender<Result<StorePullResponse, Status>>,
    request: Request<StoreRequest>,
) -> Result<()> {
    let req = request.into_inner();

    let artifact_chunks_size = 8192;

    let store_path = match req.kind() {
        StoreKind::Artifact => get_artifact_archive_path(&req.hash, &req.name),
        StoreKind::ArtifactSource => get_source_archive_path(&req.hash, &req.name),
        _ => anyhow::bail!("unsupported store path kind"),
    };

    if !store_path.exists() {
        anyhow::bail!("store path not found");
    }

    info!("serving: {}", store_path.display());

    let data = read(&store_path)
        .await
        .expect("failed to read store path data");

    for artifact_chunk in data.chunks(artifact_chunks_size) {
        tx.send(Ok(StorePullResponse {
            data: artifact_chunk.to_vec(),
        }))
        .await
        .expect("failed to send store chunk");
    }

    let data_size = data.len();

    info!("served: {}", data_size);

    Ok(())
}
