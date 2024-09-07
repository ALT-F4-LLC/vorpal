use crate::log::{
    print_package_archive, print_package_hash, print_package_log, print_package_output,
    print_packages_list, print_source_archive, print_source_cache, print_source_url,
};
use anyhow::Result;
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use std::path::Path;
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, write, File};
use tokio::io::AsyncWriteExt;
use tokio_tar::Archive;
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
    Package,
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zip, unpack_zstd},
    hashes::hash_files,
    paths::{
        get_file_paths, get_package_archive_path, get_package_path, get_private_key_path,
        get_source_archive_path,
    },
    temps::create_temp_dir,
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit
                                         //
#[derive(Debug, PartialEq, Eq)]
enum PackageSourceKind {
    Unknown,
    Local,
    Git,
    Http,
}

pub async fn build(
    config_hash: &str,
    package: &Package,
    packages: Vec<PackageOutput>,
    target: PackageSystem,
    worker: &str,
) -> Result<PackageOutput> {
    if !packages.is_empty() {
        let packages_names = packages
            .clone()
            .into_iter()
            .map(|p| p.name)
            .collect::<Vec<String>>();

        print_packages_list(&package.name, &packages_names);
    }

    let mut package_source_hash = config_hash.to_owned();
    let mut package_source_type = PackageSourceKind::Unknown;

    if let Some(source) = &package.source {
        package_source_type = match source {
            s if Path::new(s).exists() => PackageSourceKind::Local,
            s if s.starts_with("git") => PackageSourceKind::Git,
            s if s.starts_with("http") => PackageSourceKind::Http,
            _ => anyhow::bail!("Package `source` path not supported."),
        };

        if package_source_type != PackageSourceKind::Local && package.source_hash.is_none() {
            anyhow::bail!("Package `source_hash` not found for remote `source` path");
        }

        if package_source_type == PackageSourceKind::Local {
            let path = Path::new(source).to_path_buf();

            if !path.exists() {
                anyhow::bail!("Package `source` path not found: {:?}", path);
            }

            let source_files = get_file_paths(
                &path,
                package.source_excludes.clone(),
                package.source_includes.clone(),
            )?;

            let (hash, _) = hash_files(source_files).await?;

            if let Some(source_hash) = package.source_hash.clone() {
                if source_hash != hash {
                    anyhow::bail!(
                        "Package `source_hash` mismatch: {} != {}",
                        source_hash,
                        hash
                    );
                }
            }

            package_source_hash = hash;
        }

        if let Some(hash) = package.source_hash.clone() {
            package_source_hash = hash;
        }
    }

    print_package_hash(&package.name, &package_source_hash);

    let package_path = get_package_path(&package_source_hash, &package.name);

    if package_path.exists() {
        let output = PackageOutput {
            hash: package_source_hash,
            name: package.name.clone(),
        };

        print_package_output(&package.name, &output);

        return Ok(output);
    }

    let package_archive_path = get_package_archive_path(&package_source_hash, &package.name);

    if package_archive_path.exists() {
        create_dir_all(&package_path)
            .await
            .expect("failed to create package path");

        unpack_zstd(&package_path, &package_archive_path).await?;

        print_package_archive(&package.name, &package_archive_path);

        return Ok(PackageOutput {
            hash: package_source_hash,
            name: package.name.clone(),
        });
    }

    let worker_package = StoreRequest {
        kind: StoreKind::Package as i32,
        name: package.name.clone(),
        hash: package_source_hash.clone(),
    };

    let mut worker_store = StoreServiceClient::connect(worker.to_owned())
        .await
        .expect("failed to connect to store");

    if (worker_store.exists(worker_package.clone()).await).is_ok() {
        println!("=> cache: {:?}", worker_package);

        let worker_store_package = StoreRequest {
            kind: StoreKind::Package as i32,
            name: package.name.clone(),
            hash: package_source_hash.clone(),
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
            anyhow::bail!("Package stream data empty");
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
            hash: package_source_hash,
            name: package.name.clone(),
        });
    }

    let mut request_stream: Vec<BuildRequest> = vec![];

    if let Some(source) = &package.source {
        let source_archive_path = get_source_archive_path(&package_source_hash, &package.name);

        let worker_store_source = StoreRequest {
            hash: package_source_hash.clone(),
            kind: StoreKind::Source as i32,
            name: package.name.clone(),
        };

        match worker_store.exists(worker_store_source.clone()).await {
            Ok(_) => {
                let source_cache = format!("{}/{}-{}", worker, package.name, package_source_hash);
                print_source_cache(&source_cache)
            }
            Err(status) => {
                if status.code() == tonic::Code::NotFound {
                    if !source_archive_path.exists() {
                        match package_source_type {
                            PackageSourceKind::Unknown => {
                                anyhow::bail!("Package source type unknown")
                            }

                            PackageSourceKind::Git => {
                                anyhow::bail!("Package source git not supported")
                            }

                            PackageSourceKind::Http => {
                                let url = Url::parse(source).expect("failed to parse source url");

                                if url.scheme() != "http" && url.scheme() != "https" {
                                    anyhow::bail!(
                                        "Package source URL scheme not supported: {:?}",
                                        url.scheme()
                                    );
                                }

                                let response = reqwest::get(url.as_str())
                                    .await
                                    .expect("failed to request source")
                                    .bytes()
                                    .await
                                    .expect("failed to get source bytes");

                                let response_bytes = response.as_ref();

                                let temp_dir_path = create_temp_dir().await?;

                                if let Some(kind) = infer::get(response_bytes) {
                                    if let "application/gzip" = kind.mime_type() {
                                        let gz_decoder = GzipDecoder::new(response_bytes);
                                        let mut archive = Archive::new(gz_decoder);

                                        archive
                                            .unpack(&temp_dir_path)
                                            .await
                                            .expect("failed to unpack");
                                    } else if let "application/x-bzip2" = kind.mime_type() {
                                        let bz_decoder = BzDecoder::new(response_bytes);
                                        let mut archive = Archive::new(bz_decoder);

                                        archive
                                            .unpack(&temp_dir_path)
                                            .await
                                            .expect("failed to unpack");
                                    } else if let "application/x-xz" = kind.mime_type() {
                                        let xz_decoder = XzDecoder::new(response_bytes);
                                        let mut archive = Archive::new(xz_decoder);

                                        archive
                                            .unpack(&temp_dir_path)
                                            .await
                                            .expect("failed to unpack");
                                    } else if let "application/zip" = kind.mime_type() {
                                        let temp_file_name = Uuid::now_v7().to_string();
                                        let temp_file = format!("/tmp/{}", temp_file_name);
                                        let temp_file_path = Path::new(&temp_file).to_path_buf();

                                        write(&temp_file_path, response_bytes)
                                            .await
                                            .expect("failed to write");

                                        unpack_zip(&temp_file_path, &temp_dir_path).await?;

                                        remove_file(&temp_file_path)
                                            .await
                                            .expect("failed to remove");
                                    } else {
                                        let file_name =
                                            url.path_segments().unwrap().last().unwrap();
                                        let file_path = temp_dir_path.join(file_name);

                                        write(&file_path, response_bytes)
                                            .await
                                            .expect("failed to write");
                                    }

                                    let source_files = get_file_paths(
                                        &temp_dir_path,
                                        package.source_excludes.clone(),
                                        package.source_includes.clone(),
                                    )?;

                                    let (temp_hash, temp_files) = hash_files(source_files).await?;

                                    if let Some(source_hash) = package.source_hash.clone() {
                                        if source_hash != temp_hash {
                                            anyhow::bail!(
                                                "Package source hash mismatch: {:?} != {:?}",
                                                source_hash,
                                                temp_hash
                                            );
                                        }
                                    }

                                    print_source_url(&package.name, url.as_str());

                                    compress_zstd(
                                        &temp_dir_path,
                                        &temp_files,
                                        &source_archive_path,
                                    )
                                    .await?;

                                    remove_dir_all(&temp_dir_path)
                                        .await
                                        .expect("failed to remove");
                                }
                            }

                            PackageSourceKind::Local => {
                                let source_path = Path::new(&source).to_path_buf();

                                if !source_path.exists() {
                                    anyhow::bail!(
                                        "Package `source` path not found: {:?}",
                                        source_path
                                    );
                                }

                                let source_files = get_file_paths(
                                    &source_path.clone(),
                                    package.source_excludes.clone(),
                                    package.source_includes.clone(),
                                )?;

                                for file_path in &source_files {
                                    if file_path.display().to_string().ends_with(".tar.zst") {
                                        anyhow::bail!(
                                            "Package source archive found: {:?}",
                                            file_path
                                        );
                                    }
                                }

                                compress_zstd(&source_path, &source_files, &source_archive_path)
                                    .await?;
                            }
                        }
                    }

                    if !source_archive_path.exists() {
                        anyhow::bail!(
                            "Package source archive not found: {}",
                            source_archive_path.display()
                        );
                    }

                    let source_archive_data =
                        read(&source_archive_path).await.expect("failed to read");

                    let private_key_path = get_private_key_path();

                    if !private_key_path.exists() {
                        anyhow::bail!("Private key not found: {}", private_key_path.display());
                    }

                    let source_signature =
                        vorpal_notary::sign(private_key_path, &source_archive_data).await?;

                    for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
                        request_stream.push(BuildRequest {
                            environment: package.environment.clone(),
                            name: package.name.clone(),
                            packages: packages.clone(),
                            sandbox: false,
                            script: package.script.clone(),
                            source_data: Some(chunk.to_vec()),
                            source_data_signature: Some(source_signature.to_string()),
                            source_hash: Some(package_source_hash.clone()),
                            target: target as i32,
                        });
                    }
                }
            }
        }

        print_source_archive(&package.name, &source_archive_path.display().to_string());
    }

    if request_stream.is_empty() {
        request_stream.push(BuildRequest {
            environment: package.environment.clone(),
            name: package.name.clone(),
            packages: packages.clone(),
            sandbox: false,
            script: package.script.clone(),
            source_data: None,
            source_data_signature: None,
            source_hash: Some(package_source_hash.clone()),
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
        hash: package_source_hash,
        name: package.name.clone(),
    })
}
