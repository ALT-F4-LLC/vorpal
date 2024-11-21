use crate::log::{
    print_artifact_archive, print_artifact_hash, print_artifact_log, print_artifact_output,
    print_artifacts_list, print_source_cache, print_source_url, SourceStatus,
};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, read, remove_dir_all, File};
use tokio::io::AsyncWriteExt;
use tonic::Code::NotFound;
use vorpal_schema::vorpal::{
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient, Artifact, ArtifactBuildRequest, ArtifactId,
        ArtifactSource, ArtifactSystem,
    },
    store::v0::{store_service_client::StoreServiceClient, StoreKind, StoreRequest},
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
    target: ArtifactSystem,
    worker: &str,
) -> Result<()> {
    let artifact_path = get_artifact_path(&artifact_id.hash, &artifact_id.name);

    if artifact_path.exists() {
        print_artifact_output(&artifact_id.name, &artifact_id);

        return Ok(());
    }

    // Check if artifact archive exists

    let artifact_archive_path = get_artifact_archive_path(&artifact_id.hash, &artifact_id.name);

    if artifact_archive_path.exists() {
        create_dir_all(&artifact_path)
            .await
            .expect("failed to create artifact path");

        unpack_zstd(&artifact_path, &artifact_archive_path).await?;

        print_artifact_archive(&artifact.name, &artifact_archive_path);

        print_artifact_output(&artifact.name, &artifact_id);

        return Ok(());
    }

    // Check if artifact exists in worker store

    let worker_artifact = StoreRequest {
        hash: artifact_id.hash.clone(),
        kind: StoreKind::Artifact as i32,
        name: artifact_id.name.clone(),
    };

    let mut worker_store = StoreServiceClient::connect(worker.to_owned())
        .await
        .expect("failed to connect to store");

    if (worker_store.exists(worker_artifact.clone()).await).is_ok() {
        println!("=> cache: {:?}", worker_artifact);

        let worker_store_artifact = StoreRequest {
            hash: artifact_id.hash.clone(),
            kind: StoreKind::Artifact as i32,
            name: artifact_id.name.clone(),
        };

        let mut stream = worker_store
            .pull(worker_store_artifact.clone())
            .await
            .expect("failed to pull artifact")
            .into_inner();

        let mut stream_data = Vec::new();

        while let Some(chunk) = stream.message().await.expect("failed to get message") {
            if !chunk.data.is_empty() {
                stream_data.extend_from_slice(&chunk.data);
            }
        }

        if stream_data.is_empty() {
            bail!("Artifact stream data empty");
        }

        let stream_data_size = stream_data.len();

        println!("=> fetched: {} bytes", stream_data_size);

        let mut artifact_archive = File::create(&artifact_archive_path)
            .await
            .expect("failed to create artifact archive");

        artifact_archive
            .write_all(&stream_data)
            .await
            .expect("failed to write artifact archive");

        create_dir_all(&artifact_path)
            .await
            .expect("failed to create artifact path");

        unpack_zstd(&artifact_path, &artifact_archive_path).await?;

        print_artifact_hash(&artifact_id.name, &artifact_id.hash);

        return Ok(());
    }

    // Print artifact dependencies

    if !artifact.artifacts.is_empty() {
        let artifact_list = artifact
            .artifacts
            .clone()
            .into_iter()
            .map(|p| p.name)
            .collect::<Vec<String>>();

        print_artifacts_list(&artifact.name, &artifact_list);
    }

    // Setup artifact build request

    let mut request_stream: Vec<ArtifactBuildRequest> = vec![];

    let mut request_source_data_path = None;

    // Check if artifact source exists in store

    let source_archive_path = get_source_archive_path(&artifact_id.hash, &artifact_id.name);

    if source_archive_path.exists() {
        request_source_data_path = Some(source_archive_path);
    }

    // Check if artifact source exists in worker store

    let worker_store_source = StoreRequest {
        hash: artifact_id.hash.clone(),
        kind: StoreKind::ArtifactSource as i32,
        name: artifact_id.name.clone(),
    };

    if request_source_data_path.is_none() {
        match worker_store.exists(worker_store_source.clone()).await {
            Ok(_) => {
                print_source_cache(
                    &artifact.name,
                    format!("{} => {}-{}", worker, artifact_id.name, artifact_id.hash).as_str(),
                );
            }

            Err(status) => {
                if status.code() == NotFound {
                    let source_archive_path =
                        get_source_archive_path(&artifact_id.hash, &artifact_id.name);

                    if !source_archive_path.exists() {
                        let mut sandbox_fetches = vec![];
                        // let mut sandbox_source_hashes = vec![];

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

                                    // if let Ok(result) = result {
                                    //     sandbox_source_hashes.push(result);
                                    // }
                                }

                                Err(e) => eprintln!("Task failed: {}", e),
                            }
                        }

                        // TODO: instead of compiling one source, compile sources for hashes

                        let sandbox_path_files = get_file_paths(&sandbox_path, vec![], vec![])?;

                        compress_zstd(&sandbox_path, &sandbox_path_files, &source_archive_path)
                            .await?;

                        remove_dir_all(&sandbox_path)
                            .await
                            .expect("failed to remove");
                    }

                    request_source_data_path = Some(source_archive_path);
                }
            }
        }
    }

    // Check if artifact source exists in worker store for same-host.
    // If not found for same-host, then chunks need to be added to request.

    match worker_store.exists(worker_store_source.clone()).await {
        Ok(_) => {
            print_source_cache(&artifact.name, worker);
        }

        Err(status) => {
            if status.code() == NotFound {
                if let Some(source_archive_path) = request_source_data_path {
                    let source_data = read(&source_archive_path).await.expect("failed to read");

                    let private_key_path = get_private_key_path();

                    if !private_key_path.exists() {
                        bail!("Private key not found: {}", private_key_path.display());
                    }

                    let source_signature =
                        vorpal_notary::sign(private_key_path, &source_data).await?;

                    for chunk in source_data.chunks(DEFAULT_CHUNKS_SIZE) {
                        request_stream.push(ArtifactBuildRequest {
                            artifacts: artifact.artifacts.clone(),
                            hash: artifact_id.hash.clone(),
                            name: artifact_id.name.clone(),
                            source_data: Some(chunk.to_vec()),
                            source_data_signature: Some(source_signature.to_vec()),
                            steps: artifact.steps.clone(),
                            target: target as i32,
                        });
                    }
                }
            }
        }
    };

    // Add artifact build request if no source data chunks

    if request_stream.is_empty() {
        request_stream.push(ArtifactBuildRequest {
            artifacts: artifact.artifacts.clone(),
            hash: artifact_id.hash.clone(),
            name: artifact_id.name.clone(),
            source_data: None,
            source_data_signature: None,
            steps: artifact.steps.clone(),
            target: target as i32,
        });
    }

    // Build artifact

    let mut service = ArtifactServiceClient::connect(worker.to_owned())
        .await
        .expect("failed to connect to artifact");

    let response = service
        .build(tokio_stream::iter(request_stream))
        .await
        .expect("failed to build");

    let mut stream = response.into_inner();

    while let Some(res) = stream.message().await.expect("failed to get message") {
        if !res.output.is_empty() {
            print_artifact_log(&artifact.name, &res.output);
        }
    }

    Ok(())
}
