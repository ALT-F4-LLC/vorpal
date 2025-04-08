use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use std::path::Path;
use tokio::fs::{read, remove_dir_all, remove_file, write};
use tokio::sync::mpsc::Sender;
use tokio_tar::Archive;
use tonic::{transport::Channel, Code, Status};
use url::Url;
use vorpal_schema::{
    agent::v0::PrepareArtifactResponse,
    archive::v0::{
        archive_service_client::ArchiveServiceClient, ArchivePullRequest, ArchivePushRequest,
    },
    artifact::v0::ArtifactSource,
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zip},
    hashes::hash_files,
    paths::{copy_files, get_private_key_path},
    temps::{create_sandbox_dir, create_sandbox_file},
};
use vorpal_store::{
    notary,
    paths::{get_file_paths, set_timestamps},
};

pub mod service;

#[derive(PartialEq)]
enum ArtifactSourceType {
    Unknown,
    Local,
    Git,
    Http,
}

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

pub async fn build_source(
    client: &mut ArchiveServiceClient<Channel>,
    source: &ArtifactSource,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<String> {
    if let Some(digest) = &source.digest {
        let request = ArchivePullRequest {
            digest: digest.to_string(),
        };

        match client.check(request).await {
            Err(status) => {
                if status.code() != Code::NotFound {
                    bail!("registry pull error: {:?}", status);
                }
            }

            Ok(_) => {
                return Ok(digest.to_string());
            }
        }
    }

    // 2. Build source

    let source_type = match &source.path {
        s if Path::new(s).exists() => ArtifactSourceType::Local,
        s if s.starts_with("git") => ArtifactSourceType::Git,
        s if s.starts_with("http") => ArtifactSourceType::Http,
        _ => ArtifactSourceType::Unknown,
    };

    if source_type == ArtifactSourceType::Git {
        bail!("'source.{}.path' git not supported", source.name);
    }

    if source_type == ArtifactSourceType::Unknown {
        bail!(
            "'source.{}.path' unknown kind: {:?}",
            source.name,
            source.path
        );
    }

    let source_sandbox = create_sandbox_dir().await?;

    if source_type == ArtifactSourceType::Http {
        if source.digest.is_none() {
            bail!(
                "'source.{}.hash' required for remote sources: {:?}",
                source.name,
                source.path
            );
        }

        if source.digest.is_some() && source.digest.clone().unwrap() == "" {
            bail!(
                "'source.{}.hash' empty for remote sources: {:?}",
                source.name,
                source.path
            );
        }

        let http_path = Url::parse(&source.path).map_err(|e| anyhow::anyhow!(e))?;

        if http_path.scheme() != "http" && http_path.scheme() != "https" {
            bail!("remote scheme not supported: {:?}", http_path.scheme());
        }

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("download: {}", http_path)),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        let remote_response = reqwest::get(http_path.as_str())
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        if !remote_response.status().is_success() {
            anyhow::bail!("URL not failed: {:?}", remote_response.status());
        }

        let remote_response_bytes = remote_response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        let remote_response_bytes = remote_response_bytes.as_ref();

        let kind = infer::get(remote_response_bytes);

        if kind.is_none() {
            let source_file_name = http_path
                .path_segments()
                .and_then(|segments| segments.last())
                .and_then(|name| if name.is_empty() { None } else { Some(name) })
                .unwrap_or(&source.name);

            let source_file_path = source_sandbox.join(source_file_name);

            write(&source_file_path, remote_response_bytes)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("unpack: {}", http_path)),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        if let Some(kind) = kind {
            match kind.mime_type() {
                "application/gzip" => {
                    let decoder = GzipDecoder::new(remote_response_bytes);
                    let mut archive = Archive::new(decoder);

                    archive
                        .unpack(&source_sandbox)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;

                    // let source_cache_path = source_cache_path.join("...");
                }

                "application/x-bzip2" => {
                    let decoder = BzDecoder::new(remote_response_bytes);
                    let mut archive = Archive::new(decoder);

                    archive
                        .unpack(&source_sandbox)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                }

                "application/x-xz" => {
                    let decoder = XzDecoder::new(remote_response_bytes);
                    let mut archive = Archive::new(decoder);

                    archive
                        .unpack(&source_sandbox)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                }

                "application/zip" => {
                    let archive_sandbox_path = create_sandbox_file(Some("zip")).await?;

                    write(&archive_sandbox_path, remote_response_bytes)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;

                    unpack_zip(&archive_sandbox_path, &source_sandbox).await?;

                    remove_file(&archive_sandbox_path)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                }

                _ => {
                    bail!(
                        "'source.{}.path' unsupported mime-type detected: {:?}",
                        source.name,
                        source.path
                    );
                }
            }
        }
    }

    if source_type == ArtifactSourceType::Local {
        let local_path = Path::new(&source.path).to_path_buf();

        if !local_path.exists() {
            bail!("'source.{}.path' not found: {:?}", source.name, source.path);
        }

        // TODO: make path relevant to the current working directory

        let local_files = get_file_paths(
            &local_path,
            source.excludes.clone(),
            source.includes.clone(),
        )?;

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("copy: {}", local_path.display())),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        copy_files(&local_path, local_files, &source_sandbox).await?;
    }

    let source_sandbox_files = get_file_paths(
        &source_sandbox,
        source.excludes.clone(),
        source.includes.clone(),
    )?;

    if source_sandbox_files.is_empty() {
        bail!(
            "Artifact 'source.{}.path' no files found: {:?}",
            source.name,
            source.path
        );
    }

    // 3. Sanitize files

    for sandbox_path in source_sandbox_files.clone().into_iter() {
        set_timestamps(&sandbox_path).await?;
    }

    // 4. Hash files

    let source_digest = hash_files(source_sandbox_files.clone())?;

    if let Some(hash) = source.digest.clone() {
        if hash != source_digest {
            bail!(
                "'source.{}.hash' mismatch: {} != {}",
                source.name,
                source_digest,
                hash
            );
        }
    }

    // 5. Push source

    let registry_request = ArchivePullRequest {
        digest: source_digest.clone(),
    };

    if let Err(status) = client.check(registry_request).await {
        if status.code() != Code::NotFound {
            bail!("registry pull error: {:?}", status);
        }

        let source_sandbox_archive = create_sandbox_file(Some("tar.zst")).await?;

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("pack: {}", source_digest)),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        compress_zstd(
            &source_sandbox,
            &source_sandbox_files,
            &source_sandbox_archive,
        )
        .await?;

        let private_key_path = get_private_key_path();

        if !private_key_path.exists() {
            bail!("Private key not found: {}", private_key_path.display());
        }

        let source_archive_data = read(&source_sandbox_archive).await?;

        let source_signature = notary::sign(private_key_path.clone(), &source_archive_data).await?;

        let mut source_stream = vec![];

        for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
            source_stream.push(ArchivePushRequest {
                data: chunk.to_vec(),
                digest: source_digest.clone(),
                signature: source_signature.clone().to_vec(),
            });
        }

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("push: {}", source_digest)),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        client
            .push(tokio_stream::iter(source_stream))
            .await
            .expect("failed to push");

        remove_file(&source_sandbox_archive).await?;
    }

    remove_dir_all(&source_sandbox)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(source_digest)
}
