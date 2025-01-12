use std::path::Path;

use anyhow::{anyhow, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_RANGE, CONTENT_TYPE},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use tracing::info;
use vorpal_schema::vorpal::registry::v0::{RegistryKind, RegistryPullResponse, RegistryRequest};

use crate::{PushMetadata, RegistryBackend, RegistryError, DEFAULT_GRPC_CHUNK_SIZE};

const API_VERSION: &str = "6.0-preview.1";
const DEFAULT_GHA_CHUNK_SIZE: usize = 32 * 1024 * 1024; // 32MB

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactCacheEntry {
    #[serde(rename = "archiveLocation")]
    pub archive_location: String,
    #[serde(rename = "cacheKey")]
    pub cache_key: String,
    #[serde(rename = "cacheVersion")]
    pub cache_version: String,
    pub scope: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReserveCacheRequest {
    pub key: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "cacheSize")]
    pub cache_size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReserveCacheResponse {
    #[serde(rename = "cacheId")]
    pub cache_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitCacheRequest {
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct CacheClient {
    base_url: String,
    client: Client,
}

impl CacheClient {
    pub fn new() -> Result<Self> {
        let token = std::env::var("ACTIONS_RUNTIME_TOKEN")
            .context("ACTIONS_RUNTIME_TOKEN environment variable not found")?;

        let base_url = std::env::var("ACTIONS_CACHE_URL")
            .context("ACTIONS_CACHE_URL environment variable not found")?;

        let mut headers = HeaderMap::new();

        headers.insert(
            ACCEPT,
            HeaderValue::from_str(&format!("application/json;api-version={API_VERSION}"))?,
        );

        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {token}"))?,
        );

        let client = Client::builder()
            .user_agent("vorpal/github-actions-cache")
            .default_headers(headers)
            .build()?;

        Ok(Self { client, base_url })
    }

    pub async fn get_cache_entry(
        &self,
        key: &str,
        version: &str,
    ) -> Result<Option<ArtifactCacheEntry>> {
        let url = format!(
            "{}_apis/artifactcache/cache?keys={}&version={}",
            self.base_url, key, version
        );

        info!("get cache entry url -> {}", url);

        let response = self.client.get(&url).send().await?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(None),
            StatusCode::OK => {
                let entry = response.json::<ArtifactCacheEntry>().await?;
                Ok(Some(entry))
            }
            status => Err(anyhow!("Unexpected status code: {}", status)),
        }
    }

    pub async fn reserve_cache(
        &self,
        key: String,
        version: String,
        cache_size: Option<u64>,
    ) -> Result<ReserveCacheResponse> {
        let url = format!("{}_apis/artifactcache/caches", self.base_url);

        let request = ReserveCacheRequest {
            cache_size,
            key,
            version,
        };

        info!("reserve cache request -> {:?}", request);

        let request = self.client.post(&url).json(&request);

        let response = request.send().await?;

        if response.status() != StatusCode::CREATED {
            return Err(anyhow!("Unexpected status code: {}", response.status()));
        }

        let response_text = response.text().await?;

        info!("reserve cache response -> {}", response_text);

        let response = serde_json::from_str(&response_text)?;

        Ok(response)
    }

    pub async fn save_cache(&self, cache_id: u64, buffer: &[u8], chunk_size: usize) -> Result<()> {
        let buffer_size = buffer.len() as u64;
        let url = format!("{}_apis/artifactcache/caches/{}", self.base_url, cache_id);

        info!("Uploading cache buffer with size: {} bytes", buffer_size);

        for (i, chunk) in buffer.chunks(chunk_size).enumerate() {
            let chunk_len = chunk.len() as u64;
            let chunk_start = (i * chunk_size) as u64;
            let chunk_end = chunk_start + chunk_len - 1;
            let chunk_range = format!("bytes {}-{}/{}", chunk_start, chunk_end, buffer_size);

            info!("Uploading chunk range '{}'", chunk_range);

            let response = self
                .client
                .patch(&url)
                .header(CONTENT_TYPE, "application/octet-stream")
                .header(CONTENT_RANGE, &chunk_range)
                .body(chunk.to_vec())
                .send()
                .await?
                .error_for_status()?;

            info!(
                "Uploaded chunk '{}' response -> '{}'",
                chunk_range,
                response.status()
            );
        }

        // Commit the cache
        info!("Committing cache");
        let commit_request = CommitCacheRequest { size: buffer_size };
        self.client
            .post(&url)
            .json(&commit_request)
            .send()
            .await?
            .error_for_status()?;

        info!("Cache saved successfully");

        Ok(())
    }
}

