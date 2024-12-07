use crate::log::{print_artifact_log, print_artifact_output, print_source_url, SourceStatus};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tokio::{
    fs::{create_dir_all, read, remove_dir_all, remove_file, File},
    io::AsyncWriteExt,
};
use tonic::Code::NotFound;
use vorpal_schema::vorpal::{
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient, Artifact, ArtifactBuildRequest, ArtifactId,
        ArtifactSource, ArtifactSystem,
    },
    registry::v0::{
        registry_service_client::RegistryServiceClient, RegistryPullRequest, RegistryPushRequest,
        RegistryStoreKind,
    },
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_artifact_archive_path, get_artifact_path, get_file_paths,
        get_private_key_path, get_source_archive_path,
    },
    temps::create_temp_dir,
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

async fn fetch_source(
    sandbox_path: PathBuf,
    artifact_name: String,
    source: ArtifactSource,
) -> Result<()> {
    print_source_url(&artifact_name, SourceStatus::Pending, source.path.as_str());

    let sandbox_source_path = sandbox_path.join(source.name.clone());

    create_dir_all(&sandbox_source_path)
        .await
        .expect("failed to create sandbox path");

    let source_path = Path::new(&source.path).to_path_buf();

    if !source_path.exists() {
        bail!("Artifact `source` path not found: {:?}", source_path);
    }

    // TODO: check if source is a directory or file

    if source_path.is_dir() {
        let dir_path = source_path.canonicalize().expect("failed to canonicalize");

        let dir_files = get_file_paths(
            &dir_path.clone(),
            source.excludes.clone(),
            source.includes.clone(),
        )?;

        for file_path in &dir_files {
            if file_path.display().to_string().ends_with(".tar.zst") {
                bail!("Artifact source archive found: {:?}", file_path);
            }
        }

        copy_files(&dir_path, dir_files, &sandbox_source_path).await?;
    }

    Ok(())
}

pub async fn build(
    artifact: &Artifact,
    artifact_id: &ArtifactId,
    artifact_target: ArtifactSystem,
    registry_host: &str,
    worker_host: &str,
) -> Result<()> {
    // Check if artifact exists (local)

    let artifact_path = get_artifact_path(&artifact_id.hash, &artifact_id.name);

    if artifact_path.exists() {
        print_artifact_output(&artifact_id.name, artifact_id);

        return Ok(());
    }

    // Check if artifact exists (registry)

    let registry_pull = RegistryPullRequest {
        artifact_id: Some(artifact_id.clone()),
        kind: RegistryStoreKind::Artifact as i32,
    };

    let mut registry = RegistryServiceClient::connect(registry_host.to_owned())
        .await
        .expect("failed to connect to store");

    match registry.pull(registry_pull.clone()).await {
        Ok(response) => {
            let mut response = response.into_inner();
            let mut response_data = Vec::new();

            while let Ok(message) = response.message().await {
                if message.is_none() {
                    break;
                }

                if let Some(res) = message {
                    if !res.data.is_empty() {
                        response_data.extend_from_slice(&res.data);
                    }
                }
            }

            if !response_data.is_empty() {
                let artifact_archive_path =
                    get_artifact_archive_path(&artifact_id.hash, &artifact_id.name);

                let mut artifact_archive = File::create(&artifact_archive_path)
                    .await
                    .expect("failed to create artifact archive");

                artifact_archive
                    .write_all(&response_data)
                    .await
                    .expect("failed to write artifact archive");

                create_dir_all(&artifact_path)
                    .await
                    .expect("failed to create artifact path");

                unpack_zstd(&artifact_path, &artifact_archive_path).await?;

                remove_file(&artifact_archive_path)
                    .await
                    .expect("failed to remove");

                print_artifact_output(&artifact_id.name, artifact_id);
            }
        }

        Err(status) => {
            if status.code() != NotFound {
                bail!("Registry pull error: {:?}", status);
            }
        }
    }

    // Check if artifact source exists (registry)

    if !artifact.sources.is_empty() {
        let registry_pull = RegistryPullRequest {
            artifact_id: Some(artifact_id.clone()),
            kind: RegistryStoreKind::ArtifactSource as i32,
        };

        match registry.pull(registry_pull.clone()).await {
            Ok(response) => {
                let mut response = response.into_inner();

                // If source doesnt exist, fetch and upload

                if let Err(status) = response.message().await {
                    if status.code() != NotFound {
                        bail!("Registry pull error: {:?}", status);
                    }

                    let mut sandbox_fetches = vec![];
                    let sandbox_path = create_temp_dir().await?;

                    for artifact_source in &artifact.sources {
                        let handle = tokio::spawn(fetch_source(
                            sandbox_path.clone(),
                            artifact.name.clone(),
                            artifact_source.clone(),
                        ));

                        sandbox_fetches.push(handle);
                    }

                    for handle in sandbox_fetches {
                        match handle.await {
                            Ok(result) => {
                                if result.is_err() {
                                    bail!("Task error: {:?}", result);
                                }
                            }
                            Err(e) => eprintln!("Task failed: {}", e),
                        }
                    }

                    // TODO: instead of compiling one source, compile sources for hashes

                    let sandbox_path_files = get_file_paths(&sandbox_path, vec![], vec![])?;

                    let source_archive_path =
                        get_source_archive_path(&artifact_id.hash, &artifact_id.name);

                    compress_zstd(&sandbox_path, &sandbox_path_files, &source_archive_path).await?;

                    remove_dir_all(&sandbox_path)
                        .await
                        .expect("failed to remove");

                    // TODO: upload artifact source archive to registry

                    let source_archive_data =
                        read(&source_archive_path).await.expect("failed to read");

                    let private_key_path = get_private_key_path();

                    if !private_key_path.exists() {
                        bail!("Private key not found: {}", private_key_path.display());
                    }

                    let source_signature =
                        vorpal_notary::sign(private_key_path, &source_archive_data).await?;

                    let mut request_stream = vec![];

                    for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
                        request_stream.push(RegistryPushRequest {
                            artifact_id: Some(artifact_id.clone()),
                            data: chunk.to_vec(),
                            data_signature: source_signature.clone().to_vec(),
                            kind: RegistryStoreKind::ArtifactSource as i32,
                        });
                    }

                    let response = registry
                        .push(tokio_stream::iter(request_stream))
                        .await
                        .expect("failed to push");

                    let response = response.into_inner();

                    if !response.success {
                        bail!("Registry push failed");
                    }
                }
            }

            Err(status) => {
                if status.code() != NotFound {
                    bail!("Registry pull error: {:?}", status);
                }
            }
        }
    }

    // Build artifact

    let mut worker = ArtifactServiceClient::connect(worker_host.to_owned())
        .await
        .expect("failed to connect to artifact");

    let response = worker
        .build(ArtifactBuildRequest {
            artifacts: artifact.artifacts.clone(),
            hash: artifact_id.hash.clone(),
            name: artifact_id.name.clone(),
            steps: artifact.steps.clone(),
            target: artifact_target as i32,
        })
        .await
        .expect("failed to build");

    let mut stream = response.into_inner();

    while let Ok(message) = stream.message().await {
        if message.is_none() {
            break;
        }

        if let Some(res) = message {
            if !res.output.is_empty() {
                print_artifact_log(&artifact.name, &res.output);
            }
        }
    }

    Ok(())
}
