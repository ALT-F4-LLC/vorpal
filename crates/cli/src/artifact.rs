use crate::get_prefix;
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use std::{collections::HashMap, path::Path};
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, write};
use tokio_tar::Archive;
use tonic::{transport::Channel, Code};
use tracing::info;
use url::Url;
use vorpal_schema::artifact::v0::ArtifactBuildRequest;
use vorpal_schema::{
    artifact::v0::artifact_service_client::ArtifactServiceClient,
    config::v0::{ConfigArtifact, ConfigArtifactSource},
    registry::v0::{
        registry_service_client::RegistryServiceClient, RegistryArchive, RegistryPullRequest,
        RegistryPushRequest,
    },
};
use vorpal_store::{
    archives::unpack_zstd,
    paths::{get_archive_path, get_file_paths, get_store_path, set_timestamps},
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zip},
    hashes::hash_files,
    paths::{copy_files, get_private_key_path},
    temps::{create_sandbox_dir, create_sandbox_file},
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit
                                         //
#[derive(PartialEq)]
enum ConfigArtifactSourceType {
    Unknown,
    Local,
    Git,
    Http,
}

pub async fn build_source(
    artifact_name: &str,
    source: &ConfigArtifactSource,
    registry: &mut RegistryServiceClient<Channel>,
) -> Result<String> {
    if let Some(hash) = &source.hash {
        let request = RegistryPullRequest {
            archive: RegistryArchive::ArtifactSource as i32,
            hash: hash.to_string(),
        };

        match registry.get_archive(request).await {
            Err(status) => {
                if status.code() != Code::NotFound {
                    bail!("registry pull error: {:?}", status);
                }
            }

            Ok(_) => {
                return Ok(hash.to_string());
            }
        }
    }

    // 2. Build source

    info!(
        "{} build source: {}",
        get_prefix(artifact_name),
        source.name
    );

    let source_type = match &source.path {
        s if Path::new(s).exists() => ConfigArtifactSourceType::Local,
        s if s.starts_with("git") => ConfigArtifactSourceType::Git,
        s if s.starts_with("http") => ConfigArtifactSourceType::Http,
        _ => ConfigArtifactSourceType::Unknown,
    };

    if source_type == ConfigArtifactSourceType::Git {
        bail!("`source.{}.path` git not supported", source.name);
    }

    if source_type == ConfigArtifactSourceType::Unknown {
        bail!(
            "`source.{}.path` unknown kind: {:?}",
            source.name,
            source.path
        );
    }

    let source_sandbox = create_sandbox_dir().await?;

    if source_type == ConfigArtifactSourceType::Http {
        if source.hash.is_none() {
            bail!(
                "`source.{}.hash` required for remote sources: {:?}",
                source.name,
                source.path
            );
        }

        if source.hash.is_some() && source.hash.clone().unwrap() == "" {
            bail!(
                "`source.{}.hash` empty for remote sources: {:?}",
                source.name,
                source.path
            );
        }

        info!(
            "{} download source: {}",
            get_prefix(artifact_name),
            source.name
        );

        let http_path = Url::parse(&source.path).map_err(|e| anyhow::anyhow!(e))?;

        if http_path.scheme() != "http" && http_path.scheme() != "https" {
            bail!(
                "source remote scheme not supported: {:?}",
                http_path.scheme()
            );
        }

        let remote_response = reqwest::get(http_path.as_str())
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        if !remote_response.status().is_success() {
            anyhow::bail!("source URL not failed: {:?}", remote_response.status());
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

        info!(
            "{} unpack source: {}",
            get_prefix(artifact_name),
            source.name
        );

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
                        "`source.{}.path` unsupported mime-type detected: {:?}",
                        source.name,
                        source.path
                    );
                }
            }
        }
    }

    if source_type == ConfigArtifactSourceType::Local {
        let local_path = Path::new(&source.path).to_path_buf();

        if !local_path.exists() {
            bail!("`source.{}.path` not found: {:?}", source.name, source.path);
        }

        info!("{} copy source: {}", get_prefix(artifact_name), source.name);

        // TODO: make path relevant to the current working directory

        let local_files = get_file_paths(
            &local_path,
            source.excludes.clone(),
            source.includes.clone(),
        )?;

        copy_files(&local_path, local_files, &source_sandbox).await?;
    }

    let source_sandbox_files = get_file_paths(
        &source_sandbox,
        source.excludes.clone(),
        source.includes.clone(),
    )?;

    if source_sandbox_files.is_empty() {
        bail!(
            "Artifact `source.{}.path` no files found: {:?}",
            source.name,
            source.path
        );
    }

    // 3. Sanitize files

    for sandbox_path in source_sandbox_files.clone().into_iter() {
        set_timestamps(&sandbox_path).await?;
    }

    // 4. Hash files

    info!("{} hash source: {}", get_prefix(artifact_name), source.name);

    let source_hash = hash_files(source_sandbox_files.clone())?;

    if let Some(hash) = source.hash.clone() {
        if hash != source_hash {
            bail!(
                "`source.{}.hash` mismatch: {} != {}",
                source.name,
                source_hash,
                hash
            );
        }
    }

    // 5. Push source

    let registry_request = RegistryPullRequest {
        archive: RegistryArchive::ArtifactSource as i32,
        hash: source_hash.clone(),
    };

    match registry.get_archive(registry_request).await {
        Err(status) => {
            if status.code() != Code::NotFound {
                bail!("registry pull error: {:?}", status);
            }

            info!("{} pack source: {}", get_prefix(artifact_name), source.name);

            let source_sandbox_archive = create_sandbox_file(Some("tar.zst")).await?;

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

            let source_signature =
                vorpal_notary::sign(private_key_path.clone(), &source_archive_data).await?;

            let mut source_stream = vec![];

            for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
                source_stream.push(RegistryPushRequest {
                    archive: RegistryArchive::ArtifactSource as i32,
                    data: chunk.to_vec(),
                    signature: source_signature.clone().to_vec(),
                    hash: source_hash.clone(),
                });
            }

            info!("{} push source: {}", get_prefix(artifact_name), source.name);

            registry
                .push_archive(tokio_stream::iter(source_stream))
                .await
                .expect("failed to push");

            remove_file(&source_sandbox_archive).await?;
        }

        Ok(_) => {}
    }

    remove_dir_all(&source_sandbox)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(source_hash)
}

