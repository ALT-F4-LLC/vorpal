use crate::log::{
    print_package_archive, print_package_hash, print_package_log, print_package_output,
    print_packages_list, print_source_cache, print_source_url,
};
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use std::path::Path;
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, rename, write, File};
use tokio::io::AsyncWriteExt;
use tokio_tar::Archive;
use tonic::Code::NotFound;
use url::Url;
use uuid::Uuid;
use vorpal_schema::{
    api::{
        package::{
            package_service_client::PackageServiceClient, BuildRequest, PackageOutput,
            PackageSystem,
        },
        store::{store_service_client::StoreServiceClient, StoreKind, StoreRequest},
    },
    get_source_type, Package, PackageSourceKind,
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zip, unpack_zstd},
    hashes::{get_hash_digest, get_package_hash, hash_files},
    paths::{
        copy_files, get_file_paths, get_package_archive_path, get_package_path,
        get_private_key_path, get_source_archive_path,
    },
    temps::create_temp_dir,
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

pub async fn build(
    package: &Package,
    packages: Vec<PackageOutput>,
    target: PackageSystem,
    worker: &str,
) -> Result<PackageOutput> {
    if !packages.is_empty() {
        let package_list = packages
            .clone()
            .into_iter()
            .map(|p| p.name)
            .collect::<Vec<String>>();
        print_packages_list(&package.name, &package_list);
    }

    let package_config = serde_json::to_string(package).expect("failed to serialize package");
    let package_config_hash = get_hash_digest(&package_config);
    let package_hash = get_package_hash(&package_config_hash, &package.source).await?;

    print_package_hash(&package.name, &package_hash);

    let package_path = get_package_path(&package_hash, &package.name);

    if package_path.exists() {
        let output = PackageOutput {
            hash: package_hash,
            name: package.name.clone(),
        };

        print_package_output(&package.name, &output);

        return Ok(output);
    }

    let package_archive_path = get_package_archive_path(&package_hash, &package.name);

    if package_archive_path.exists() {
        create_dir_all(&package_path)
            .await
            .expect("failed to create package path");

        unpack_zstd(&package_path, &package_archive_path).await?;

        print_package_archive(&package.name, &package_archive_path);

        return Ok(PackageOutput {
            hash: package_hash,
            name: package.name.clone(),
        });
    }

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

        println!("=> archive: {:?}", package_path);

        return Ok(PackageOutput {
            hash: package_hash,
            name: package.name.clone(),
        });
    }

    let mut request_stream: Vec<BuildRequest> = vec![];

    let mut request_source_data_path = None;

    let worker_store_source = StoreRequest {
        hash: package_hash.clone(),
        kind: StoreKind::Source as i32,
        name: package.name.clone(),
    };

    match worker_store.exists(worker_store_source.clone()).await {
        Ok(_) => {
            print_source_cache(
                &package.name,
                format!("{} => {}-{}", worker, package.name, package_hash).as_str(),
            );
        }
        Err(status) => {
            if status.code() == NotFound {
                let archive_path = get_source_archive_path(&package_hash, &package.name);

                if !archive_path.exists() {
                    let sandbox_path = create_temp_dir().await?;

                    for (source_name, source) in package.source.iter() {
                        let sandbox_source_path = sandbox_path.join(source_name);

                        create_dir_all(&sandbox_source_path)
                            .await
                            .expect("failed to create sandbox path");

                        let source_type = get_source_type(&source.uri);

                        match source_type {
                            PackageSourceKind::Unknown => bail!("Package source type unknown"),
                            PackageSourceKind::Git => bail!("Package source git not supported"),
                            PackageSourceKind::Http => {
                                let url = Url::parse(&source.uri).expect("failed to parse url");

                                if url.scheme() != "http" && url.scheme() != "https" {
                                    bail!("Source scheme not supported: {:?}", url.scheme());
                                }

                                let response = reqwest::get(url.as_str())
                                    .await
                                    .expect("failed to request source")
                                    .bytes()
                                    .await
                                    .expect("failed to get source bytes");

                                let response_bytes = response.as_ref();

                                if let Some(kind) = infer::get(response_bytes) {
                                    if let "application/gzip" = kind.mime_type() {
                                        let gz_decoder = GzipDecoder::new(response_bytes);
                                        let mut archive = Archive::new(gz_decoder);

                                        archive
                                            .unpack(&sandbox_source_path)
                                            .await
                                            .expect("failed to unpack");
                                    } else if let "application/x-bzip2" = kind.mime_type() {
                                        let bz_decoder = BzDecoder::new(response_bytes);
                                        let mut archive = Archive::new(bz_decoder);

                                        archive
                                            .unpack(&sandbox_source_path)
                                            .await
                                            .expect("failed to unpack");
                                    } else if let "application/x-xz" = kind.mime_type() {
                                        let xz_decoder = XzDecoder::new(response_bytes);
                                        let mut archive = Archive::new(xz_decoder);

                                        archive
                                            .unpack(&sandbox_source_path)
                                            .await
                                            .expect("failed to unpack");
                                    } else if let "application/zip" = kind.mime_type() {
                                        let temp_file_name = Uuid::now_v7().to_string();
                                        let temp_file = format!("/tmp/{}", temp_file_name);
                                        let temp_file_path = Path::new(&temp_file).to_path_buf();

                                        write(&temp_file_path, response_bytes)
                                            .await
                                            .expect("failed to write");

                                        unpack_zip(&temp_file_path, &sandbox_source_path)
                                            .await
                                            .expect("failed to unpack");

                                        remove_file(&temp_file_path)
                                            .await
                                            .expect("failed to remove");
                                    } else {
                                        let file_name =
                                            url.path_segments().unwrap().last().unwrap();

                                        let file_path = sandbox_source_path.join(file_name);

                                        write(&file_path, response_bytes)
                                            .await
                                            .expect("failed to write");
                                    }
                                }
                            }

                            PackageSourceKind::Local => {
                                let source_path = Path::new(&source.uri).to_path_buf();

                                if !source_path.exists() {
                                    bail!("Package `source` path not found: {:?}", source_path);
                                }

                                let source_files = get_file_paths(
                                    &source_path.clone(),
                                    source.excludes.clone(),
                                    source.includes.clone(),
                                )?;

                                for file_path in &source_files {
                                    if file_path.display().to_string().ends_with(".tar.zst") {
                                        bail!("Package source archive found: {:?}", file_path);
                                    }
                                }

                                copy_files(&source_path, source_files, &sandbox_source_path)
                                    .await?;
                            }
                        }

                        let mut source_files = get_file_paths(
                            &sandbox_source_path,
                            source.excludes.clone(),
                            source.includes.clone(),
                        )?;

                        if source.strip_prefix {
                            let mut prefix_path = None;

                            for file_path in &source_files {
                                if *file_path == sandbox_source_path {
                                    continue;
                                }

                                let path = file_path
                                    .strip_prefix(sandbox_source_path.parent().unwrap())
                                    .unwrap();

                                let path_parts: Vec<&str> =
                                    path.to_str().unwrap().split('/').collect();

                                prefix_path = Some(sandbox_source_path.join(path_parts[1]));

                                let path_parts_updated = &path_parts[2_usize..];

                                let path_updated =
                                    sandbox_source_path.join(path_parts_updated.join("/"));

                                println!("=> strip: {:?}", path_updated);

                                if !file_path.is_dir() {
                                    rename(file_path, path_updated)
                                        .await
                                        .expect("failed to rename");
                                } else {
                                    create_dir_all(&path_updated)
                                        .await
                                        .expect("failed to create new path");
                                }
                            }

                            if let Some(prefix) = prefix_path {
                                println!("=> removing prefix: {:?}", prefix.display());
                                remove_dir_all(&prefix).await.expect("failed to remove dir");
                            }

                            let source_files_stripped = get_file_paths(
                                &sandbox_source_path,
                                source.excludes.clone(),
                                source.includes.clone(),
                            )?;

                            if source_files_stripped.len() != (source_files.len() - 1) {
                                bail!(
                                    "Source files stripped mismatch {} != {}",
                                    source_files.len(),
                                    source_files_stripped.len()
                                );
                            }

                            source_files = source_files_stripped;
                        }

                        let source_hash = hash_files(source_files.clone()).await?;

                        if let Some(hash) = source.hash.clone() {
                            if hash != source_hash {
                                bail!("Source hash mismatch: {:?} != {:?}", hash, source_hash);
                            }
                        }

                        print_source_url(&package.name, source.uri.as_str());
                    }

                    let sandbox_path_files = get_file_paths(&sandbox_path, vec![], vec![])?;

                    compress_zstd(&sandbox_path, &sandbox_path_files, &archive_path).await?;

                    remove_dir_all(&sandbox_path)
                        .await
                        .expect("failed to remove");
                }

                request_source_data_path = Some(archive_path);
            }
        }
    }

    if let Some(archive_path) = request_source_data_path {
        let source_data = read(&archive_path).await.expect("failed to read");

        let private_key_path = get_private_key_path();

        if !private_key_path.exists() {
            bail!("Private key not found: {}", private_key_path.display());
        }

        let source_signature = vorpal_notary::sign(private_key_path, &source_data).await?;

        for chunk in source_data.chunks(DEFAULT_CHUNKS_SIZE) {
            request_stream.push(BuildRequest {
                environment: package.environment.clone(),
                name: package.name.clone(),
                packages: packages.clone(),
                sandbox: package.sandbox,
                script: package.script.clone(),
                source_data: Some(chunk.to_vec()),
                source_data_signature: Some(source_signature.to_string()),
                source_hash: Some(package_hash.clone()),
                target: target as i32,
            });
        }
    } else {
        request_stream.push(BuildRequest {
            environment: package.environment.clone(),
            name: package.name.clone(),
            packages: packages.clone(),
            sandbox: package.sandbox,
            script: package.script.clone(),
            source_data: None,
            source_data_signature: None,
            source_hash: Some(package_hash.clone()),
            target: target as i32,
        });
    }

    let mut service = PackageServiceClient::connect(worker.to_owned())
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
