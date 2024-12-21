use anyhow::Result;
use aws_sdk_s3::Client;
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
use tracing::error;
use vorpal_notary::get_public_key;
use vorpal_schema::vorpal::registry::v0::{
    registry_service_server::{RegistryService, RegistryServiceServer},
    RegistryKind,
    RegistryKind::{Artifact, ArtifactSource, UnknownStoreKind},
    RegistryPullResponse, RegistryPushRequest, RegistryRequest, RegistryResponse,
};
use vorpal_store::paths::{
    get_artifact_archive_path, get_public_key_path, get_source_archive_path, get_store_dir_name,
    set_timestamps, setup_paths,
};

const DEFAULT_CHUNK_SIZE: usize = 8192;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RegistryServerBackend {
    #[default]
    Unknown,
    Local,
    S3,
}

#[derive(Debug, Default)]
pub struct RegistryServer {
    pub backend: RegistryServerBackend,
    pub backend_s3_bucket: Option<String>,
}

impl RegistryServer {
    pub fn new(backend: RegistryServerBackend, backend_s3_bucket: Option<String>) -> Self {
        Self {
            backend,
            backend_s3_bucket,
        }
    }
}

#[tonic::async_trait]
impl RegistryService for RegistryServer {
    type PullStream = ReceiverStream<Result<RegistryPullResponse, Status>>;

    async fn exists(
        &self,
        request: Request<RegistryRequest>,
    ) -> Result<Response<RegistryResponse>, Status> {
        let request = request.into_inner();

        if request.hash.is_empty() {
            return Err(Status::invalid_argument("missing store id"));
        }

        if request.name.is_empty() {
            return Err(Status::invalid_argument("missing store name"));
        }

        let backend = self.backend.clone();

        if backend == RegistryServerBackend::Local {
            let path = match request.kind() {
                Artifact => get_artifact_archive_path(&request.hash, &request.name),
                ArtifactSource => get_source_archive_path(&request.hash, &request.name),
                _ => return Err(Status::invalid_argument("unsupported store kind")),
            };

            if !path.exists() {
                return Err(Status::not_found("store path not found"));
            }
        }

        if backend == RegistryServerBackend::S3 && self.backend_s3_bucket.is_none() {
            return Err(Status::invalid_argument("missing s3 bucket"));
        }

        if backend == RegistryServerBackend::S3 {
            let artifact_key = match request.kind() {
                Artifact => format!(
                    "store/{}.artifact",
                    get_store_dir_name(&request.hash, &request.name)
                ),
                ArtifactSource => format!(
                    "store/{}.source",
                    get_store_dir_name(&request.hash, &request.name)
                ),
                _ => return Err(Status::invalid_argument("unsupported store kind")),
            };

            let client_config = aws_config::load_from_env().await;
            let client = Client::new(&client_config);

            let head_result = client
                .head_object()
                .bucket(self.backend_s3_bucket.clone().unwrap())
                .key(&artifact_key)
                .send()
                .await;

            if head_result.is_err() {
                return Err(Status::not_found("store path not found"));
            }
        }

        Ok(Response::new(RegistryResponse { success: true }))
    }

    async fn pull(
        &self,
        request: Request<RegistryRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        let backend = self.backend.clone();
        let backend_s3_bucket = self.backend_s3_bucket.clone();

        if backend == RegistryServerBackend::S3 && backend_s3_bucket.is_none() {
            return Err(Status::invalid_argument("missing s3 bucket"));
        }

        let client_bucket_name = backend_s3_bucket.unwrap_or_default();

        tokio::spawn(async move {
            let request = request.into_inner();

            if request.hash.is_empty() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("missing artifact id")))
                    .await
                {
                    error!("failed to send store error: {:?}", err);
                }

                return;
            }

