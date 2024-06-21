use crate::api::package_service_client::PackageServiceClient;
use crate::api::store_service_client::StoreServiceClient;
use crate::api::{
    ConfigPackageBuild, ConfigPackageOutput, ConfigPackageRequest, ConfigPackageResponse,
    ConfigPackageSource, ConfigPackageSourceKind, PackageBuildRequest, PackagePrepareRequest,
    PrepareBuildPackage, StorePath, StorePathKind,
};
use crate::notary;
use crate::store::{archives, hashes, paths, temps};
use anyhow::Result;
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use git2::build::RepoBuilder;
use git2::{Cred, RemoteCallbacks};
use reqwest;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::fs::{copy, create_dir_all, read, remove_file, write, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Sender;
use tokio::task;
use tokio_stream;
use tokio_tar::Archive;
use tonic::{Request, Status};
use tracing::info;
use url::Url;

pub async fn run(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    request: Request<ConfigPackageRequest>,
) -> Result<(), anyhow::Error> {
    let config = request.into_inner();

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("[agent] package config name: {}", config.name),
        package_output: None,
    }))
    .await?;

    let config_build = match config.build {
        None => anyhow::bail!("package build config is required"),
        Some(build) => build,
    };

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("[agent] package build config: {:?}", config_build),
        package_output: None,
    }))
    .await?;

    let config_source = match config.source {
        None => anyhow::bail!("package source config is required"),
        Some(source) => source,
    };

    let mut config_source_hash = config_source.hash.clone().unwrap_or_default();

    match config_source.kind() {
        ConfigPackageSourceKind::Unknown => anyhow::bail!("package source kind is unknown"),
        ConfigPackageSourceKind::Git => {}
        ConfigPackageSourceKind::Http => {}
        ConfigPackageSourceKind::Local => {
            if config_source_hash.is_empty() {
                let source_path = Path::new(&config_source.uri).canonicalize()?;
                let (source_hash, _) = validate_source(tx, &source_path, &config_source).await?;

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("[agent] package source hash computed: {}", source_hash),
                    package_output: None,
                }))
                .await?;

                config_source_hash = source_hash;
            }
        }
    };

    if config_source_hash.is_empty() {
        anyhow::bail!("package source hash is required");
    }

    // check package_path exists in agent (local) cache

    let package_path = paths::get_package_path(&config.name, &config_source_hash);

    if package_path.exists() {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] package cache: {}", package_path.display()),
            package_output: Some(ConfigPackageOutput {
                hash: config_source_hash,
                name: config.name,
            }),
        }))
        .await?;

        return Ok(());
    }

    // check package tar exists in agent (local) cache

    let package_tar_path = paths::get_package_tar_path(&config.name, &config_source_hash);

    if !package_path.exists() && package_tar_path.exists() {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] package tar cache: {}", package_tar_path.display()),
            package_output: None,
        }))
        .await?;

        fs::create_dir_all(&package_path).await?;

        archives::unpack_tar_gz(&package_path, &package_tar_path).await?;

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!(
                "[agent] package tar cache unpacked: {}",
                package_path.display()
            ),
            package_output: Some(ConfigPackageOutput {
                hash: config_source_hash,
                name: config.name,
            }),
        }))
        .await?;

        return Ok(());
    }

    // check if package exists in worker (remote) cache

    let mut store_service = StoreServiceClient::connect("http://[::1]:23151").await?;

    let store_package_path = StorePath {
        kind: StorePathKind::Package as i32,
        name: config.name.clone(),
        hash: config_source_hash.clone(),
    };

    if let Ok(res) = store_service.path(store_package_path.clone()).await {
        let store_path = res.into_inner();

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] package tar cache (worker): {}", store_path.uri),
            package_output: None,
        }))
        .await?;

        if let Ok(res) = store_service.fetch(store_package_path.clone()).await {
            let mut stream = res.into_inner();
            let mut stream_data = Vec::new();

            while let Some(chunk) = stream.message().await? {
                if !chunk.data.is_empty() {
                    stream_data.extend_from_slice(&chunk.data);
                }
            }

            if stream_data.is_empty() {
                anyhow::bail!("failed to fetch package tar");
            }

            let mut package_tar = File::create(&package_tar_path).await?;
            if let Err(e) = package_tar.write(&stream_data).await {
                anyhow::bail!(e.to_string());
            }

            tx.send(Ok(ConfigPackageResponse {
                log_output: format!(
                    "[agent] package tar cache (worker): {}",
                    package_tar_path.display()
                ),
                package_output: None,
            }))
            .await?;

            fs::create_dir_all(&package_path).await?;

            archives::unpack_tar_gz(&package_path, &package_tar_path).await?;

            tx.send(Ok(ConfigPackageResponse {
                log_output: format!(
                    "[agent] package tar cache unpacked (worker): {}",
                    package_path.display()
                ),
                package_output: Some(ConfigPackageOutput {
                    hash: config_source_hash,
                    name: config.name,
                }),
            }))
            .await?;

            return Ok(());
        }
    }

    // check if package source exists in worker cache

    let store_package_source_path = StorePath {
        kind: StorePathKind::Source as i32,
        name: config.name.clone(),
        hash: config_source_hash.clone(),
    };

    if let Ok(res) = store_service.path(store_package_source_path.clone()).await {
        let store_path = res.into_inner();

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] package source cache (worker): {}", store_path.uri),
            package_output: None,
        }))
        .await?;

        stream_build(
            tx,
            &config_build,
            &config.name,
            &config_source_hash,
            &store_package_path,
            &mut store_service,
        )
        .await?;

        return Ok(());
    }

    let package_source_tar_path =
        paths::get_package_source_tar_path(&config.name, &config_source_hash);

    if package_source_tar_path.exists() {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!(
                "[agent] package source tar cache: {}",
                package_source_tar_path.display()
            ),
            package_output: None,
        }))
        .await?;

        stream_prepare(
            tx,
            &config.name,
            &config_source_hash,
            &package_source_tar_path,
        )
        .await?;

        stream_build(
            tx,
            &config_build,
            &config.name,
            &config_source_hash,
            &store_package_path,
            &mut store_service,
        )
        .await?;

        return Ok(());
    }

    let source_hash =
        source_prepare(tx, &config.name, &config_source, &package_source_tar_path).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!(
            "[agent] package source tar: {}",
            package_source_tar_path.display()
        ),
        package_output: None,
    }))
    .await?;

    // check if package source exists in worker cache (same as agent)

    if let Ok(res) = store_service.path(store_package_source_path).await {
        let store_path = res.into_inner();

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] package source cache (worker): {}", store_path.uri),
            package_output: None,
        }))
        .await?;

        stream_build(
            tx,
            &config_build,
            &config.name,
            &config_source_hash,
            &store_package_path,
            &mut store_service,
        )
        .await?;

        return Ok(());
    }

    stream_prepare(tx, &config.name, &source_hash, &package_source_tar_path).await?;

    stream_build(
        tx,
        &config_build,
        &config.name,
        &source_hash,
        &store_package_path,
        &mut store_service,
    )
    .await?;

    Ok(())
}

