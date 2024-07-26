use crate::api::store_service_server::StoreService;
use crate::api::{StoreFetchResponse, StorePath, StorePathResponse};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

mod fetch;
mod path;

#[derive(Debug, Default)]
pub struct Store {}

#[tonic::async_trait]
impl StoreService for Store {
    type FetchStream = ReceiverStream<Result<StoreFetchResponse, Status>>;

    async fn fetch(
        &self,
        request: Request<StorePath>,
    ) -> Result<Response<Self::FetchStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            match fetch::stream(&tx, request).await {
                Ok(_) => (),
                Err(e) => {
                    tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn path(
        &self,
        request: Request<StorePath>,
    ) -> Result<Response<StorePathResponse>, Status> {
        let store_path = path::get(request)
            .await
            .map_err(|_| Status::internal("failed to get store path"))?;

        Ok(Response::new(store_path))
    }
}