fn get_cache_key(name: &str, hash: &str, kind: RegistryKind) -> Result<String> {
    let prefix = "vorpal-registry";
    let affix = format!("{}-{}", name, hash);

    match kind {
        RegistryKind::Artifact => Ok(format!("{}-{}-artifact", prefix, affix)),
        RegistryKind::ArtifactSource => Ok(format!("{}-{}-source", prefix, affix)),
        _ => Err(anyhow::anyhow!("unsupported store kind")),
    }
}

#[derive(Debug, Clone)]
pub struct GhaRegistryBackend {
    cache_client: CacheClient,
}

impl GhaRegistryBackend {
    pub fn new() -> Result<Self, RegistryError> {
        let cache_client = CacheClient::new()
            .map_err(|err| RegistryError::FailedToCreateGhaClient(err.to_string()))?;

        Ok(Self { cache_client })
    }
}

#[async_trait]
impl RegistryBackend for GhaRegistryBackend {
    async fn exists(&self, request: &RegistryRequest) -> Result<(), Status> {
        let cache_key = get_cache_key(&request.name, &request.hash, request.kind())
            .expect("failed to get cache key");
        let cache_key_file = format!("/tmp/{}", cache_key);
        let cache_key_file_path = Path::new(&cache_key_file);

        if cache_key_file_path.exists() {
            return Ok(());
        }

        info!("get cache entry -> {}", cache_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&cache_key, &request.hash)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        info!("get cache entry response -> {:?}", cache_entry);

        if cache_entry.is_none() {
            return Err(Status::not_found("store path not found"));
        }

        Ok(())
    }

    async fn pull(
        &self,
        request: &RegistryRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status> {
        let cache_key = get_cache_key(&request.name, &request.hash, request.kind())
            .expect("failed to get cache key");
        let cache_key_file = format!("/tmp/{}", cache_key);
        let cache_key_file_path = Path::new(&cache_key_file);

        if cache_key_file_path.exists() {
            let data = read(&cache_key_file_path)
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            for chunk in data.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
                tx.send(Ok(RegistryPullResponse {
                    data: chunk.to_vec(),
                }))
                .await
                .map_err(|err| {
                    Status::internal(format!("failed to send store chunk: {:?}", err))
                })?;
            }

            return Ok(());
        }

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&cache_key, &request.hash)
            .await
            .expect("failed to get cache entry");

        let Some(cache_entry) = cache_entry else {
            return Err(Status::not_found("store path not found"));
        };

        info!(
            "cache entry archive location -> {:?}",
            cache_entry.archive_location
        );

        let response = reqwest::get(&cache_entry.archive_location)
            .await
            .expect("failed to get");

        let response_bytes = response.bytes().await.expect("failed to read response");

        for chunk in response_bytes.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
            tx.send(Ok(RegistryPullResponse {
                data: chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        write(&cache_key_file_path, &response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        Ok(())
    }

    async fn push(&self, metadata: PushMetadata) -> Result<(), Status> {
        let PushMetadata {
            data_kind,
            hash,
            name,
            data,
        } = metadata;

        let cache_key = get_cache_key(&name, &hash, data_kind)
            .map_err(|err| Status::internal(format!("failed to get cache key: {:?}", err)))?;

        let cache_size = data.len() as u64;

        let cache_reserve = &self
            .cache_client
            .reserve_cache(cache_key, hash.clone(), Some(cache_size))
            .await
            .map_err(|e| {
                Status::internal(format!("failed to reserve cache: {:?}", e.to_string()))
            })?;

        if cache_reserve.cache_id == 0 {
            return Err(Status::internal("failed to reserve cache returned 0"));
        }

        self.cache_client
            .save_cache(cache_reserve.cache_id, &data, DEFAULT_GHA_CHUNK_SIZE)
            .await
            .map_err(|e| Status::internal(format!("failed to save cache: {:?}", e.to_string())))?;

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn RegistryBackend> {
        Box::new(self.clone())
    }
}