            if backend == RegistryServerBackend::Local {
                let path = match request.kind() {
                    Artifact => get_artifact_archive_path(&request.hash, &request.name),
                    ArtifactSource => get_source_archive_path(&request.hash, &request.name),
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

            if backend == RegistryServerBackend::S3 {
                let artifact_key = match request.kind() {
                    Artifact => format!(
                        "store/{}.artifact",
                        get_store_dir_name(&request.hash, &request.name)
                    ),

                    ArtifactSource => {
                        format!(
                            "store/{}.source",
                            get_store_dir_name(&request.hash, &request.name)
                        )
                    }

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

                let client_config = aws_config::load_from_env().await;
                let client = Client::new(&client_config);

                let _ = match client
                    .head_object()
                    .bucket(client_bucket_name.clone())
                    .key(artifact_key.clone())
                    .send()
                    .await
                {
                    Ok(_) => {}
                    Err(err) => {
                        if let Err(err) = tx.send(Err(Status::not_found(err.to_string()))).await {
                            error!("failed to send store error: {:?}", err);
                        }

                        return;
                    }
                };

                let mut stream = match client
                    .get_object()
                    .bucket(client_bucket_name)
                    .key(artifact_key)
                    .send()
                    .await
                {
                    Ok(output) => output.body,
                    Err(err) => {
                        if let Err(err) = tx.send(Err(Status::internal(err.to_string()))).await {
                            error!("failed to send store error: {:?}", err);
                        }

                        return;
                    }
                };

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            if let Err(err) = tx
                                .send(Ok(RegistryPullResponse {
                                    data: chunk.to_vec(),
                                }))
                                .await
                            {
                                error!("failed to send store chunk: {:?}", err.to_string());

                                break;
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(Status::internal(err.to_string()))).await;

                            break;
                        }
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn push(
        &self,
        request: Request<Streaming<RegistryPushRequest>>,
    ) -> Result<Response<RegistryResponse>, Status> {
        let backend = self.backend.clone();

        if backend == RegistryServerBackend::S3 && self.backend_s3_bucket.is_none() {
            return Err(Status::invalid_argument("missing `s3` bucket argument"));
        }

        let mut data: Vec<u8> = vec![];
        let mut data_hash = None;
        let mut data_kind = UnknownStoreKind;
        let mut data_name = None;
        let mut data_signature = vec![];
        let mut stream = request.into_inner();

        while let Some(result) = stream.next().await {
            let result = result.map_err(|err| Status::internal(err.to_string()))?;

            data.extend_from_slice(&result.data);

            data_hash = Some(result.hash);
            data_kind = RegistryKind::try_from(result.kind).unwrap_or(UnknownStoreKind);
            data_name = Some(result.name);
            data_signature = result.data_signature;
        }

        if data.is_empty() {
            return Err(Status::invalid_argument("missing `data` field"));
        }

        if data_hash.is_none() {
            return Err(Status::invalid_argument("missing `hash` field"));
        }

        if data_kind == UnknownStoreKind {
            return Err(Status::invalid_argument("missing `kind` field"));
        }

        if data_signature.is_empty() {
            return Err(Status::invalid_argument("missing `data_signature` field"));
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

        let backend = self.backend.clone();
        let hash = data_hash.unwrap();
        let name = data_name.unwrap();

        if backend == RegistryServerBackend::Local {
            let path = match data_kind {
                Artifact => get_artifact_archive_path(&hash, &name),
                ArtifactSource => get_source_archive_path(&hash, &name),
                _ => return Err(Status::invalid_argument("unsupported store kind")),
            };

            if path.exists() {
                return Ok(Response::new(RegistryResponse { success: true }));
            }

            write(&path, &data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;

            set_timestamps(&path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;
        }

        if backend == RegistryServerBackend::S3 {
            let artifact_key = match data_kind {
                Artifact => format!("store/{}.artifact", get_store_dir_name(&hash, &name)),
                ArtifactSource => format!("store/{}.source", get_store_dir_name(&hash, &name)),
                _ => return Err(Status::invalid_argument("unsupported store kind")),
            };

            let client_config = aws_config::load_from_env().await;
            let client = Client::new(&client_config);

            let head_result = client
                .head_object()
                .bucket(self.backend_s3_bucket.clone().unwrap())
                .key(&artifact_key)
                .send()
                .await;

            if head_result.is_ok() {
                return Ok(Response::new(RegistryResponse { success: true }));
            }

            let _ = client
                .put_object()
                .bucket(self.backend_s3_bucket.clone().unwrap())
                .key(artifact_key)
                .body(data.into())
                .send()
                .await
                .map_err(|err| {
                    Status::internal(format!("failed to write store path: {:?}", err))
                })?;
        }

        Ok(Response::new(RegistryResponse { success: true }))
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

    let registry_service = RegistryServiceServer::new(RegistryServer::default());

    Server::builder()
        .add_service(registry_service)
        .serve(addr)
        .await
        .map_err(|err| anyhow::anyhow!("failed to serve: {:?}", err))?;

    Ok(())
}
