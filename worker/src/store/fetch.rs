use anyhow::Result;
use tokio::fs::read;
use tokio::sync::mpsc::Sender;
use tonic::{Request, Status};
use tracing::info;
use vorpal_schema::api::store::{StoreKind, StorePullResponse, StoreRequest};
use vorpal_store::paths::{get_package_archive_path, get_source_archive_path};

pub async fn stream(
    tx: &Sender<Result<StorePullResponse, Status>>,
    request: Request<StoreRequest>,
) -> Result<(), anyhow::Error> {
    let req = request.into_inner();

    let package_chunks_size = 8192;

    let store_path = match req.kind() {
        StoreKind::Package => get_package_archive_path(&req.name, &req.hash),
        StoreKind::Source => get_source_archive_path(&req.name, &req.hash),
        _ => anyhow::bail!("unsupported store path kind"),
    };

    if !store_path.exists() {
        anyhow::bail!("store path not found");
    }

    info!("serving store path: {}", store_path.display());

    let data = read(&store_path)
        .await
        .map_err(|_| Status::internal("failed to read store path"))?;

    let data_size = data.len();

    info!("serving store size: {}", data_size);

    for package_chunk in data.chunks(package_chunks_size) {
        tx.send(Ok(StorePullResponse {
            data: package_chunk.to_vec(),
        }))
        .await?;
    }

    Ok(())
}
