use crate::api::{ConfigPackageResponse, ConfigPackageSource, ConfigPackageSourceKind};
use crate::service::config::stream::{send, send_error};
use crate::store::{archives, hashes, paths, temps};
use anyhow::Result;
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use git2::build::RepoBuilder;
use git2::{Cred, RemoteCallbacks};
use reqwest;
use std::path::{Path, PathBuf};
use tokio::fs::{copy, create_dir_all, remove_file, write};
use tokio::sync::mpsc::Sender;
use tokio::task;
use tokio_tar::Archive;
use tonic::Status;
use tracing::info;
use url::Url;

pub async fn validate(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    source_path: &Path,
    source: &ConfigPackageSource,
) -> Result<(String, Vec<PathBuf>), anyhow::Error> {
    let workdir_files = paths::get_file_paths(source_path, &source.ignore_paths)?;

    if workdir_files.is_empty() {
        send_error(tx, "no source files found".to_string()).await?
    }

    send(
        tx,
        format!("source files: {}", workdir_files.len()).into_bytes(),
        None,
    )
    .await?;

    let workdir_files_hashes = hashes::get_files(&workdir_files)?;
    let workdir_hash = hashes::get_source(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        send_error(tx, "no source hash found".to_string()).await?
    }

    send(
        tx,
        format!("source hash: {}", workdir_hash).into_bytes(),
        None,
    )
    .await?;

    if let Some(request_hash) = &source.hash {
        if &workdir_hash != request_hash {
            let message = &format!("hash mismatch: {} != {}", request_hash, workdir_hash);
            send_error(tx, message.to_string()).await?
        }
    }

    Ok((workdir_hash, workdir_files))
}

pub async fn prepare(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    name: &str,
    source: &ConfigPackageSource,
    source_hash: &str,
    source_tar_path: &PathBuf,
) -> Result<String, anyhow::Error> {
    let temp_source_path = temps::create_dir().await?.canonicalize()?;

    send(
        tx,
        format!("preparing source: {:?}", temp_source_path).into_bytes(),
        None,
    )
    .await?;

    if source.kind == ConfigPackageSourceKind::Git as i32 {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("preparing git source: {:?}", &source.uri).into_bytes(),
            package_output: None,
        }))
        .await?;

        let source_uri = source.uri.clone();
        let package_clone_path = temp_source_path.clone();

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
            log_output: format!("preparing git source commit: {:?}", &result).into_bytes(),
            package_output: None,
        }))
        .await?;
    }

    if source.kind == ConfigPackageSourceKind::Http as i32 {
        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("preparing download source: {:?}", &source.uri).into_bytes(),
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
                log_output: format!("preparing download source kind: {:?}", kind).into_bytes(),
                package_output: None,
            }))
            .await?;

            if let "application/gzip" = kind.mime_type() {
                let gz_decoder = GzipDecoder::new(response_bytes);
                let mut archive = Archive::new(gz_decoder);
                archive.unpack(&temp_source_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing download gzip source: {:?}", temp_source_path)
                        .into_bytes(),
                    package_output: None,
                }))
                .await?;
            } else if let "application/x-bzip2" = kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);
                let mut archive = Archive::new(bz_decoder);
                archive.unpack(&temp_source_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing bzip2 source: {:?}", temp_source_path)
                        .into_bytes(),
                    package_output: None,
                }))
                .await?;
            } else if let "application/x-xz" = kind.mime_type() {
                let xz_decoder = XzDecoder::new(response_bytes);
                let mut archive = Archive::new(xz_decoder);
                archive.unpack(&temp_source_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing xz source: {:?}", temp_source_path).into_bytes(),
                    package_output: None,
                }))
                .await?;
            } else if let "application/zip" = kind.mime_type() {
                let temp_zip_path = temps::create_file("zip").await?;
                write(&temp_zip_path, response_bytes).await?;
                archives::unpack_zip(&temp_zip_path, &temp_source_path).await?;
                remove_file(&temp_zip_path).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing zip source: {:?}", temp_source_path)
                        .into_bytes(),
                    package_output: None,
                }))
                .await?;
            } else {
                let file_name = url.path_segments().unwrap().last();
                let file = file_name.unwrap();
                write(&file, response_bytes).await?;
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing source file: {:?}", file).into_bytes(),
                    package_output: None,
                }))
                .await?;
            }
        }
    }

    let source_path = paths::get_package_source_path(name, source_hash);

    if source.kind == ConfigPackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        tx.send(Ok(ConfigPackageResponse {
            log_output: format!("preparing source path: {:?}", source_path).into_bytes(),
            package_output: None,
        }))
        .await?;

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("preparing source kind: {:?}", source_kind).into_bytes(),
                package_output: None,
            }))
            .await?;

            if source_kind.mime_type() == "application/gzip" {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!("preparing packed source: {:?}", temp_source_path)
                        .into_bytes(),
                    package_output: None,
                }))
                .await?;

                archives::unpack_tar_gz(&temp_source_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = temp_source_path.join(source_path.file_name().unwrap());
            copy(&source_path, &dest).await?;
            tx.send(Ok(ConfigPackageResponse {
                log_output: format!(
                    "preparing source file: {:?} -> {:?}",
                    source_path.display(),
                    dest.display()
                )
                .into_bytes(),
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
                    let dest = temp_source_path.join(src.strip_prefix(&source_path)?);
                    create_dir_all(dest).await?;
                    continue;
                }

                let dest = temp_source_path.join(src.strip_prefix(&source_path)?);

                copy(src, &dest).await?;

                tx.send(Ok(ConfigPackageResponse {
                    log_output: format!(
                        "preparing source file: {:?} -> {:?}",
                        source_path.display(),
                        dest.display()
                    )
                    .into_bytes(),
                    package_output: None,
                }))
                .await?;
            }
        }
    }

    // At this point, any source URI should be a local file path

    let (source_hash, _) = validate(tx, &temp_source_path, source).await?;

    create_dir_all(&source_path).await?;

    paths::copy_files(&temp_source_path, &source_path).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("source: {}", source_path.display()).into_bytes(),
        package_output: None,
    }))
    .await?;

    let source_path_files = paths::get_file_paths(&source_path, &Vec::<&str>::new())?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("source tar packing: {}", source_tar_path.display()).into_bytes(),
        package_output: None,
    }))
    .await?;

    archives::compress_tar_gz(&source_path, &source_path_files, source_tar_path).await?;

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("source tar packed: {}", source_tar_path.display()).into_bytes(),
        package_output: None,
    }))
    .await?;

    Ok(source_hash)
}
