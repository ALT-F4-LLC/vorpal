use crate::api::config_service_server::ConfigService;
use crate::api::package_service_client::PackageServiceClient;
use crate::api::store_service_client::StoreServiceClient;
use crate::api::{
    ConfigPackageBuild, ConfigPackageRequest, ConfigPackageResponse, ConfigPackageSource,
    ConfigPackageSourceKind, PackageBuildRequest, PackagePrepareRequest, StorePath, StorePathKind,
};
use crate::notary;
use crate::store::archives;
use crate::store::hashes;
use crate::store::paths;
use crate::store::temps;
use anyhow::Result;
use async_compression::tokio::bufread::BzDecoder;
use git2::build::RepoBuilder;
use git2::{Cred, RemoteCallbacks};
use reqwest;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::fs::{copy, create_dir_all, read, remove_dir_all, remove_file, write, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream;
use tokio_stream::wrappers::ReceiverStream;
use tokio_tar::Archive;
use tonic::{Request, Response, Status};
use tracing::error;
use url::Url;

#[derive(Debug, Default)]
pub struct Agent {}

#[tonic::async_trait]
impl ConfigService for Agent {
    type PackageStream = ReceiverStream<Result<ConfigPackageResponse, Status>>;

    async fn package(
        &self,
        request: Request<ConfigPackageRequest>,
    ) -> Result<Response<Self::PackageStream>, Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let config = request.into_inner();

            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("package name: {}", config.name),
            }))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

            // check for valid package build and source

            let build = config
                .build
                .ok_or_else(|| Status::invalid_argument("package build config is required"))?;

            let source = config
                .source
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("package source is required"))?;

            if source.kind == ConfigPackageSourceKind::Unknown as i32 {
                return Err(Status::invalid_argument("package source kind is required"));
            }

            // check if we have package source hash

            let mut source_hash = source.hash.clone().unwrap_or("".to_string());

            if source.kind == ConfigPackageSourceKind::Local as i32 && source_hash.is_empty() {
                let source_path = Path::new(&source.uri).canonicalize()?;
                let (hash, _) = validate_source(&tx, &source_path, &source)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("package source hash: {}", hash),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
                source_hash = hash;
            }

            if source_hash.is_empty() {
                return Err(Status::invalid_argument("source hash is required"));
            }

            // check if package exists in agent cache

            let package_path = paths::get_package_path(&config.name, &source_hash);

            if package_path.exists() {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("package already unpacked: {}", package_path.display()),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
                return Ok(());
            }

            // check if package tar exists in agent cache

            let package_tar_path = paths::get_package_tar_path(&config.name, &source_hash);

            if package_tar_path.exists() {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("package tar (cached): {}", package_tar_path.display()),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

                fs::create_dir_all(&package_path).await?;

                archives::unpack_tar_gz(&package_path, &package_tar_path)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "package tar (cached) unpacked: {}",
                        package_path.display()
                    ),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

                return Ok(());
            }

            // check if package exists in worker cache

            let worker_package_path = StorePath {
                kind: StorePathKind::Package as i32,
                name: config.name.clone(),
                hash: source_hash.clone(),
            };

            let mut worker_store = StoreServiceClient::connect("http://[::1]:23151")
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            if let Ok(response) = worker_store.path(worker_package_path.clone()).await {
                let store_path = response.into_inner();

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("package tar cache available: {}", store_path.uri),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

                let fetch_response = worker_store.fetch(worker_package_path).await?;
                let mut fetch_stream = fetch_response.into_inner();
                let mut fetch_stream_data = Vec::new();

                while let Some(chunk) = fetch_stream.message().await? {
                    if !chunk.data.is_empty() {
                        fetch_stream_data.extend_from_slice(&chunk.data);
                    }
                }

                if fetch_stream_data.is_empty() {
                    return Err(Status::internal(
                        "Failed to fetch package cache".to_string(),
                    ));
                }

                let package_tar_path = paths::get_package_tar_path(&config.name, &source_hash);

                let mut package_tar = File::create(&package_tar_path).await?;
                if let Err(e) = package_tar.write(&fetch_stream_data).await {
                    return Err(Status::internal(e.to_string()));
                }

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("package tar downloaded: {}", package_tar_path.display()),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

                let package_path = paths::get_package_path(&config.name, &source_hash);

                fs::create_dir_all(&package_path).await?;

                archives::unpack_tar_gz(&package_path, &package_tar_path)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("package tar unpacked: {}", package_path.display()),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

                return Ok(());
            }

            // check if package source exists in worker cache

            let worker_package_source_path = StorePath {
                kind: StorePathKind::Source as i32,
                name: config.name.clone(),
                hash: source_hash.clone(),
            };

            match worker_store.path(worker_package_source_path.clone()).await {
                Err(e) => error!("worker store path error: {}", e),
                Ok(response) => {
                    let path_response = response.into_inner();

                    tx.send(Ok(ConfigPackageResponse {
                        log_output: format!("package source cache prepared: {}", path_response.uri),
                    }))
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;

                    let mut worker_package = PackageServiceClient::connect("http://[::1]:23151")
                        .await
                        .map_err(|e| Status::internal(e.to_string()))?;

                    let worker_package_request = PackageBuildRequest {
                        build_packages: vec![],
                        build_script: build.script.to_string(),
                        source_name: config.name.clone(),
                        source_hash: source_hash.clone(),
                    };

                    let build_response = worker_package.build(worker_package_request).await?;
                    let mut build_stream = build_response.into_inner();

                    while let Some(chunk) = build_stream.message().await? {
                        if !chunk.log_output.is_empty() {
                            tx.send(Ok(ConfigPackageResponse {
                                log_output: chunk.log_output.to_string(),
                            }))
                            .await
                            .map_err(|e| Status::internal(e.to_string()))?;
                        }
                    }

                    if !package_tar_path.exists() {
                        let fetch_response = worker_store.fetch(worker_package_path).await?;
                        let mut fetch_stream = fetch_response.into_inner();
                        let mut fetch_stream_data = Vec::new();

                        while let Some(chunk) = fetch_stream.message().await? {
                            if !chunk.data.is_empty() {
                                fetch_stream_data.extend_from_slice(&chunk.data);
                            }
                        }

                        let mut package_tar = File::create(&package_tar_path).await?;
                        if let Err(e) = package_tar.write(&fetch_stream_data).await {
                            return Err(Status::internal(e.to_string()));
                        }

                        tx.send(Ok(ConfigPackageResponse {
                            log_output: format!(
                                "package tar (downloaded): {}",
                                package_tar_path.display()
                            ),
                        }))
                        .await
                        .map_err(|e| Status::internal(e.to_string()))?;
                    }

                    let package_path = paths::get_package_path(&config.name, &source_hash);

                    fs::create_dir_all(&package_path).await?;

                    archives::unpack_tar_gz(&package_path, &package_tar_path)
                        .await
                        .map_err(|e| Status::internal(e.to_string()))?;

                    tx.send(Ok(ConfigPackageResponse {
                        log_output: format!("package tar unpacked: {}", package_path.display()),
                    }))
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;

                    return Ok(());
                }
            }

            let package_source_tar_path =
                paths::get_package_source_tar_path(&config.name, &source_hash);

            if package_source_tar_path.exists() {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "package source tar exists: {}",
                        package_source_tar_path.display()
                    ),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

                stream_build(
                    &tx,
                    &build,
                    &config.name,
                    &source_hash,
                    &package_source_tar_path,
                )
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
                return Ok(());
            }

            let source_hash = package(&tx, &source, &package_source_tar_path)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            stream_build(
                &tx,
                &build,
                &config.name,
                &source_hash,
                &package_source_tar_path,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

            Ok(())
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

async fn validate_source(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    source_path: &Path,
    source: &ConfigPackageSource,
) -> Result<(String, Vec<PathBuf>), anyhow::Error> {
    let workdir_files = paths::get_file_paths(source_path, &source.ignore_paths)?;

    if workdir_files.is_empty() {
        return Err(anyhow::anyhow!("No source files found"));
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("package source files: {:?} found", workdir_files.len()),
    }))
    .await?;

    let workdir_files_hashes = hashes::get_files(&workdir_files)?;
    let workdir_hash = hashes::get_source(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        return Err(anyhow::anyhow!("Failed to get source hash"));
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("package source hash computed: {}", workdir_hash),
    }))
    .await?;

    if let Some(request_hash) = &source.hash {
        if &workdir_hash != request_hash {
            let message = format!("Hash mismatch: {} != {}", request_hash, workdir_hash);
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
    source_tar_path: &Path,
) -> Result<(), anyhow::Error> {
    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("package source tar: {}", source_tar_path.display()),
    }))
    .await?;

    let data = read(&source_tar_path).await?;

    let signature = notary::sign(&data).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("package source tar signature: {}", signature),
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
        log_output: format!("package source chunks send: {}", request_chunks.len()),
    }))
    .await?;

    let mut client = PackageServiceClient::connect("http://[::1]:23151").await?;
    let response = client.prepare(tokio_stream::iter(request_chunks)).await?;
    let mut stream = response.into_inner();

    while let Some(chunk) = stream.message().await? {
        if !chunk.log_output.is_empty() {
            tx.send(Ok(ConfigPackageResponse {
                log_output: chunk.log_output.to_string(),
            }))
            .await?;
        }
    }

    let worker_package_request = PackageBuildRequest {
        build_packages: vec![],
        build_script: build.script.to_string(),
        source_name: name.to_string(),
        source_hash: source_hash.to_string(),
    };

    let mut worker_package = PackageServiceClient::connect("http://[::1]:23151")
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let build_response = worker_package.build(worker_package_request).await?;
    let mut build_stream = build_response.into_inner();

    while let Some(chunk) = build_stream.message().await? {
        if !chunk.log_output.is_empty() {
            tx.send(Ok(ConfigPackageResponse {
                log_output: chunk.log_output.to_string(),
            }))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        }
    }

    let mut worker_store = StoreServiceClient::connect("http://[::1]:23151")
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let worker_package_path = StorePath {
        kind: StorePathKind::Package as i32,
        name: name.to_string(),
        hash: source_hash.to_string(),
    };

    let fetch_response = worker_store.fetch(worker_package_path).await?;
    let mut fetch_stream = fetch_response.into_inner();
    let mut fetch_stream_data = Vec::new();

    while let Some(chunk) = fetch_stream.message().await? {
        if !chunk.data.is_empty() {
            fetch_stream_data.extend_from_slice(&chunk.data);
        }
    }

    let package_tar_path = paths::get_package_tar_path(name, source_hash);

    let mut package_tar = File::create(&package_tar_path).await?;
    if let Err(e) = package_tar.write(&fetch_stream_data).await {
        return Err(anyhow::anyhow!("Failed to write tar: {:?}", e));
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("package tar (downloaded): {}", package_tar_path.display()),
    }))
    .await?;

    let package_path = paths::get_package_path(name, source_hash);

    fs::create_dir_all(&package_path).await?;

    archives::unpack_tar_gz(&package_path, &package_tar_path).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("package dir: {}", package_path.display()),
    }))
    .await?;

    Ok(())
}

