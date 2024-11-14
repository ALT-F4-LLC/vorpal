use crate::log::{
    print_package_archive, print_package_hash, print_package_log, print_package_output,
    print_packages_list, print_source_cache, print_source_url, SourceStatus,
};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, read, remove_dir_all, File};
use tokio::io::AsyncWriteExt;
use tonic::Code::NotFound;
use vorpal_schema::vorpal::{
    package::v0::{Package, PackageOutput, PackageSource, PackageSystem},
    store::v0::{store_service_client::StoreServiceClient, StoreKind, StoreRequest},
    worker::v0::{worker_service_client::WorkerServiceClient, BuildRequest},
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    hashes::{get_hash_digest, get_package_hash, hash_files},
    paths::{
        copy_files, get_file_paths, get_package_archive_path, get_package_path,
        get_private_key_path, get_source_archive_path,
    },
    temps::create_temp_dir,
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

async fn fetch_source(
    sandbox_path: PathBuf,
    package_name: String,
    source: PackageSource,
) -> Result<String> {
    print_source_url(&package_name, SourceStatus::Pending, source.path.as_str());

    let sandbox_source_path = sandbox_path.join(source.name.clone());

    create_dir_all(&sandbox_source_path)
        .await
        .expect("failed to create sandbox path");

    let source_path = Path::new(&source.path).to_path_buf();

    if !source_path.exists() {
        bail!("Package `source` path not found: {:?}", source_path);
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
                bail!("Package source archive found: {:?}", file_path);
            }
        }

        copy_files(&dir_path, dir_files, &sandbox_source_path).await?;
    }

    let source_files = get_file_paths(
        &sandbox_source_path,
        source.excludes.clone(),
        source.includes.clone(),
    )?;

    let source_hash = hash_files(source_files.clone()).await?;

    if let Some(hash) = source.hash.clone() {
        if hash != source_hash {
            bail!("Source hash mismatch: {:?} != {:?}", hash, source_hash);
        }
    }

    print_source_url(&package_name, SourceStatus::Complete, source.path.as_str());

    Ok(source_hash)
}

