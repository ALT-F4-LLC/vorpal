use anyhow::{bail, Result};
use console::style;
use tokio::{
    fs::{create_dir_all, remove_file, File},
    io::AsyncWriteExt,
};
use tonic::Code::NotFound;
use tracing::info;
use vorpal_schema::vorpal::{
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient, Artifact, ArtifactBuildRequest, ArtifactId,
        ArtifactSystem,
    },
    registry::v0::{registry_service_client::RegistryServiceClient, RegistryKind, RegistryRequest},
};
use vorpal_store::{
    archives::unpack_zstd,
    paths::{get_artifact_path, get_file_paths, set_timestamps},
    temps::create_sandbox_file,
};

fn get_prefix(name: &str) -> String {
    style(format!("{} |>", name)).bold().to_string()
}

pub async fn build(
    artifact: &Artifact,
    artifact_id: &ArtifactId,
    artifact_target: ArtifactSystem,
    registry: &str,
    service: &str,
) -> Result<()> {
    // 1. Check if artifact exists (local)

    let artifact_path = get_artifact_path(&artifact_id.hash, &artifact_id.name);

    if artifact_path.exists() {
        return Ok(());
    }

    // 2. Check if artifact exists (registry)

    let pull_request = RegistryRequest {
        hash: artifact_id.hash.clone(),
        kind: RegistryKind::Artifact as i32,
        name: artifact_id.name.clone(),
    };

    let mut registry = RegistryServiceClient::connect(registry.to_owned())
        .await
        .expect("failed to connect to store");

    match registry.exists(pull_request.clone()).await {
        Err(status) => {
            if status.code() != NotFound {
                bail!("Registry pull error: {:?}", status);
            }
        }

        Ok(_) => match registry.pull(pull_request.clone()).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("Registry pull error: {:?}", status);
                }
            }

            Ok(response) => {
                info!(
                    "{} pulling: {}",
                    get_prefix(&artifact_id.name),
                    artifact_id.hash
                );

                let mut response = response.into_inner();
                let mut response_data = Vec::new();

                loop {
                    match response.message().await {
                        Ok(res) => match res {
                            Some(response) => {
                                if !response.data.is_empty() {
                                    response_data.extend_from_slice(&response.data);
                                }
                            }

                            None => break,
                        },

                        Err(err) => {
                            bail!("Stream error: {:?}", err);
                        }
                    };
                }

                if response_data.is_empty() {
                    bail!("artifact data not found: {:?}", artifact_id);
                }

                let archive_path = create_sandbox_file(Some("tar.zst")).await?;

                let mut archive = File::create(&archive_path)
                    .await
                    .expect("failed to create artifact archive");

                archive
                    .write_all(&response_data)
                    .await
                    .expect("failed to write artifact archive");

                info!(
                    "{} unpacking: {}",
                    get_prefix(&artifact_id.name),
                    artifact_id.hash
                );

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

                remove_file(&archive_path).await.expect("failed to remove");

                return Ok(());
            }
        },
    }

    // Build artifact

    let mut worker = ArtifactServiceClient::connect(service.to_owned())
        .await
        .expect("failed to connect to artifact");

    let response = worker
        .build(ArtifactBuildRequest {
            artifact: Some(artifact.clone()),
            system: artifact_target as i32,
        })
        .await
        .expect("failed to build");

    let mut stream = response.into_inner();

    loop {
        match stream.message().await {
            Ok(res) => match res {
                Some(response) => {
                    if !response.output.is_empty() {
                        info!("{} {}", get_prefix(&artifact_id.name), response.output);
                    }
                }

                None => break,
            },

            Err(err) => {
                bail!("Stream error: {:?}", err);
            }
        };
    }

    Ok(())
}
