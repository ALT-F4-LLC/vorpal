use crate::api::{StoreFetchResponse, StorePath, StorePathKind};
use crate::store::paths::{get_package_archive_path, get_source_archive_path};
use anyhow::Result;
use tokio::fs::read;
use tokio::sync::mpsc::Sender;
use tonic::{Request, Status};
use tracing::info;

pub async fn stream(
    tx: &Sender<Result<StoreFetchResponse, Status>>,
    request: Request<StorePath>,
) -> Result<(), anyhow::Error> {
    let req = request.into_inner();

    let package_chunks_size = 8192;

    let store_path = match req.kind() {
        StorePathKind::Package => get_package_archive_path(&req.name, &req.hash),
        StorePathKind::Source => get_source_archive_path(&req.name, &req.hash),
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
        tx.send(Ok(StoreFetchResponse {
            data: package_chunk.to_vec(),
        }))
        .await?;
    }

    Ok(())
}
