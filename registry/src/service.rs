use anyhow::Result;
use rsa::{
    pss::{Signature, VerifyingKey},
    sha2::Sha256,
    signature::Verifier,
};
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status, Streaming};
use tracing::{error, info};
use vorpal_notary::get_public_key;
use vorpal_schema::vorpal::registry::v0::{
    registry_service_server::{RegistryService, RegistryServiceServer},
    RegistryPullRequest, RegistryPullResponse, RegistryPushRequest, RegistryPushResponse,
    RegistryStoreKind,
    RegistryStoreKind::{Artifact, ArtifactSource, UnknownStoreKind},
};
use vorpal_store::paths::{
    get_artifact_archive_path, get_public_key_path, get_source_archive_path, setup_paths,
};

const DEFAULT_CHUNK_SIZE: usize = 8192;

#[derive(Debug, Default)]
pub struct RegistryServer {}

#[tonic::async_trait]
impl RegistryService for RegistryServer {
    type PullStream = ReceiverStream<Result<RegistryPullResponse, Status>>;

    async fn pull(
        &self,
        request: Request<RegistryPullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let request = request.into_inner();
            let request_artifact_id = request.artifact_id.clone();

            if request_artifact_id.is_none() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("missing artifact id")))
                    .await
                {
                    error!("failed to send store error: {:?}", err);
                }

                return;
            }

            if let Some(artifact_id) = request_artifact_id {
                let path = match request.kind() {
                    Artifact => get_artifact_archive_path(&artifact_id.hash, &artifact_id.name),

                    ArtifactSource => get_source_archive_path(&artifact_id.hash, &artifact_id.name),

                    _ => {
                        if let Err(err) = tx
                            .send(Err(Status::invalid_argument("unsupported store kind")))
                            .await
                        {
                            error!("failed to send store error: {:?}", err);
                        }

                        return;
                    }
                };

                if !path.exists() {
                    if let Err(err) = tx
                        .send(Err(Status::not_found("store path not found")))
                        .await
                    {
                        error!("failed to send store error: {:?}", err);
                    }

                    return;
                }

                info!("serving path: {}", path.display());

                let data = match read(&path).await {
                    Ok(data) => data,
                    Err(err) => {
                        if let Err(err) = tx.send(Err(Status::internal(err.to_string()))).await {
                            error!("failed to send store error: {:?}", err);
                        }

                        return;
                    }
                };

                for chunk in data.chunks(DEFAULT_CHUNK_SIZE) {
                    if let Err(err) = tx
                        .send(Ok(RegistryPullResponse {
                            data: chunk.to_vec(),
                        }))
                        .await
                    {
                        error!("failed to send store chunk: {:?}", err);

                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn push(
        &self,
        request: Request<Streaming<RegistryPushRequest>>,
    ) -> Result<Response<RegistryPushResponse>, Status> {
        let mut data: Vec<u8> = vec![];
        let mut data_artifact_id = None;
        let mut data_kind = UnknownStoreKind;
        let mut data_signature = vec![];
        let mut stream = request.into_inner();

        while let Some(result) = stream.next().await {
            let result = result.map_err(|err| Status::internal(err.to_string()))?;

            data.extend_from_slice(&result.data);

            data_artifact_id = result.artifact_id;
            data_kind = RegistryStoreKind::try_from(result.kind).unwrap_or(UnknownStoreKind);
            data_signature = result.data_signature;
        }

        if data.is_empty() {
            return Err(Status::invalid_argument("missing data"));
        }

        if data_artifact_id.is_none() {
            return Err(Status::invalid_argument("missing artifact id"));
        }

        if data_kind == UnknownStoreKind {
            return Err(Status::invalid_argument("missing store kind"));
        }

        if data_signature.is_empty() {
            return Err(Status::invalid_argument("missing data signature"));
        }

        let public_key_path = get_public_key_path();

        let public_key = get_public_key(public_key_path).await.map_err(|err| {
            Status::internal(format!("failed to get public key: {:?}", err.to_string()))
        })?;

        let data_signature = Signature::try_from(data_signature.as_slice())
            .map_err(|err| Status::internal(format!("failed to parse signature: {:?}", err)))?;

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        if let Err(msg) = verifying_key.verify(&data, &data_signature) {
            return Err(Status::invalid_argument(format!(
                "invalid data signature: {:?}",
                msg
            )));
        }

        let artifact_id = data_artifact_id.unwrap();

        let path = match data_kind {
            Artifact => get_artifact_archive_path(&artifact_id.hash, &artifact_id.name),
            ArtifactSource => get_source_archive_path(&artifact_id.hash, &artifact_id.name),
            _ => return Err(Status::invalid_argument("unsupported store kind")),
        };

        if path.exists() {
            return Ok(Response::new(RegistryPushResponse { success: true }));
        }

        write(&path, &data)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("stored path: {}", path.display());

        Ok(Response::new(RegistryPushResponse { success: true }))
    }
}

pub async fn listen(port: u16) -> Result<()> {
    setup_paths().await?;

    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let addr = format!("[::]:{}", port)
        .parse()
        .map_err(|err| anyhow::anyhow!("failed to parse address: {:?}", err))?;

    info!("worker address: {}", addr);

    let registry_service = RegistryServiceServer::new(RegistryServer::default());

    Server::builder()
        .add_service(registry_service)
        .serve(addr)
        .await
        .map_err(|err| anyhow::anyhow!("failed to serve: {:?}", err))?;

    Ok(())
}