async fn package(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    source: &ConfigPackageSource,
    source_tar_path: &PathBuf,
) -> Result<String, anyhow::Error> {
    let workdir = temps::create_dir().await?;
    let workdir_path = workdir.canonicalize()?;

    if source.kind == ConfigPackageSourceKind::Git as i32 {
        let mut builder = RepoBuilder::new();

        if source.uri.starts_with("git://") {
            let mut callbacks = RemoteCallbacks::new();

            callbacks.credentials(|_url, username_from_url, _allowed_types| {
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

            let mut fetch_options = git2::FetchOptions::new();

            fetch_options.remote_callbacks(callbacks);

            builder.fetch_options(fetch_options);
        }

        let _ = builder.clone(&source.uri, &workdir_path)?;
    }

    if source.kind == ConfigPackageSourceKind::Http as i32 {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("preparing download source: {:?}", &source.uri),
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
                log_output: format!("preparing download source kind: {:?}", kind),
            }))
            .await?;

            if let "application/gzip" = kind.mime_type() {
                let temp_file = temps::create_file("tar.gz").await?;
                write(&temp_file, response_bytes).await?;
                archives::unpack_tar_gz(&workdir_path, &temp_file).await?;
                remove_file(&temp_file).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing download gzip source: {:?}", workdir_path),
                }))
                .await?;
            } else if let "application/x-bzip2" = kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);
                let mut archive = Archive::new(bz_decoder);
                archive.unpack(&workdir_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing bzip2 source: {:?}", workdir_path),
                }))
                .await?;
            } else {
                let file_name = url.path_segments().unwrap().last();
                let file = file_name.unwrap();
                write(&file, response_bytes).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing source file: {:?}", file),
                }))
                .await?;
            }
        }
    }

    if source.kind == ConfigPackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("preparing source path: {:?}", source_path),
        }))
        .await?;

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("preparing source kind: {:?}", source_kind),
            }))
            .await?;

            if source_kind.mime_type() == "application/gzip" {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing packed source: {:?}", workdir),
                }))
                .await?;

                archives::unpack_tar_gz(&workdir_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = workdir_path.join(source_path.file_name().unwrap());
            copy(&source_path, &dest).await?;
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!(
                    "preparing source file: {:?} -> {:?}",
                    source_path.display(),
                    dest.display()
                ),
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
                    let dest = workdir_path.join(src.strip_prefix(&source_path)?);
                    create_dir_all(dest).await?;
                    continue;
                }

                let dest = workdir_path.join(src.strip_prefix(&source_path)?);

                copy(src, &dest).await?;

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "preparing source file: {:?} -> {:?}",
                        source_path.display(),
                        dest.display()
                    ),
                }))
                .await?;
            }
        }
    }

    // At this point, any source URI should be a local file path

    let (workdir_hash, workdir_files) = validate_source(tx, &workdir_path, source).await?;

    archives::compress_tar_gz(&workdir_path, &source_tar_path, &workdir_files).await?;

    remove_dir_all(&workdir_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to remove workdir: {}", e))?;

    Ok(workdir_hash)
}
