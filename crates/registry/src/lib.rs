use aws_sdk_s3::Client;

pub mod archive;
pub mod artifact;
pub mod gha;
pub mod s3;

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("missing s3 bucket")]
    MissingS3Bucket,

    #[error("failed to create GHA cache client: {0}")]
    FailedToCreateGhaClient(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ServerBackend {
    #[default]
    Unknown,
    GHA,
    Local,
    S3,
}

#[derive(Clone, Debug)]
pub struct LocalBackend;

#[derive(Debug, Clone)]
pub struct GhaBackend {
    cache_client: gha::CacheClient,
}

impl GhaBackend {
    pub fn new() -> Result<Self, BackendError> {
        let cache_client = gha::CacheClient::new()
            .map_err(|err| BackendError::FailedToCreateGhaClient(err.to_string()))?;

        Ok(Self { cache_client })
    }
}

#[derive(Clone, Debug)]
pub struct S3Backend {
    bucket: String,
    client: Client,
}

impl LocalBackend {
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self)
    }
}

impl S3Backend {
    pub async fn new(bucket: Option<String>) -> Result<Self, BackendError> {
        let Some(bucket) = bucket else {
            return Err(BackendError::MissingS3Bucket);
        };

        let client_config = aws_config::load_from_env().await;
        let client = Client::new(&client_config);

        Ok(Self { bucket, client })
    }
}
