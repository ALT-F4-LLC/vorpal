use crate::api::config_service_server::ConfigService;
use crate::api::package_service_client::PackageServiceClient;
use crate::api::{
    ConfigPackageRequest, ConfigPackageResponse, ConfigPackageSource, ConfigPackageSourceKind,
    PackageMakeRequest, PackageMakeSource,
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
use url::Url;

#[derive(Debug, Default)]
pub struct Proxy {}

#[tonic::async_trait]
impl ConfigService for Proxy {
    type PackageStream = ReceiverStream<Result<ConfigPackageResponse, Status>>;

    async fn package(
        &self,
        request: Request<ConfigPackageRequest>,
    ) -> Result<Response<Self::PackageStream>, Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let config = request.into_inner();

            let config_source = config
                .source
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("source is required"))?;

            if config_source.kind == ConfigPackageSourceKind::Unknown as i32 {
                return Err(Status::invalid_argument("source kind is required"));
            }

            tx.send(Ok(ConfigPackageResponse {
                log: format!("source name: {}", config_source.name),
            }))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

            let mut config_source_hash = config_source.hash.clone().unwrap_or("".to_string());

            if config_source_hash.is_empty()
                && config_source.kind == ConfigPackageSourceKind::Local as i32
            {
                let source_path = Path::new(&config_source.uri).canonicalize()?;
                let (source_hash, _) = validate_source(&tx, &source_path, &config_source)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;
                config_source_hash = source_hash;
            }

            if config_source_hash.is_empty() {
                return Err(Status::invalid_argument("source hash is required"));
            }

            // we should `config_source_hash` no matter what at this point

            let source_tar_path =
                paths::get_package_source_tar_path(&config_source.name, &config_source_hash);

            if source_tar_path.exists() {
                tx.send(Ok(ConfigPackageResponse {
                    log: format!("source tar exists: {}", source_tar_path.display()),
                }))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

                stream_make(
                    &tx,
                    &config.build_script,
                    &config_source,
                    &config_source_hash,
                    &source_tar_path,
                )
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
                return Ok(());
            }

            let source_hash = package(&tx, &config_source, &source_tar_path)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            stream_make(
                &tx,
                &config.build_script,
                &config_source,
                &source_hash,
                &source_tar_path,
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
        log: format!("preparing source files: {:?} found", workdir_files.len()),
    }))
    .await?;

    let workdir_files_hashes = hashes::get_files(&workdir_files)?;
    let workdir_hash = hashes::get_source(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        return Err(anyhow::anyhow!("Failed to get source hash"));
    }

    tx.send(Ok(ConfigPackageResponse {
        log: format!("preparing source hash: {}", workdir_hash),
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

async fn stream_make(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    build_script: &str,
    source: &ConfigPackageSource,
    source_hash: &str,
    source_tar_path: &Path,
) -> Result<(), anyhow::Error> {
    tx.send(Ok(ConfigPackageResponse {
        log: format!("preparing source tar: {}", source_tar_path.display()),
    }))
    .await?;

    let data = read(&source_tar_path).await?;

    let signature = notary::sign(&data).await?;

    tx.send(Ok(ConfigPackageResponse {
        log: format!("preparing source tar signature: {}", signature),
    }))
    .await?;

    let mut request_chunks = vec![];
    let request_chunks_size = 8192; // default grpc limit

    for chunk in data.chunks(request_chunks_size) {
        request_chunks.push(PackageMakeRequest {
            build_script: build_script.to_string(),
            source: Some(PackageMakeSource {
                data: chunk.to_vec(),
                name: source.name.clone(),
                hash: source_hash.to_string(),
                signature: signature.to_string(),
            }),
        });
    }

    tx.send(Ok(ConfigPackageResponse {
        log: format!("preparing source chunks: {}", request_chunks.len()),
    }))
    .await?;

    let mut client = PackageServiceClient::connect("http://[::1]:23151").await?;
    let response = client.make(tokio_stream::iter(request_chunks)).await?;
    let mut stream = response.into_inner();
    let mut stream_data = Vec::new();

    while let Some(chunk) = stream.message().await? {
        if !chunk.data.is_empty() {
            stream_data.extend_from_slice(&chunk.data);
        }

        if !chunk.log.is_empty() {
            tx.send(Ok(ConfigPackageResponse {
                log: chunk.log.to_string(),
            }))
            .await?;
        }
    }

    let package_tar_path = paths::get_package_tar_path(&source.name, source_hash);

    let mut package_tar = File::create(&package_tar_path).await?;
    if let Err(e) = package_tar.write(&stream_data).await {
        return Err(anyhow::anyhow!("Failed to write tar: {:?}", e));
    }

    tx.send(Ok(ConfigPackageResponse {
        log: format!("stored package tar: {}", package_tar_path.display()),
    }))
    .await?;

    let package_path = paths::get_package_path(&source.name, source_hash);

    fs::create_dir_all(&package_path).await?;

    archives::unpack_tar_gz(&package_path, &package_tar_path).await?;

    tx.send(Ok(ConfigPackageResponse {
        log: format!("unpacked package: {}", package_path.display()),
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
            log: format!("preparing download source: {:?}", &source.uri),
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
                log: format!("preparing download source kind: {:?}", kind),
            }))
            .await?;

            if let "application/gzip" = kind.mime_type() {
                let temp_file = temps::create_file("tar.gz").await?;
                write(&temp_file, response_bytes).await?;
                archives::unpack_tar_gz(&workdir_path, &temp_file).await?;
                remove_file(&temp_file).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log: format!("preparing download gzip source: {:?}", workdir_path),
                }))
                .await?;
            } else if let "application/x-bzip2" = kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);
                let mut archive = Archive::new(bz_decoder);
                archive.unpack(&workdir_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log: format!("preparing bzip2 source: {:?}", workdir_path),
                }))
                .await?;
            } else {
                let file_name = url.path_segments().unwrap().last();
                let file = file_name.unwrap();
                write(&file, response_bytes).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log: format!("preparing source file: {:?}", file),
                }))
                .await?;
            }
        }
    }

    if source.kind == ConfigPackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        tx.send(Ok(ConfigPackageResponse {
            log: format!("preparing source path: {:?}", source_path),
        }))
        .await?;

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            tx.send(Ok(ConfigPackageResponse {
                log: format!("preparing source kind: {:?}", source_kind),
            }))
            .await?;

            if source_kind.mime_type() == "application/gzip" {
                tx.send(Ok(ConfigPackageResponse {
                    log: format!("preparing packed source: {:?}", workdir),
                }))
                .await?;

                archives::unpack_tar_gz(&workdir_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = workdir_path.join(source_path.file_name().unwrap());
            copy(&source_path, &dest).await?;
            tx.send(Ok(ConfigPackageResponse {
                log: format!(
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

                let dest = workdir_path.join(src.file_name().unwrap());
                copy(src, &dest).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log: format!(
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
