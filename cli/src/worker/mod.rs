use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use std::path::Path;
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, write, File};
use tokio::io::AsyncWriteExt;
use tokio_tar::Archive;
use url::Url;
use uuid::Uuid;
use vorpal_schema::api::package::package_service_client::PackageServiceClient;
use vorpal_schema::api::package::{BuildRequest, PackageOutput, PackageSystem};
use vorpal_schema::api::store::store_service_client::StoreServiceClient;
use vorpal_schema::api::store::{StoreKind, StoreRequest};
use vorpal_schema::Package;
use vorpal_store::{
    archives::{compress_zstd, unpack_zip, unpack_zstd},
    hashes::hash_files,
    paths::{
        get_file_paths, get_package_archive_path, get_package_path, get_private_key_path,
        get_source_archive_path,
    },
    temps::create_temp_dir,
};

mod source;
mod stream;

#[derive(Clone, Debug)]
pub struct Worker {
    pub system: PackageSystem,
    pub uri: String,
}

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
    config_hash: &String,
    package: &Package,
    packages: Vec<PackageOutput>,
    workers: &Vec<Worker>,
) -> anyhow::Result<PackageOutput> {
    println!("=> name: {}", package.name);

    if packages.len() > 0 {
        let packages_names = packages
            .clone()
            .into_iter()
            .map(|p| p.name)
            .collect::<Vec<String>>();

        println!("=> packages: {}", packages_names.join(", "));
    }

    let mut package_source_hash = config_hash.clone();
    let mut package_source_type = PackageSourceKind::Unknown;

    if let Some(source) = &package.source {
        package_source_type = match source {
            s if Path::new(s).exists() => PackageSourceKind::Local,
            s if s.starts_with("git") => PackageSourceKind::Git,
            s if s.starts_with("http") => PackageSourceKind::Http,
            _ => anyhow::bail!("Package `source` path not supported."),
        };

        if package_source_type != PackageSourceKind::Local && package.source_hash == None {
            anyhow::bail!("Package `source_hash` not found for remote `source` path");
        }

        if package_source_type == PackageSourceKind::Local {
            let path = Path::new(source).to_path_buf();

            if !path.exists() {
                anyhow::bail!("Package `source` path not found: {:?}", path);
            }

            let source_files = get_file_paths(path, &package.source_ignores)?;

            let (hash, _) = hash_files(source_files).await?;

            if let Some(source_hash) = package.source_hash.clone() {
                if source_hash != hash {
                    anyhow::bail!("Package `source_hash` mismatch: {:?}", source_hash);
                }
            }

            package_source_hash = hash;
        }

        if let Some(hash) = package.source_hash.clone() {
            package_source_hash = hash;
        }
    }

    println!("=> hash: {}", package_source_hash);

    let package_path = get_package_path(&package_source_hash, &package.name);

    if package_path.exists() {
        println!("=> path: {}", package_path.display());

        return Ok(PackageOutput {
            hash: package_source_hash,
            name: package.name.clone(),
        });
    }

    let package_archive_path = get_package_archive_path(&package_source_hash, &package.name);

    if package_archive_path.exists() {
        create_dir_all(&package_path).await?;

        unpack_zstd(&package_path, &package_archive_path).await?;

        println!("=> archive: {}", package_path.display());

        return Ok(PackageOutput {
            hash: package_source_hash,
            name: package.name.clone(),
        });
    }

    let worker = workers.first().unwrap();

    if worker.system != package.target {
        anyhow::bail!("Worker system mismatch: {:?}", worker.system);
    }

    let worker_package = StoreRequest {
        kind: StoreKind::Package as i32,
        name: package.name.clone(),
        hash: package_source_hash.clone(),
    };

    let mut worker_store = StoreServiceClient::connect(worker.uri.clone()).await?;

    if let Ok(_) = worker_store.exists(worker_package.clone()).await {
        println!("=> cache: {:?}", worker_package);

        let worker_store_package = StoreRequest {
            kind: StoreKind::Package as i32,
            name: package.name.clone(),
            hash: package_source_hash.clone(),
        };

        let mut stream = worker_store
            .pull(worker_store_package.clone())
            .await?
            .into_inner();

        let mut stream_data = Vec::new();

        while let Some(chunk) = stream.message().await? {
            if !chunk.data.is_empty() {
                stream_data.extend_from_slice(&chunk.data);
            }
        }

        if stream_data.is_empty() {
            anyhow::bail!("Package stream data empty");
        }

        let stream_data_size = stream_data.len();

        println!("=> fetched: {} bytes", stream_data_size);

        let mut package_archive = File::create(&package_archive_path).await?;

        package_archive.write_all(&stream_data).await?;

        create_dir_all(&package_path).await?;

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
            kind: StoreKind::Source as i32,
            name: package.name.clone(),
            hash: package_source_hash.clone(),
        };

        match worker_store.exists(worker_store_source.clone()).await {
            Ok(_) => println!("=> source cache: {:?}", worker_store_source),
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
                                let url = Url::parse(&source)?;

                                if url.scheme() != "http" && url.scheme() != "https" {
                                    anyhow::bail!(
                                        "Package source URL scheme not supported: {:?}",
                                        url.scheme()
                                    );
                                }

                                let response = reqwest::get(url.as_str()).await?.bytes().await?;
                                let response_bytes = response.as_ref();
                                let temp_dir_path = create_temp_dir().await?;

                                if let Some(kind) = infer::get(response_bytes) {
                                    println!("=> source kind: {}", kind.mime_type());

                                    if let "application/gzip" = kind.mime_type() {
                                        let gz_decoder = GzipDecoder::new(response_bytes);
                                        let mut archive = Archive::new(gz_decoder);

                                        archive.unpack(&temp_dir_path).await?;
                                    } else if let "application/x-bzip2" = kind.mime_type() {
                                        let bz_decoder = BzDecoder::new(response_bytes);
                                        let mut archive = Archive::new(bz_decoder);

                                        archive.unpack(&temp_dir_path).await?;
                                    } else if let "application/x-xz" = kind.mime_type() {
                                        let xz_decoder = XzDecoder::new(response_bytes);
                                        let mut archive = Archive::new(xz_decoder);

                                        archive.unpack(&temp_dir_path).await?;
                                    } else if let "application/zip" = kind.mime_type() {
                                        let temp_file_name = Uuid::now_v7().to_string();
                                        let temp_file = format!("/tmp/{}", temp_file_name);
                                        let temp_file_path = Path::new(&temp_file).to_path_buf();

                                        write(&temp_file_path, response_bytes).await?;
                                        unpack_zip(&temp_file_path, &temp_dir_path).await?;
                                        remove_file(&temp_file_path).await?;
                                    } else {
                                        let file_name =
                                            url.path_segments().unwrap().last().unwrap();
                                        let file_path = temp_dir_path.join(file_name);

                                        write(&file_path, response_bytes).await?;
                                    }

                                    let source_files =
                                        get_file_paths(&temp_dir_path, &package.source_ignores)?;

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

                                    println!("=> source retrieved: {}", url);

                                    compress_zstd(
                                        &temp_dir_path,
                                        &temp_files,
                                        &source_archive_path,
                                    )
                                    .await?;

                                    remove_dir_all(&temp_dir_path).await?;
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

                                let source_files =
                                    get_file_paths(source_path.clone(), &package.source_ignores)?;

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

                    let source_archive_data = read(&source_archive_path).await?;

                    let private_key_path = get_private_key_path();

                    if !private_key_path.exists() {
                        anyhow::bail!("Private key not found: {}", private_key_path.display());
                    }

                    let source_signature =
                        vorpal_notary::sign(private_key_path, &source_archive_data).await?;

                    for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
                        request_stream.push(BuildRequest {
                            package_environment: package.environment.clone(),
                            package_image: None,
                            package_name: package.name.clone(),
                            package_packages: packages.clone(),
                            package_sandbox: false,
                            package_script: package.script.clone(),
                            package_source_data: Some(chunk.to_vec()),
                            package_source_data_signature: Some(source_signature.to_string()),
                            package_source_hash: Some(package_source_hash.clone()),
                            package_systems: package
                                .systems
                                .iter()
                                .map(|s| s.clone() as i32)
                                .collect(),
                            package_target: package.target as i32,
                        });
                    }
                }
            }
        }

        println!("=> source archive: {}", source_archive_path.display());
    }

    if request_stream.is_empty() {
        request_stream.push(BuildRequest {
            package_environment: package.environment.clone(),
            package_image: None,
            package_name: package.name.clone(),
            package_packages: packages.clone(),
            package_sandbox: false,
            package_script: package.script.clone(),
            package_source_data: None,
            package_source_data_signature: None,
            package_source_hash: Some(package_source_hash.clone()),
            package_systems: package.systems.iter().map(|s| s.clone() as i32).collect(),
            package_target: package.target as i32,
        });
    }

    let mut service = PackageServiceClient::connect(worker.uri.clone()).await?;

    let response = service.build(tokio_stream::iter(request_stream)).await?;

    let mut stream = response.into_inner();

    while let Some(res) = stream.message().await? {
        if !res.package_log.is_empty() {
            println!("=> {}", res.package_log);
        }
    }

    Ok(PackageOutput {
        hash: package_source_hash,
        name: package.name.clone(),
    })
}
