use crate::api::package_service_client::PackageServiceClient;
use crate::api::{
    BuildRequest, PackageRequest, PackageResponse, PackageSource, PackageSourceKind, PrepareRequest,
};
use crate::notary;
use crate::store::archives;
use crate::store::hashes;
use crate::store::paths;
use crate::store::temps;
use async_compression::tokio::bufread::BzDecoder;
use git2::build::RepoBuilder;
use git2::{Cred, RemoteCallbacks};
use reqwest;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::fs;
use tokio::fs::{
    copy, create_dir_all, read, remove_dir_all, remove_file, set_permissions, write, File,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Sender;
use tokio_stream;
use tokio_stream::StreamExt;
use tokio_tar::Archive;
use tonic::Status;
use url::Url;

pub async fn prepare(
    tx: &Sender<Result<PackageResponse, Status>>,
    name: &str,
    source: &PackageSource,
) -> Result<(i32, String), anyhow::Error> {
    let workdir = temps::create_dir().await?;
    let workdir_path = workdir.canonicalize()?;

    tx.send(Ok(PackageResponse {
        package_log: format!("preparing working dir: {:?}", workdir_path),
    }))
    .await?;

    if source.kind == PackageSourceKind::Unknown as i32 {
        return Err(anyhow::anyhow!("unknown source kind"));
    }

    if source.kind == PackageSourceKind::Git as i32 {
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

    if source.kind == PackageSourceKind::Http as i32 {
        tx.send(Ok(PackageResponse {
            package_log: format!("preparing download source: {:?}", &source.uri),
        }))
        .await?;

        let url = Url::parse(&source.uri)?;

        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(anyhow::anyhow!("invalid HTTP source URL"));
        }

        let response = reqwest::get(url.as_str()).await?.bytes().await?;
        let response_bytes = response.as_ref();

        if let Some(source_kind) = infer::get(response_bytes) {
            tx.send(Ok(PackageResponse {
                package_log: format!("preparing download source kind: {:?}", source_kind),
            }))
            .await?;

            if let "application/gzip" = source_kind.mime_type() {
                let temp_file = temps::create_file("tar.gz").await?;
                write(&temp_file, response_bytes).await?;
                archives::unpack_tar_gz(&workdir_path, &temp_file).await?;
                remove_file(&temp_file).await?;
                tx.send(Ok(PackageResponse {
                    package_log: format!("preparing download gzip source: {:?}", workdir_path),
                }))
                .await?;
            } else if let "application/x-bzip2" = source_kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);
                let mut archive = Archive::new(bz_decoder);
                archive.unpack(&workdir_path).await?;
                tx.send(Ok(PackageResponse {
                    package_log: format!("preparing bzip2 source: {:?}", workdir_path),
                }))
                .await?;
            } else {
                let source_file_name = url.path_segments().unwrap().last();
                let source_file = source_file_name.unwrap();
                write(&source_file, response_bytes).await?;
                tx.send(Ok(PackageResponse {
                    package_log: format!("preparing source file: {:?}", source_file),
                }))
                .await?;
            }
        }
    }

    if source.kind == PackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        tx.send(Ok(PackageResponse {
            package_log: format!("preparing source path: {:?}", source_path),
        }))
        .await?;

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            tx.send(Ok(PackageResponse {
                package_log: format!("preparing source kind: {:?}", source_kind),
            }))
            .await?;

            if source_kind.mime_type() == "application/gzip" {
                tx.send(Ok(PackageResponse {
                    package_log: format!("preparing packed source: {:?}", workdir),
                }))
                .await?;

                archives::unpack_tar_gz(&workdir_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = workdir_path.join(source_path.file_name().unwrap());
            copy(&source_path, &dest).await?;
            tx.send(Ok(PackageResponse {
                package_log: format!(
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
                tx.send(Ok(PackageResponse {
                    package_log: format!(
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

    let workdir_files = paths::get_file_paths(&workdir_path, &source.ignore_paths)?;

    if workdir_files.is_empty() {
        return Err(anyhow::anyhow!("No source files found"));
    }

    tx.send(Ok(PackageResponse {
        package_log: format!("preparing source files: {:?} found", workdir_files.len()),
    }))
    .await?;

    let workdir_files_hashes = hashes::get_files(&workdir_files)?;
    let workdir_hash = hashes::get_source(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        return Err(anyhow::anyhow!("Failed to get source hash"));
    }

    tx.send(Ok(PackageResponse {
        package_log: format!("preparing source hash: {}", workdir_hash),
    }))
    .await?;

    if let Some(request_hash) = &source.hash {
        if &workdir_hash != request_hash {
            let message = format!("Hash mismatch: {} != {}", request_hash, workdir_hash);
            return Err(anyhow::anyhow!("{}", message));
        }
    }

    let source_tar = temps::create_file("tar.gz").await?;
    let source_tar_path = source_tar.canonicalize()?;

    archives::compress_tar_gz(&workdir_path, &source_tar_path, &workdir_files).await?;

    set_permissions(&source_tar, Permissions::from_mode(0o444)).await?;

    tx.send(Ok(PackageResponse {
        package_log: format!("preparing source tar: {}", source_tar_path.display()),
    }))
    .await?;

    remove_dir_all(&workdir_path).await?;

    let data = read(&source_tar).await?;

    let signature = notary::sign(&data).await?;

    tx.send(Ok(PackageResponse {
        package_log: format!("preparing source tar signature: {}", signature),
    }))
    .await?;

    let mut request_chunks = vec![];
    let request_chunks_size = 8192; // default grpc limit

    for chunk in data.chunks(request_chunks_size) {
        request_chunks.push(PrepareRequest {
            source_data: chunk.to_vec(),
            source_hash: workdir_hash.to_string(),
            source_name: name.to_string(),
            source_signature: signature.to_string(),
        });
    }

    tx.send(Ok(PackageResponse {
        package_log: format!("preparing source chunks: {}", request_chunks.len()),
    }))
    .await?;

    let mut client = PackageServiceClient::connect("http://[::1]:23151").await?;
    let response = client.prepare(tokio_stream::iter(request_chunks)).await?;
    let mut stream = response.into_inner();
    let mut source_id = 0;

    while let Some(chunk) = stream.message().await? {
        tx.send(Ok(PackageResponse {
            package_log: chunk.source_log.to_string(),
        }))
        .await?;

        if chunk.source_id != 0 {
            source_id = chunk.source_id;
        }
    }

    tx.send(Ok(PackageResponse {
        package_log: format!("source id: {}", source_id).to_string(),
    }))
    .await?;

    remove_file(&source_tar).await?;

    Ok((source_id, workdir_hash.to_string()))
}

pub async fn build(
    tx: &Sender<Result<PackageResponse, Status>>,
    source_id: i32,
    source_hash: &str,
    request: &PackageRequest,
) -> Result<(), anyhow::Error> {
    let package = paths::get_package_path(&request.name, source_hash);

    if package.exists() {
        tx.send(Ok(PackageResponse {
            package_log: format!("using existing package: {}", package.display()),
        }))
        .await?;

        return Ok(());
    }

    let package_tar = package.with_extension("tar.gz");

    if package_tar.exists() {
        tx.send(Ok(PackageResponse {
            package_log: format!("unpacking existing package tar: {}", package_tar.display()),
        }))
        .await?;

        fs::create_dir_all(&package).await?;

        archives::unpack_tar_gz(&package, &package_tar).await?;

        tx.send(Ok(PackageResponse {
            package_log: format!("unpacked existing package tar: {}", package.display()),
        }))
        .await?;

        return Ok(());
    }

    let build_args = tonic::Request::new(BuildRequest {
        build_deps: Vec::new(),
        build_phase: request.build_phase.to_string(),
        install_deps: Vec::new(),
        install_phase: request.install_phase.to_string(),
        source_id,
    });

    let mut client = PackageServiceClient::connect("http://[::1]:23151").await?;
    let response = client.build(build_args).await?;
    let mut response_stream = response.into_inner();
    let mut package_archived = false;
    let mut package_data = Vec::new();

    while let Some(build_response) = response_stream.next().await {
        let response = build_response?;

        tx.send(Ok(PackageResponse {
            package_log: response.package_log,
        }))
        .await?;

        if !response.package_data.is_empty() {
            package_data.extend_from_slice(&response.package_data);
        }

        if response.is_archive {
            package_archived = true;
        }
    }

    if package_archived {
        let mut store_tar = File::create(&package_tar).await?;
        if let Err(e) = store_tar.write(&package_data).await {
            return Err(anyhow::anyhow!("Failed to write tar: {:?}", e));
        } else {
            let metadata = fs::metadata(&package_tar).await?;
            let mut permissions = metadata.permissions();

            permissions.set_mode(0o444);

            fs::set_permissions(&package_tar, permissions).await?;
        }

        tx.send(Ok(PackageResponse {
            package_log: format!("stored package tar: {}", package_tar.display()),
        }))
        .await?;

        fs::create_dir_all(&package).await?;

        archives::unpack_tar_gz(&package, &package_tar).await?;

        tx.send(Ok(PackageResponse {
            package_log: format!("unpacked package: {}", package.display()),
        }))
        .await?;

        return Ok(());
    }

    let mut store_file = File::create(&package).await?;
    if let Err(e) = store_file.write(&package_data).await {
        return Err(anyhow::anyhow!("Failed to file: {:?}", e));
    } else {
        let metadata = fs::metadata(&package_tar).await?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o444);
        fs::set_permissions(&package, permissions).await?;
    }

    Ok(())
}