async fn validate_source(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    source_path: &Path,
    source: &ConfigPackageSource,
) -> Result<(String, Vec<PathBuf>), anyhow::Error> {
    let workdir_files = paths::get_file_paths(source_path, &source.ignore_paths)?;

    if workdir_files.is_empty() {
        return Err(anyhow::anyhow!("no source files found"));
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("[agent] package source files: {}", workdir_files.len()),
        package_output: None,
    }))
    .await?;

    let workdir_files_hashes = hashes::get_files(&workdir_files)?;
    let workdir_hash = hashes::get_source(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        return Err(anyhow::anyhow!("failed to get source hash"));
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("[agent] package source hash computed: {}", workdir_hash),
        package_output: None,
    }))
    .await?;

    if let Some(request_hash) = &source.hash {
        if &workdir_hash != request_hash {
            let message = &format!(
                "[agent] hash mismatch: {} != {}",
                request_hash, workdir_hash
            );

            tx.send(Ok(ConfigPackageResponse {
                log_output: message.to_string(),
                package_output: None,
            }))
            .await?;

            return Err(anyhow::anyhow!("{}", message));
        }
    }

    Ok((workdir_hash, workdir_files))
}

async fn stream_build(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    build: &ConfigPackageBuild,
    name: &str,
    source_hash: &str,
    store_package_path: &StorePath,
    store_service: &mut StoreServiceClient<tonic::transport::channel::Channel>,
) -> Result<(), anyhow::Error> {
    let mut build_packages = vec![];

    for output in build.packages.clone().into_iter() {
        build_packages.push(PrepareBuildPackage {
            hash: output.hash,
            name: output.name,
        });
    }

    let build_config = PackageBuildRequest {
        build_environment: build.environment.clone(),
        build_sandbox: build.sandbox,
        build_packages,
        build_script: build.script.to_string(),
        source_name: name.to_string(),
        source_hash: source_hash.to_string(),
    };

    let package_path = paths::get_package_path(name, source_hash);

    let mut package_service = PackageServiceClient::connect("http://[::1]:23151").await?;

    if let Ok(res) = package_service.build(build_config).await {
        let mut build_stream = res.into_inner();

        while let Some(chunk) = build_stream.message().await? {
            if !chunk.log_output.is_empty() {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("[worker] {}", chunk.log_output),
                    package_output: None,
                }))
                .await?;
            }
        }

        let package_tar_path = paths::get_package_tar_path(name, source_hash);

        if !package_tar_path.exists() {
            let fetch_response = store_service.fetch(store_package_path.clone()).await?;
            let mut fetch_stream = fetch_response.into_inner();
            let mut fetch_stream_data = Vec::new();

            while let Some(chunk) = fetch_stream.message().await? {
                if !chunk.data.is_empty() {
                    fetch_stream_data.extend_from_slice(&chunk.data);
                }
            }

            let mut package_tar = File::create(&package_tar_path).await?;
            if let Err(e) = package_tar.write(&fetch_stream_data).await {
                anyhow::bail!(e.to_string());
            }

            tx.send(Ok(ConfigPackageResponse {
                log_output: format!(
                    "[agent] package tar fetched (worker): {}",
                    package_tar_path.display()
                ),
                package_output: None,
            }))
            .await?;

            fs::create_dir_all(package_path.clone()).await?;

            archives::unpack_tar_gz(&package_path, &package_tar_path).await?;

            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("[agent] package tar unpacked: {}", package_path.display()),
                package_output: Some(ConfigPackageOutput {
                    hash: source_hash.to_string(),
                    name: name.to_string(),
                }),
            }))
            .await?;
        }

        // TODO: check if build failed
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("[agent] package output: {}", package_path.display()),
        package_output: Some(ConfigPackageOutput {
            hash: source_hash.to_string(),
            name: name.to_string(),
        }),
    }))
    .await?;

    Ok(())
}

