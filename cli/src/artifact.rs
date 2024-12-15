use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tokio::{
    fs::{create_dir_all, read, remove_dir_all, remove_file, File},
    io::AsyncWriteExt,
};
use tonic::Code::NotFound;
use tracing::{error, info, warn};
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
        copy_files, get_artifact_path, get_file_paths, get_private_key_path, get_source_path,
        set_timestamps,
    },
    temps::{create_sandbox_dir, create_sandbox_file},
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

async fn fetch_source(sandbox_path: PathBuf, source: ArtifactSource) -> Result<ArtifactSource> {
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

    Ok(source)
}

pub async fn build(
    artifact: &Artifact,
    artifact_id: &ArtifactId,
    artifact_target: ArtifactSystem,
    registry: &str,
    service: &str,
) -> Result<()> {
    // Check if artifact exists (local)

    let artifact_path = get_artifact_path(&artifact_id.hash, &artifact_id.name);

    if artifact_path.exists() {
        info!("[{}] build cache ({})", artifact_id.name, artifact_id.hash);
        return Ok(());
    }

    // Check if artifact exists (registry)

    info!("[{}] pulling...", artifact_id.name);

    let registry_pull = RegistryPullRequest {
        artifact_id: Some(artifact_id.clone()),
        kind: RegistryStoreKind::Artifact as i32,
    };

    let mut registry = RegistryServiceClient::connect(registry.to_owned())
        .await
        .expect("failed to connect to store");

    match registry.pull(registry_pull.clone()).await {
        Err(status) => {
            if status.code() != NotFound {
                bail!("Registry pull error: {:?}", status);
            }
        }

        Ok(response) => {
            let mut response = response.into_inner();
            let mut response_data = Vec::new();

            while let Ok(Some(res)) = response.message().await {
                if !res.data.is_empty() {
                    response_data.extend_from_slice(&res.data);
                }
            }

            if response_data.is_empty() {
                warn!(
                    "[{}] pull failed (missing {}...)",
                    artifact_id.name, artifact_id.hash
                )
            }

            if !response_data.is_empty() {
                let archive_path = create_sandbox_file(Some("tar.zst")).await?;

                let mut archive = File::create(&archive_path)
                    .await
                    .expect("failed to create artifact archive");

                archive
                    .write_all(&response_data)
                    .await
                    .expect("failed to write artifact archive");

                info!("[{}] unpacking...", artifact_id.name);

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
        }
    }

    // Check if artifact source exists (registry)
    let mut artifact_source_path = None;

    if !artifact.sources.is_empty() {
        info!("[{}] pulling source...", artifact_id.name);

        let registry_pull = RegistryPullRequest {
            artifact_id: Some(artifact_id.clone()),
            kind: RegistryStoreKind::ArtifactSource as i32,
        };

        match registry.pull(registry_pull.clone()).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("Registry pull error: {:?}", status);
                }
            }

            Ok(response) => {
                let mut response = response.into_inner();
                let mut response_data = Vec::new();

                while let Ok(Some(res)) = response.message().await {
                    if !res.data.is_empty() {
                        response_data.extend_from_slice(&res.data);
                    }
                }

                if !response_data.is_empty() {
                    let source_archive_path = create_sandbox_file(Some("tar.zst")).await?;

                    let mut archive = File::create(&source_archive_path)
                        .await
                        .expect("failed to create artifact archive");

                    archive
                        .write_all(&response_data)
                        .await
                        .expect("failed to write artifact archive");

                    info!("[{}] unpacking source...", artifact_id.name);

                    let source_path = get_source_path(&artifact_id.hash, &artifact_id.name);

                    create_dir_all(&source_path)
                        .await
                        .expect("failed to create artifact path");

                    unpack_zstd(&source_path, &source_archive_path).await?;

                    remove_file(&source_archive_path)
                        .await
                        .expect("failed to remove");

                    info!("[{}] unpacked source", artifact_id.name);

                    artifact_source_path = Some(source_path);
                }
            }
        }

        if !artifact.sources.is_empty() && artifact_source_path.is_none() {
            let mut source_fetches = vec![];
            let source_path = create_sandbox_dir().await?;

            for artifact_source in &artifact.sources {
                info!(
                    "[{}] preparing source... ({})",
                    artifact_id.name, artifact_source.name
                );

                let source_handle =
                    tokio::spawn(fetch_source(source_path.clone(), artifact_source.clone()));

                source_fetches.push(source_handle);
            }

            for handle in source_fetches {
                match handle.await {
                    Ok(result) => {
                        if let Err(err) = result {
                            bail!("Task error: {:?}", err);
                        }

                        let source = result.unwrap();

                        info!("[{}] prepared source ({})", artifact_id.name, source.name);
                    }
                    Err(e) => error!("Task failed: {}", e),
                }
            }

            // TODO: instead of compiling one source, compile sources for hashes

            info!(
                "[{}] packing source... ({})",
                artifact_id.name, artifact_id.hash
            );

            let source_path_files = get_file_paths(&source_path, vec![], vec![])?;

            let source_archive_path = create_sandbox_file(Some("tar.zst")).await?;

            compress_zstd(&source_path, &source_path_files, &source_archive_path).await?;

            remove_dir_all(&source_path)
                .await
                .expect("failed to remove");

            info!(
                "[{}] packed source ({})",
                artifact_id.name, artifact_id.hash
            );

            let source_archive_data = read(&source_archive_path).await.expect("failed to read");

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

            info!(
                "[{}] pushing source... ({})",
                artifact_id.name, artifact_id.hash
            );

            let response = registry
                .push(tokio_stream::iter(request_stream))
                .await
                .expect("failed to push");

            let response = response.into_inner();

            if !response.success {
                bail!("Registry push failed");
            }

            remove_file(&source_archive_path)
                .await
                .expect("failed to remove");

            info!(
                "[{}] pushed source ({})",
                artifact_id.name, artifact_id.hash
            );
        }
    }

    // Build artifact

    info!("[{}] building...", artifact_id.name);

    let mut worker = ArtifactServiceClient::connect(service.to_owned())
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

    loop {
        match stream.message().await {
            Ok(res) => match res {
                Some(response) => {
                    if !response.output.is_empty() {
                        info!("[{}] {}", artifact_id.name, response.output);
                    }
                }

                None => {
                    info!("[{}] build success", artifact_id.name);

                    break;
                }
            },

            Err(err) => {
                bail!("Stream error: {:?}", err);
            }
        };
    }

    Ok(())
}
