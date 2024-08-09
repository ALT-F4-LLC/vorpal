use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use vorpal_schema::api::store::{
    store_service_server::StoreService, StoreExistsResponse, StorePullResponse, StoreRequest,
};

mod fetch;
mod path;

#[derive(Debug, Default)]
pub struct StoreServer {}

#[tonic::async_trait]
impl StoreService for StoreServer {
    type PullStream = ReceiverStream<Result<StorePullResponse, Status>>;

    async fn pull(
        &self,
        request: Request<StoreRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
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

    async fn exists(
        &self,
        request: Request<StoreRequest>,
    ) -> Result<Response<StoreExistsResponse>, Status> {
        let store_path = path::get(request)
            .await
            .map_err(|_| Status::internal("failed to get store path"))?;

        Ok(Response::new(store_path))
    }
}
