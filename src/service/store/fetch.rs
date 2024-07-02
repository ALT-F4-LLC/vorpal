use crate::api::{StoreFetchResponse, StorePath, StorePathKind};
use crate::store::paths::{get_package_archive_path, get_package_source_archive_path};
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

    if req.kind == StorePathKind::Unknown as i32 {
        anyhow::bail!("invalid store path kind");
    }

    if req.kind == StorePathKind::Package as i32 {
        let package_archive_path = get_package_archive_path(&req.name, &req.hash);

        if !package_archive_path.exists() {
            anyhow::bail!("package archive not found");
        }

        info!(
            "serving package archive: {}",
            package_archive_path.display()
        );

        let data = read(&package_archive_path)
            .await
            .map_err(|_| Status::internal("failed to read cached package"))?;

        let data_size = data.len();

        info!("serving package archive size: {}", data_size);

        for package_chunk in data.chunks(package_chunks_size) {
            tx.send(Ok(StoreFetchResponse {
                data: package_chunk.to_vec(),
            }))
            .await?;
        }

        return Ok(());
    }

    let package_source_tar_path = get_package_source_archive_path(&req.name, &req.hash);

    if !package_source_tar_path.exists() {
        anyhow::bail!("package source tar not found");
    }

    info!(
        "serving package source: {}",
        package_source_tar_path.display()
    );

    let data = read(&package_source_tar_path)
        .await
        .map_err(|_| Status::internal("failed to read cached package"))?;

    for package_chunk in data.chunks(package_chunks_size) {
        tx.send(Ok(StoreFetchResponse {
            data: package_chunk.to_vec(),
        }))
        .await
        .unwrap();
    }

    Ok(())
}
