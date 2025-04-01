use crate::get_prefix;
use anyhow::{bail, Result};
use std::collections::HashMap;
use tokio::fs::{create_dir_all, write};
use tonic::{transport::Channel, Code::NotFound};
use tracing::info;
use vorpal_schema::{
    artifact::v0::{artifact_service_client::ArtifactServiceClient, ArtifactBuildRequest},
    config::v0::ConfigArtifact,
    registry::v0::{
        registry_service_client::RegistryServiceClient, RegistryArchive, RegistryPullRequest,
    },
};
use vorpal_store::{
    archives::unpack_zstd,
    paths::{get_archive_path, get_file_paths, get_store_path, set_timestamps},
};

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
            if status.code() != NotFound {
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
                        if status.code() != NotFound {
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
