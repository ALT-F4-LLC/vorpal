use anyhow::{anyhow, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_RANGE, CONTENT_TYPE},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use tracing::info;

const API_VERSION: &str = "6.0-preview.1";

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

#[derive(Debug)]
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

        Ok(())
    }
}
