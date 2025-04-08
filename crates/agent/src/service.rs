use crate::build_source;
use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use vorpal_schema::{
    agent::v0::{
        agent_service_server::{AgentService, AgentServiceServer},
        PrepareArtifactResponse,
    },
    archive::v0::archive_service_client::ArchiveServiceClient,
    artifact::v0::{Artifact, ArtifactSource},
};
use vorpal_store::paths::get_public_key_path;

#[derive(Debug, Default)]
pub struct AgentServer {
    pub registry: String,
}

impl AgentServer {
    pub fn new(registry: String) -> Self {
        Self { registry }
    }
}

async fn prepare_artifact(
    registry: String,
    request: Request<Artifact>,
    tx: &mpsc::Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<(), Status> {
    let mut client = ArchiveServiceClient::connect(registry.to_owned())
        .await
        .map_err(|err| Status::internal(format!("failed to connect to registry: {:?}", err)))?;

    let artifact = request.into_inner();

    let mut artifact_sources = vec![];

    for source in artifact.sources.into_iter() {
        let digest = build_source(&mut client, &source, &tx.clone())
            .await
            .map_err(|err| Status::internal(format!("{}", err)))?;

        let source = ArtifactSource {
            digest: Some(digest.to_string()),
            excludes: source.excludes,
            includes: source.includes,
            name: source.name,
            path: source.path,
        };

        artifact_sources.push(source);
    }

    // TODO: explore using combined sources digest for the artifact

    let artifact = Artifact {
        name: artifact.name,
        sources: artifact_sources,
        steps: artifact.steps,
        systems: artifact.systems,
        target: artifact.target,
    };

    let artifact_json = serde_json::to_string(&artifact)
        .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;

    let artifact_digest = sha256::digest(&artifact_json);

    let artifact_response = PrepareArtifactResponse {
        artifact: Some(artifact),
        artifact_digest: Some(artifact_digest),
        artifact_output: None,
    };

    let _ = tx
        .send(Ok(artifact_response))
        .await
        .map_err(|_| Status::internal("failed to send response"));

    Ok(())
}

#[tonic::async_trait]
impl AgentService for AgentServer {
    type PrepareArtifactStream = ReceiverStream<Result<PrepareArtifactResponse, Status>>;

    async fn prepare_artifact(
        &self,
        request: Request<Artifact>,
    ) -> Result<Response<Self::PrepareArtifactStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        let registry = self.registry.clone();

        tokio::spawn(async move {
            if let Err(err) = prepare_artifact(registry, request, &tx).await {
                let _ = tx.send(Err(err)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

pub async fn listen(registry: &str, port: u16) -> Result<()> {
    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    let service = AgentServiceServer::new(AgentServer::new(registry.to_string()));

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}
