use anyhow::{anyhow, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_RANGE, CONTENT_TYPE},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::Arc,
};
use tokio::sync::Semaphore;
use tracing::info;

const VERSION_SALT: &str = "1.0";
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
    client: Client,
    base_url: String,
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
        keys: &[String],
        paths: &[String],
        compression_method: Option<String>,
        enable_cross_os_archive: bool,
    ) -> Result<Option<ArtifactCacheEntry>> {
        let version = get_cache_version(paths, compression_method, enable_cross_os_archive)?;
        let keys_str = keys.join(",");
        let url = format!(
            "{}/_apis/artifactcache/cache?keys={}&version={}",
            self.base_url, keys_str, version
        );

        info!("Getting cache entry from: {}", url);

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
        key: &str,
        paths: &[String],
        compression_method: Option<String>,
        enable_cross_os_archive: bool,
        cache_size: Option<u64>,
    ) -> Result<ReserveCacheResponse> {
        let version = get_cache_version(paths, compression_method, enable_cross_os_archive)?;
        let url = format!("{}/_apis/artifactcache/caches", self.base_url);

        let request = ReserveCacheRequest {
            cache_size,
            key: key.to_string(),
            version,
        };

        let request = self.client.post(&url).json(&request);

        let response = request.send().await?;

        if response.status() != StatusCode::CREATED {
            return Err(anyhow!("Unexpected status code: {}", response.status()));
        }

        let response_text = response.text().await?;

        info!("Response text: {}", response_text);

        let response = serde_json::from_str(&response_text)?;

        Ok(response)
    }

    pub async fn save_cache(
        &self,
        cache_id: u64,
        archive_path: &Path,
        concurrency: usize,
        chunk_size: usize,
    ) -> Result<()> {
        let file = File::open(archive_path)?;
        let file_size = file.metadata()?.len();
        let url = format!("{}/_apis/artifactcache/caches/{}", self.base_url, cache_id);

        info!("Uploading cache file with size: {} bytes", file_size);

        // Create a semaphore to limit concurrent uploads
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut tasks = Vec::new();
        let file = Arc::new(tokio::sync::Mutex::new(file));

        for chunk_start in (0..file_size).step_by(chunk_size) {
            let chunk_end = (chunk_start + chunk_size as u64 - 1).min(file_size - 1);
            let permit = semaphore.clone().acquire_owned().await?;
            let client = self.client.clone();
            let url = url.clone();
            let file = file.clone();

            let task = tokio::spawn(async move {
                let _permit = permit; // Keep permit alive for the duration of the upload
                let mut file = file.lock().await;
                file.seek(SeekFrom::Start(chunk_start))?;

                let mut buffer = vec![0; (chunk_end - chunk_start + 1) as usize];
                file.read_exact(&mut buffer)?;
                drop(file);

                let range = format!("bytes {}-{}/{}", chunk_start, chunk_end, file_size);
                let response = client
                    .patch(&url)
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .header(CONTENT_RANGE, &range)
                    .body(buffer)
                    .send()
                    .await?
                    .error_for_status()?;

                info!("Uploaded chunk response: {}", response.status());

                Result::<()>::Ok(())
            });

            tasks.push(task);
        }

        // Wait for all upload tasks to complete
        for task in tasks {
            task.await??;
        }

        // Commit the cache
        info!("Committing cache");
        let commit_request = CommitCacheRequest { size: file_size };
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

fn get_cache_version(
    paths: &[String],
    compression_method: Option<String>,
    enable_cross_os_archive: bool,
) -> Result<String> {
    let mut components = paths.to_vec();

    if let Some(method) = compression_method {
        components.push(method);
    }

    if cfg!(windows) && !enable_cross_os_archive {
        components.push("windows-only".to_string());
    }

    components.push(VERSION_SALT.to_string());

    let mut hasher = Sha256::new();
    hasher.update(components.join("|"));
    Ok(format!("{:x}", hasher.finalize()))
}
