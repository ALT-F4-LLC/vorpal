use crate::build_source;
use anyhow::Result;
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

#[tonic::async_trait]
impl AgentService for AgentServer {
    async fn prepare_artifact(
        &self,
        request: Request<Artifact>,
    ) -> Result<Response<PrepareArtifactResponse>, Status> {
        let mut client = ArchiveServiceClient::connect(self.registry.to_owned())
            .await
            .map_err(|err| Status::internal(format!("failed to connect to registry: {:?}", err)))?;

        let request = request.into_inner();

        let mut sources = vec![];

        for source in request.sources.into_iter() {
            let digest = build_source(&mut client, &source)
                .await
                .map_err(|err| Status::internal(format!("failed to build source: {:?}", err)))?;

            let source = ArtifactSource {
                digest: Some(digest.to_string()),
                excludes: source.excludes,
                includes: source.includes,
                name: source.name,
                path: source.path,
            };

            sources.push(source);
        }

        let artifact = Artifact {
            name: request.name,
            sources,
            steps: request.steps,
            systems: request.systems,
            target: request.target,
        };

        let artifact_json = serde_json::to_string(&artifact)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;

        let artifact_digest = sha256::digest(&artifact_json);

        Ok(Response::new(PrepareArtifactResponse {
            artifact: Some(artifact),
            artifact_digest: artifact_digest.to_string(),
        }))
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
