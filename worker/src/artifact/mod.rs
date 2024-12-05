use anyhow::Result;
use tokio::fs::remove_dir_all;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use vorpal_schema::vorpal::{
    artifact::v0::ArtifactSystem,
    artifact::v0::{
        artifact_service_server::ArtifactService, ArtifactBuildRequest, ArtifactBuildResponse,
    },
};
use vorpal_store::temps::create_temp_dir;

mod build;

#[derive(Debug, Default)]
pub struct ArtifactServer {
    pub system: ArtifactSystem,
}

impl ArtifactServer {
    pub fn new(system: ArtifactSystem) -> Self {
        Self { system }
    }
}

#[tonic::async_trait]
impl ArtifactService for ArtifactServer {
    type BuildStream = ReceiverStream<Result<ArtifactBuildResponse, Status>>;

    async fn build(
        &self,
        request: Request<Streaming<ArtifactBuildRequest>>,
    ) -> Result<Response<Self::BuildStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let build_path = match create_temp_dir().await {
                Ok(path) => path,
                Err(e) => {
                    tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                    return;
                }
            };

            if let Err(e) = build::run(&build_path, request, &tx).await {
                tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            }

            if let Err(e) = remove_dir_all(build_path.clone()).await {
                tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
