use crate::{RegistryBackend, RegistryError, DEFAULT_GRPC_CHUNK_SIZE};
use anyhow::{anyhow, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_RANGE, CONTENT_TYPE},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::path::Path;
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use tracing::info;
use vorpal_schema::{
    config::v0::{ConfigArtifact, ConfigArtifactRequest},
    registry::v0::{RegistryPullRequest, RegistryPullResponse, RegistryPushRequest},
};

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

fn get_archive_key(hash: &str) -> String {
    format!("{}.tar.zst", hash)
}

fn get_config_key(hash: &str) -> String {
    format!("{}.json", hash)
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
    async fn get_archive(&self, request: &RegistryPullRequest) -> Result<(), Status> {
        let artifact_key = get_archive_key(&request.hash);
        let artifact_file = format!("/tmp/{}", artifact_key);
        let artifact_path = Path::new(&artifact_file);

        if artifact_path.exists() {
            return Ok(());
        }

        info!("fetch cache: {}", artifact_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&artifact_key, &request.hash)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            return Err(Status::not_found(format!(
                "cache entry not found: {artifact_key}"
            )));
        }

        let cache_entry = cache_entry.as_ref().unwrap();

        info!("fetch cache location: {:?}", cache_entry.archive_location);

        let cache_response = reqwest::get(&cache_entry.archive_location)
            .await
            .expect("failed to get");

        let cache_response_bytes = cache_response
            .bytes()
            .await
            .expect("failed to read response");

        write(&artifact_path, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("fetch cache saved: {:?}", artifact_path);

        Ok(())
    }

    async fn get_artifact(
        &self,
        request: &ConfigArtifactRequest,
    ) -> Result<ConfigArtifact, Status> {
        let artifact_key = get_config_key(&request.hash);
        let artifact_file_path = format!("/tmp/{}", artifact_key);
        let artifact_file = Path::new(&artifact_file_path);

        if artifact_file.exists() {
            let artifact_data = read(&artifact_file)
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            let artifact = serde_json::from_slice::<ConfigArtifact>(&artifact_data)
                .map_err(|err| Status::internal(err.to_string()))?;

            return Ok(artifact);
        }

        info!("fetch cache: {}", artifact_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&artifact_key, &request.hash)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            return Err(Status::not_found(format!(
                "cache entry not found: {artifact_key}"
            )));
        }

        let cache_entry = cache_entry.as_ref().unwrap();

        info!("fetch cache location: {:?}", cache_entry.archive_location);

        let cache_response = reqwest::get(&cache_entry.archive_location)
            .await
            .expect("failed to get");

        let cache_response_bytes = cache_response
            .bytes()
            .await
            .expect("failed to read response");

        write(&artifact_file, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("fetch cache saved: {:?}", artifact_file);

        let artifact_data = read(&artifact_file)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let artifact = serde_json::from_slice::<ConfigArtifact>(&artifact_data)
            .map_err(|err| Status::internal(err.to_string()))?;

        Ok(artifact)
    }

    async fn pull_archive(
        &self,
        request: &RegistryPullRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status> {
        let archive_key = get_archive_key(&request.hash);
        let archive_file_path = format!("/tmp/{}", archive_key);
        let archive_file = Path::new(&archive_file_path);

        if archive_file.exists() {
            let archive_data = read(&archive_file)
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            for chunk in archive_data.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
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

        info!("fetch entry: {}", archive_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&archive_key, &request.hash)
            .await
            .expect("failed to get cache entry");

        let Some(cache_entry) = cache_entry else {
            return Err(Status::not_found("store path not found"));
        };

        info!("fetch cache location: {:?}", cache_entry.archive_location);

        let cache_response = reqwest::get(&cache_entry.archive_location)
            .await
            .expect("failed to get");

        let cache_response_bytes = cache_response
            .bytes()
            .await
            .expect("failed to read response");

        info!("fetch cache saved: {:?}", archive_file);

        write(&archive_file, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("archive send: {:?}", cache_response_bytes.len());

        for chunk in cache_response_bytes.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
            tx.send(Ok(RegistryPullResponse {
                data: chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        info!("archive sent: {:?}", archive_file);

        Ok(())
    }

    async fn push_archive(&self, request: &RegistryPushRequest) -> Result<(), Status> {
        let archive_key = get_archive_key(request.hash.as_str());
        let archive_file_path = format!("/tmp/{}", archive_key);
        let archive_file = Path::new(&archive_file_path);

        if !archive_file.exists() {
            write(archive_file, &request.data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;
        }

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&archive_key, &request.hash)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            let cache_size = request.data.len() as u64;

            let cache_reserve = &self
                .cache_client
                .reserve_cache(archive_key, request.hash.clone(), Some(cache_size))
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to reserve cache: {:?}", e.to_string()))
                })?;

            if cache_reserve.cache_id == 0 {
                return Err(Status::internal("failed to reserve cache returned 0"));
            }

            self.cache_client
                .save_cache(
                    cache_reserve.cache_id,
                    &request.data,
                    DEFAULT_GHA_CHUNK_SIZE,
                )
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to save cache: {:?}", e.to_string()))
                })?;
        }

        Ok(())
    }

    async fn put_artifact(&self, request: &ConfigArtifact) -> Result<(), Status> {
        let artifact_json = serde_json::to_vec(request)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let artifact_hash = digest(&artifact_json);
        let artifact_key = get_config_key(&artifact_hash);
        let artifact_file_path = format!("/tmp/{}", artifact_key);
        let artifact_path = Path::new(&artifact_file_path);

        if !artifact_path.exists() {
            write(artifact_path, &artifact_json)
                .await
                .map_err(|err| Status::internal(format!("failed to write artifact: {:?}", err)))?;
        }

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&artifact_key, &artifact_hash)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            let cache_size = artifact_json.len() as u64;

            let cache_reserve = &self
                .cache_client
                .reserve_cache(artifact_key, artifact_hash, Some(cache_size))
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to reserve cache: {:?}", e.to_string()))
                })?;

            if cache_reserve.cache_id == 0 {
                return Err(Status::internal("failed to reserve cache returned 0"));
            }

            self.cache_client
                .save_cache(
                    cache_reserve.cache_id,
                    &artifact_json,
                    DEFAULT_GHA_CHUNK_SIZE,
                )
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to save cache: {:?}", e.to_string()))
                })?;
        }

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn RegistryBackend> {
        Box::new(self.clone())
    }
}