pub async fn build(
    package: &Package,
    package_outputs: Vec<PackageOutput>,
    package_sandbox: Option<PackageOutput>,
    target: PackageSystem,
    worker: &str,
) -> Result<PackageOutput> {
    let package_json = serde_json::to_value(package).expect("failed to serialize package");

    let package_config = package_json.to_string();

    let package_config_hash = get_hash_digest(&package_config);

    let package_hash = get_package_hash(&package_config_hash, &package.sources).await?;

    // Check if package exists

    let package_path = get_package_path(&package_hash, &package.name);

    if package_path.exists() {
        let output = PackageOutput {
            hash: package_hash,
            name: package.name.clone(),
        };

        print_package_output(&package.name, &output);

        return Ok(output);
    }

    // Check if package archive exists

    let package_archive_path = get_package_archive_path(&package_hash, &package.name);

    if package_archive_path.exists() {
        create_dir_all(&package_path)
            .await
            .expect("failed to create package path");

        unpack_zstd(&package_path, &package_archive_path).await?;

        print_package_archive(&package.name, &package_archive_path);

        let output = PackageOutput {
            hash: package_hash,
            name: package.name.clone(),
        };

        print_package_output(&package.name, &output);

        return Ok(output);
    }

    // Check if package exists in worker store

    let worker_package = StoreRequest {
        kind: StoreKind::Package as i32,
        name: package.name.clone(),
        hash: package_hash.clone(),
    };

    let mut worker_store = StoreServiceClient::connect(worker.to_owned())
        .await
        .expect("failed to connect to store");

    if (worker_store.exists(worker_package.clone()).await).is_ok() {
        println!("=> cache: {:?}", worker_package);

        let worker_store_package = StoreRequest {
            kind: StoreKind::Package as i32,
            name: package.name.clone(),
            hash: package_hash.clone(),
        };

        let mut stream = worker_store
            .pull(worker_store_package.clone())
            .await
            .expect("failed to pull package")
            .into_inner();

        let mut stream_data = Vec::new();

        while let Some(chunk) = stream.message().await.expect("failed to get message") {
            if !chunk.data.is_empty() {
                stream_data.extend_from_slice(&chunk.data);
            }
        }

        if stream_data.is_empty() {
            bail!("Package stream data empty");
        }

        let stream_data_size = stream_data.len();

        println!("=> fetched: {} bytes", stream_data_size);

        let mut package_archive = File::create(&package_archive_path)
            .await
            .expect("failed to create package archive");

        package_archive
            .write_all(&stream_data)
            .await
            .expect("failed to write package archive");

        create_dir_all(&package_path)
            .await
            .expect("failed to create package path");

        unpack_zstd(&package_path, &package_archive_path).await?;

        print_package_hash(&package.name, &package_hash);

        return Ok(PackageOutput {
            hash: package_hash,
            name: package.name.clone(),
        });
    }

    // Print package dependencies

    if !package_outputs.is_empty() {
        let package_list = package_outputs
            .clone()
            .into_iter()
            .map(|p| p.name)
            .collect::<Vec<String>>();

        print_packages_list(&package.name, &package_list);
    }

    // Setup package build request

    let mut request_stream: Vec<BuildRequest> = vec![];

    let mut request_source_data_path = None;

    // Check if package source exists in store

    let source_archive_path = get_source_archive_path(&package_hash, &package.name);

    if source_archive_path.exists() {
        request_source_data_path = Some(source_archive_path);
    }

    // Check if package source exists in worker store

    let worker_store_source = StoreRequest {
        hash: package_hash.clone(),
        kind: StoreKind::Source as i32,
        name: package.name.clone(),
    };

    if request_source_data_path.is_none() {
        match worker_store.exists(worker_store_source.clone()).await {
            Ok(_) => {
                print_source_cache(
                    &package.name,
                    format!("{} => {}-{}", worker, package.name, package_hash).as_str(),
                );
            }
            Err(status) => {
                if status.code() == NotFound {
                    let source_archive_path = get_source_archive_path(&package_hash, &package.name);

                    if !source_archive_path.exists() {
                        let mut sandbox_fetches = vec![];
                        let mut sandbox_source_hashes = vec![];

                        let sandbox_path = create_temp_dir().await?;

                        for package_source in &package.sources {
                            let handle = tokio::spawn(fetch_source(
                                sandbox_path.clone(),
                                package.name.clone(),
                                package_source.clone(),
                            ));

                            sandbox_fetches.push(handle);
                        }

                        for handle in sandbox_fetches {
                            match handle.await {
                                Ok(result) => {
                                    if result.is_err() {
                                        bail!("Task error: {:?}", result);
                                    }

                                    if let Ok(result) = result {
                                        sandbox_source_hashes.push(result);
                                    }
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

    // Check if package source exists in worker store for same-host.
    // If not found for same-host, then chunks need to be added to request.

    match worker_store.exists(worker_store_source.clone()).await {
        Ok(_) => {
            print_source_cache(&package.name, worker);
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
                        request_stream.push(BuildRequest {
                            environment: package.environments.clone(),
                            name: package.name.clone(),
                            packages: package_outputs.clone(),
                            sandbox: package_sandbox.clone(),
                            script: package.script.clone(),
                            source_data: Some(chunk.to_vec()),
                            source_data_signature: Some(source_signature.to_vec()),
                            source_hash: package_hash.clone(),
                            target: target as i32,
                        });
                    }
                }
            }
        }
    };

    // Add package build request if no source data chunks

    if request_stream.is_empty() {
        request_stream.push(BuildRequest {
            environment: package.environments.clone(),
            name: package.name.clone(),
            packages: package_outputs.clone(),
            sandbox: package_sandbox,
            script: package.script.clone(),
            source_data: None,
            source_data_signature: None,
            source_hash: package_hash.clone(),
            target: target as i32,
        });
    }

    // Build package

    let mut service = WorkerServiceClient::connect(worker.to_owned())
        .await
        .expect("failed to connect to package");

    let response = service
        .build(tokio_stream::iter(request_stream))
        .await
        .expect("failed to build");

    let mut stream = response.into_inner();

    while let Some(res) = stream.message().await.expect("failed to get message") {
        if !res.output.is_empty() {
            print_package_log(&package.name, &res.output);
        }
    }

    Ok(PackageOutput {
        hash: package_hash,
        name: package.name.clone(),
    })
}