async fn stream_prepare(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    name: &str,
    source_hash: &str,
    source_tar_path: &Path,
) -> Result<(), anyhow::Error> {
    let data = read(&source_tar_path).await?;

    let signature = notary::sign(&data).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("[agent] package source tar signature: {}", signature),
        package_output: None,
    }))
    .await?;

    let mut request_chunks = vec![];
    let request_chunks_size = 8192; // default grpc limit

    for chunk in data.chunks(request_chunks_size) {
        request_chunks.push(PackagePrepareRequest {
            source_data: chunk.to_vec(),
            source_name: name.to_string(),
            source_hash: source_hash.to_string(),
            source_signature: signature.to_string(),
        });
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!(
            "[agent] package source chunks send: {}",
            request_chunks.len()
        ),
        package_output: None,
    }))
    .await?;

    let mut client = PackageServiceClient::connect("http://[::1]:23151").await?;
    let response = client.prepare(tokio_stream::iter(request_chunks)).await?;
    let mut stream = response.into_inner();

    while let Some(chunk) = stream.message().await? {
        if !chunk.log_output.is_empty() {
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("[worker-prepare] {}", chunk.log_output),
                package_output: None,
            }))
            .await?;
        }
    }

    Ok(())
}

async fn source_prepare(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    name: &str,
    source: &ConfigPackageSource,
    source_tar_path: &PathBuf,
) -> Result<String, anyhow::Error> {
    let package_source_path = paths::get_package_source_path(name, source.hash());

    create_dir_all(&package_source_path).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!(
            "[agent] preparing package source: {:?}",
            &package_source_path
        ),
        package_output: None,
    }))
    .await?;

    if source.kind == ConfigPackageSourceKind::Git as i32 {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] preparing git source: {:?}", &source.uri),
            package_output: None,
        }))
        .await?;

        let source_uri = source.uri.clone();
        let package_clone_path = package_source_path.clone();

        let result = task::spawn_blocking(move || {
            let mut builder = RepoBuilder::new();
            let mut fetch_options = git2::FetchOptions::new();
            let mut remote_callbacks = RemoteCallbacks::new();

            remote_callbacks.transfer_progress(|stats| {
                info!(
                    "total: {}, indexed: {}, received: {}, local: {}, total deltas: {}, indexed deltas: {}",
                    stats.total_objects(),
                    stats.indexed_objects(),
                    stats.received_objects(),
                    stats.local_objects(),
                    stats.total_deltas(),
                    stats.indexed_deltas()
                );
                true
            });

            fetch_options.depth(1);

            if source_uri.starts_with("git://") {
                remote_callbacks.credentials(|_url, username_from_url, _allowed_types| {
                    Cred::ssh_key(
                        username_from_url.unwrap(),
                        None,
                        Path::new(&format!(
                            "{}/.ssh/id_rsa",
                            dirs::home_dir().unwrap().display()
                        )),
                        None,
                    )
                });

            }

            fetch_options.remote_callbacks(remote_callbacks);

            builder.fetch_options(fetch_options);

            let repo = builder.clone(&source_uri, &package_clone_path)?;
            let head = repo.head()?;
            let head_commit = repo.find_commit(head.target().unwrap())?;

            Ok::<String, anyhow::Error>(head_commit.id().to_string())
        })
        .await?
        .map_err(|e| anyhow::anyhow!("Failed to clone git source: {}", e))?;

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] preparing git source commit: {:?}", &result),
            package_output: None,
        }))
        .await?;
    }

    if source.kind == ConfigPackageSourceKind::Http as i32 {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] preparing download source: {:?}", &source.uri),
            package_output: None,
        }))
        .await?;

        let url = Url::parse(&source.uri)?;

        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(anyhow::anyhow!("invalid HTTP source URL"));
        }

        let response = reqwest::get(url.as_str()).await?.bytes().await?;
        let response_bytes = response.as_ref();

        if let Some(kind) = infer::get(response_bytes) {
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("[agent] preparing download source kind: {:?}", kind),
                package_output: None,
            }))
            .await?;

            if let "application/gzip" = kind.mime_type() {
                let gz_decoder = GzipDecoder::new(response_bytes);
                let mut archive = Archive::new(gz_decoder);
                archive.unpack(&package_source_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "[agent] preparing download gzip source: {:?}",
                        package_source_path
                    ),
                    package_output: None,
                }))
                .await?;
            } else if let "application/x-bzip2" = kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);
                let mut archive = Archive::new(bz_decoder);
                archive.unpack(&package_source_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "[agent] preparing bzip2 source: {:?}",
                        package_source_path
                    ),
                    package_output: None,
                }))
                .await?;
            } else if let "application/x-xz" = kind.mime_type() {
                let xz_decoder = XzDecoder::new(response_bytes);
                let mut archive = Archive::new(xz_decoder);
                archive.unpack(&package_source_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("[agent] preparing xz source: {:?}", package_source_path),
                    package_output: None,
                }))
                .await?;
            } else if let "application/zip" = kind.mime_type() {
                let temp_zip_path = temps::create_file("zip").await?;
                write(&temp_zip_path, response_bytes).await?;
                archives::unpack_zip(&temp_zip_path, &package_source_path).await?;
                remove_file(&temp_zip_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("[agent] preparing zip source: {:?}", package_source_path),
                    package_output: None,
                }))
                .await?;
            } else {
                let file_name = url.path_segments().unwrap().last();
                let file = file_name.unwrap();
                write(&file, response_bytes).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("[agent] preparing source file: {:?}", file),
                    package_output: None,
                }))
                .await?;
            }
        }
    }

    if source.kind == ConfigPackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("[agent] preparing source path: {:?}", source_path),
            package_output: None,
        }))
        .await?;

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("[agent] preparing source kind: {:?}", source_kind),
                package_output: None,
            }))
            .await?;

            if source_kind.mime_type() == "application/gzip" {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "[agent] preparing packed source: {:?}",
                        package_source_path
                    ),
                    package_output: None,
                }))
                .await?;

                archives::unpack_tar_gz(&package_source_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = package_source_path.join(source_path.file_name().unwrap());
            copy(&source_path, &dest).await?;
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!(
                    "[agent] preparing source file: {:?} -> {:?}",
                    source_path.display(),
                    dest.display()
                ),
                package_output: None,
            }))
            .await?;
        }

        if source_path.is_dir() {
            let file_paths = paths::get_file_paths(&source_path, &source.ignore_paths)?;

            if file_paths.is_empty() {
                return Err(anyhow::anyhow!("No source files found"));
            }

            for src in &file_paths {
                if src.is_dir() {
                    let dest = package_source_path.join(src.strip_prefix(&source_path)?);
                    create_dir_all(dest).await?;
                    continue;
                }

                let dest = package_source_path.join(src.strip_prefix(&source_path)?);

                copy(src, &dest).await?;

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "[agent] preparing source file: {:?} -> {:?}",
                        source_path.display(),
                        dest.display()
                    ),
                    package_output: None,
                }))
                .await?;
            }
        }
    }

    // At this point, any source URI should be a local file path

    let (package_source_hash, package_source_files) =
        validate_source(tx, &package_source_path, source).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!(
            "[agent] package source packing: {}",
            source_tar_path.display()
        ),
        package_output: None,
    }))
    .await?;

    archives::compress_tar_gz(&package_source_path, &package_source_files, source_tar_path).await?;

    Ok(package_source_hash)
}
