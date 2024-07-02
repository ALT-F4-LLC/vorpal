use crate::api::{
    ConfigPackageOutput, ConfigPackageResponse, ConfigPackageSource, ConfigPackageSourceKind,
};
use crate::store::{
    archives::{compress_zstd, unpack_gzip, unpack_zip},
    hashes,
    paths::get_file_paths,
    temps,
};
use anyhow::Result;
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use git2::build::RepoBuilder;
use git2::{Cred, RemoteCallbacks};
use reqwest;
use std::path::{Path, PathBuf};
use tokio::fs::{copy, create_dir_all, remove_dir_all, remove_file, write};
use tokio::sync::mpsc::Sender;
use tokio::task;
use tokio_tar::Archive;
use tonic::Status;
use tracing::{debug, info};
use url::Url;

async fn send_error(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    message: String,
) -> Result<(), anyhow::Error> {
    debug!("send_error: {}", message);

    tx.send(Err(Status::internal(message.clone()))).await?;

    anyhow::bail!(message);
}

async fn send(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    log_output: Vec<u8>,
    package_output: Option<ConfigPackageOutput>,
) -> Result<(), anyhow::Error> {
    debug!("send: {:?}", String::from_utf8(log_output.clone()).unwrap());

    tx.send(Ok(ConfigPackageResponse {
        log_output,
        package_output,
    }))
    .await?;

    Ok(())
}

pub async fn validate(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    source_path: &Path,
    source: &ConfigPackageSource,
) -> Result<(String, Vec<PathBuf>), anyhow::Error> {
    let workdir_files = get_file_paths(source_path, &source.ignore_paths)?;

    if workdir_files.is_empty() {
        send_error(tx, "no source files found".to_string()).await?
    }

    send(
        tx,
        format!("package source files: {}", workdir_files.len()).into_bytes(),
        None,
    )
    .await?;

    let workdir_files_hashes = hashes::get_file_hashes(&workdir_files)?;
    let workdir_hash = hashes::get_source_hash(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        send_error(tx, "no source hash found".to_string()).await?
    }

    send(
        tx,
        format!("package source hash: {}", workdir_hash).into_bytes(),
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
    source: &ConfigPackageSource,
    source_tar_path: &PathBuf,
) -> Result<String, anyhow::Error> {
    let source_sandbox_path = temps::create_dir().await?;
    let source_sandbox_path = source_sandbox_path.canonicalize()?;

    let message = format!(
        "package source sandbox: {:?}",
        source_sandbox_path.file_name().unwrap()
    );

    send(tx, message.into(), None).await?;

    if source.kind == ConfigPackageSourceKind::Git as i32 {
        let message = format!("package source git: {:?}", &source.uri);

        send(tx, message.into(), None).await?;

        let source_uri = source.uri.clone();
        let package_clone_path = source_sandbox_path.clone();

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
        .await?;

        let message = format!("package source commit: {:?}", &result);

        send(tx, message.into(), None).await?;
    }

    if source.kind == ConfigPackageSourceKind::Http as i32 {
        let message = format!("package source url: {:?}", &source.uri);

        send(tx, message.into(), None).await?;

        let url = Url::parse(&source.uri)?;

        if url.scheme() != "http" && url.scheme() != "https" {
            send_error(tx, "invalid scheme".to_string()).await?;
        }

        let response = reqwest::get(url.as_str()).await?.bytes().await?;
        let response_bytes = response.as_ref();

        let source_file_name = url.path_segments().unwrap().last().unwrap();

        if let Some(kind) = infer::get(response_bytes) {
            let message = format!("package source kind: {:?}", kind.mime_type());

            send(tx, message.into(), None).await?;

            if let "application/gzip" = kind.mime_type() {
                let gz_decoder = GzipDecoder::new(response_bytes);

                let mut archive = Archive::new(gz_decoder);

                archive.unpack(&source_sandbox_path).await?;

                let message = format!("package source gzip unpacked: {:?}", source_file_name);

                send(tx, message.into(), None).await?;
            } else if let "application/x-bzip2" = kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);

                let mut archive = Archive::new(bz_decoder);

                archive.unpack(&source_sandbox_path).await?;

                let message = format!("package source bzip2 unpacked: {:?}", source_file_name);

                send(tx, message.into(), None).await?;
            } else if let "application/x-xz" = kind.mime_type() {
                let xz_decoder = XzDecoder::new(response_bytes);

                let mut archive = Archive::new(xz_decoder);

                archive.unpack(&source_sandbox_path).await?;

                let message = format!("package source xz unpacked: {:?}", source_file_name);

                send(tx, message.into(), None).await?;
            } else if let "application/zip" = kind.mime_type() {
                let temp_path = temps::create_file("zip").await?;

                write(&temp_path, response_bytes).await?;

                unpack_zip(&temp_path, &source_sandbox_path).await?;

                remove_file(&temp_path).await?;

                let message = format!("package source zip unpacked: {:?}", source_file_name);

                send(tx, message.into(), None).await?;
            } else {
                let file_name = url.path_segments().unwrap().last();

                let sandbox_file_path = source_sandbox_path.join(file_name.unwrap());

                write(&sandbox_file_path, response_bytes).await?;

                let message = format!("package source file: {:?}", source_file_name);

                send(tx, message.into(), None).await?;
            }
        }
    }

    if source.kind == ConfigPackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        let message = format!("package source local: {:?}", source_path);

        send(tx, message.into(), None).await?;

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            let message = format!("package source kind: {:?}", source_kind.mime_type());

            send(tx, message.into(), None).await?;

            if source_kind.mime_type() == "application/gzip" {
                let message = format!("package source gzip unpacking: {:?}", source_path);

                send(tx, message.into(), None).await?;

                unpack_gzip(&source_sandbox_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = source_sandbox_path.join(source_path.file_name().unwrap());

            copy(&source_path, &dest).await?;

            let message = format!(
                "package source file: {:?} -> {:?}",
                source_path.display(),
                dest.display()
            );

            send(tx, message.into(), None).await?;
        }

        if source_path.is_dir() {
            let source_paths = get_file_paths(&source_path, &source.ignore_paths)?;

            if source_paths.is_empty() {
                send_error(tx, "no source files found".to_string()).await?
            }

            for path in &source_paths {
                if path.is_dir() {
                    let dest = source_sandbox_path.join(path.strip_prefix(&source_path)?);

                    create_dir_all(dest).await?;

                    continue;
                }

                let dest = source_sandbox_path.join(path.strip_prefix(&source_path)?);

                copy(path, &dest).await?;

                let message = format!(
                    "package source file: {:?} -> {:?}",
                    path.display(),
                    dest.display()
                );

                send(tx, message.into(), None).await?;
            }
        }
    }

    // At this point, any source URI should be a local file path

    let (source_sandbox_hash, _) = validate(tx, &source_sandbox_path, source).await?;

    let source_sandbox_file_paths = get_file_paths(&source_sandbox_path, &Vec::<&str>::new())?;

    let message = format!("sandbox source files: {}", source_sandbox_file_paths.len());

    send(tx, message.into(), None).await?;

    compress_zstd(
        &source_sandbox_path,
        &source_sandbox_file_paths,
        source_tar_path,
    )
    .await?;

    let message = format!("package source archive: {:?}", source_tar_path);

    send(tx, message.into(), None).await?;

    remove_dir_all(&source_sandbox_path).await?;

    Ok(source_sandbox_hash)
}