pub async fn build(
    artifact: &ConfigArtifact,
    artifact_hash: &str,
    artifact_source_hash: &HashMap<String, String>,
    client_artifact: &mut ArtifactServiceClient<Channel>,
    client_registry: &mut RegistryServiceClient<Channel>,
) -> Result<()> {
    // 1. Check artifact

    let artifact_path = get_store_path(&artifact_hash);

    if artifact_path.exists() {
        return Ok(());
    }

    // 2. Pull

    let request_pull = RegistryPullRequest {
        archive: RegistryArchive::Artifact as i32,
        hash: artifact_hash.to_string(),
    };

    match client_registry.pull_archive(request_pull.clone()).await {
        Err(status) => {
            if status.code() != Code::NotFound {
                bail!("registry pull error: {:?}", status);
            }
        }

        Ok(response) => {
            let mut stream = response.into_inner();
            let mut stream_data = Vec::new();

            loop {
                match stream.message().await {
                    Ok(Some(chunk)) => {
                        if !chunk.data.is_empty() {
                            stream_data.extend_from_slice(&chunk.data);
                        }
                    }

                    Ok(None) => break,

                    Err(status) => {
                        if status.code() != Code::NotFound {
                            bail!("registry stream error: {:?}", status);
                        }

                        break;
                    }
                }
            }

            if !stream_data.is_empty() {
                let archive_path = get_archive_path(&artifact_hash);

                write(&archive_path, &stream_data)
                    .await
                    .expect("failed to write archive");

                set_timestamps(&archive_path).await?;

                info!("{} unpack: {}", get_prefix(&artifact.name), artifact_hash);

                create_dir_all(&artifact_path)
                    .await
                    .expect("failed to create artifact path");

                unpack_zstd(&artifact_path, &archive_path).await?;

                let artifact_files = get_file_paths(&artifact_path, vec![], vec![])?;

                if artifact_files.is_empty() {
                    bail!("Artifact files not found: {:?}", artifact_path);
                }

                for artifact_files in &artifact_files {
                    set_timestamps(artifact_files).await?;
                }

                return Ok(());
            }
        }
    };

    // Build

    let request = ArtifactBuildRequest {
        artifact: Some(artifact.clone()),
        artifact_source_hash: artifact_source_hash.clone(),
    };

    let response = client_artifact
        .build(request)
        .await
        .expect("failed to build");

    let mut stream = response.into_inner();

    loop {
        match stream.message().await {
            Ok(Some(response)) => {
                if !response.output.is_empty() {
                    info!("{} {}", get_prefix(&artifact.name), response.output);
                }
            }

            Ok(None) => break,

            Err(err) => {
                bail!("Stream error: {:?}", err);
            }
        };
    }

    Ok(())
}
