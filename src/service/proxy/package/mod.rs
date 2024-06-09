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
use tokio_stream;
use tokio_tar::Archive;
use tonic::{Request, Response, Status};
use tracing::info;
use url::Url;

pub async fn run(request: Request<PackageRequest>) -> Result<Response<PackageResponse>, Status> {
    let req = request.into_inner();

    let req_source = req
        .source
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("source is required"))?;

    info!("Preparing: {}", req.name);

    let (source_id, source_hash) = prepare(&req.name, req_source)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    info!("Building: {}-{}", req.name, source_hash);

    build(source_id, &source_hash, &req)
        .await
        .map_err(|e| Status::internal(format!("Failed to build package: {}", e)))?;

    let response = PackageResponse {
        source_id: source_id.to_string(),
        source_hash,
    };

    Ok(Response::new(response))
}

async fn prepare_source<P: AsRef<Path>>(
    source_name: &str,
    source_hash: &str,
    source_tar: &P,
) -> Result<(i32, String), anyhow::Error> {
    let data = read(source_tar).await?;

    let signature = notary::sign(&data).await?;

    info!("Source tar signature: {}", signature);

    let mut request_chunks = vec![];
    let request_chunks_size = 8192; // default grpc limit

    for chunk in data.chunks(request_chunks_size) {
        request_chunks.push(PrepareRequest {
            source_data: chunk.to_vec(),
            source_hash: source_hash.to_string(),
            source_name: source_name.to_string(),
            source_signature: signature.to_string(),
        });
    }

    info!("Request chunks: {}", request_chunks.len());

    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;
    let response = client.prepare(tokio_stream::iter(request_chunks)).await?;
    let res = response.into_inner();

    info!("Source ID: {}", res.source_id);

    remove_file(&source_tar).await?;

    Ok((res.source_id, source_hash.to_string()))
}

async fn prepare(name: &str, source: &PackageSource) -> Result<(i32, String), anyhow::Error> {
    let workdir = temps::create_dir().await?;
    let workdir_path = workdir.canonicalize()?;

    info!("Preparing working dir: {:?}", workdir_path);

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
        info!("Downloading source: {:?}", &source.uri);

        let url = Url::parse(&source.uri)?;

        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(anyhow::anyhow!("invalid HTTP source URL"));
        }

        let response = reqwest::get(url.as_str()).await?.bytes().await?;
        let response_bytes = response.as_ref();

        if let Some(source_kind) = infer::get(response_bytes) {
            info!("Preparing source kind: {:?}", source_kind);

            if let "application/gzip" = source_kind.mime_type() {
                let temp_file = temps::create_file("tar.gz").await?;
                write(&temp_file, response_bytes).await?;
                archives::unpack_tar_gz(&workdir_path, &temp_file).await?;
                remove_file(&temp_file).await?;
                info!("Prepared gzip source: {:?}", workdir_path);
            } else if let "application/x-bzip2" = source_kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);
                let mut archive = Archive::new(bz_decoder);
                archive.unpack(&workdir_path).await?;
                info!("Prepared bzip2 source: {:?}", workdir_path);
            } else {
                let source_file_name = url.path_segments().unwrap().last();
                let source_file = source_file_name.unwrap();
                write(&source_file, response_bytes).await?;
                info!("Prepared source file: {:?}", source_file);
            }
        }
    }

    if source.kind == PackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        info!("Preparing source path: {:?}", source_path);

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            info!("Preparing source kind: {:?}", source_kind);

            if source_kind.mime_type() == "application/gzip" {
                info!("Preparing packed source: {:?}", workdir);
                archives::unpack_tar_gz(&workdir_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = workdir_path.join(source_path.file_name().unwrap());
            copy(&source_path, &dest).await?;
            info!(
                "Preparing source file: {:?} -> {:?}",
                source_path.display(),
                dest.display()
            );
        }

        if source_path.is_dir() {
            let files = paths::get_files(&source_path, &source.ignore_paths)?;

            if files.is_empty() {
                return Err(anyhow::anyhow!("No source files found"));
            }

            for src in &files {
                if src.is_dir() {
                    let dest = workdir_path.join(src.strip_prefix(&source_path)?);
                    create_dir_all(dest).await?;
                    continue;
                }

                let dest = workdir_path.join(src.file_name().unwrap());
                copy(src, &dest).await?;
                info!(
                    "Preparing source file: {:?} -> {:?}",
                    src.display(),
                    dest.display()
                );
            }
        }
    }

    // At this point, any source URI should be a local file path

    let workdir_files = paths::get_files(&workdir_path, &source.ignore_paths)?;

    if workdir_files.is_empty() {
        return Err(anyhow::anyhow!("No source files found"));
    }

    info!("Preparing source files: {:?}", workdir_files);

    let workdir_files_hashes = hashes::get_files(&workdir_files)?;
    let workdir_hash = hashes::get_source(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        return Err(anyhow::anyhow!("Failed to get source hash"));
    }

    info!("Source hash: {}", workdir_hash);

    if let Some(request_hash) = &source.hash {
        if &workdir_hash != request_hash {
            let message = format!("Hash mismatch: {} != {}", request_hash, workdir_hash);
            return Err(anyhow::anyhow!("{}", message));
        }
    }

    let source_tar = temps::create_file("tar.gz").await?;
    let source_tar_path = source_tar.canonicalize()?;

    info!("Creating source tar: {:?}", source_tar);

    archives::compress_tar_gz(&workdir_path, &source_tar_path, &workdir_files).await?;

    set_permissions(&source_tar, Permissions::from_mode(0o444)).await?;

    info!("Source tar: {}", source_tar_path.display());

    remove_dir_all(&workdir_path).await?;

    prepare_source(name, &workdir_hash, &source_tar_path).await
}

async fn build(
    package_id: i32,
    package_hash: &str,
    request: &PackageRequest,
) -> Result<(), anyhow::Error> {
    let build_args = tonic::Request::new(BuildRequest {
        build_deps: Vec::new(),
        build_phase: request.build_phase.to_string(),
        install_deps: Vec::new(),
        install_phase: request.install_phase.to_string(),
        source_id: package_id,
    });
    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;
    let response = client.build(build_args).await?;
    let response_data = response.into_inner();

    if response_data.is_compressed {
        let package = paths::get_package(&request.name, package_hash);
        let package_tar = package.with_extension("tar.gz");

        if package.exists() {
            info!("Using existing package: {}", package.display());
            return Ok(());
        }

        if package_tar.exists() {
            info!("Using existing package tar: {}", package_tar.display());

            fs::create_dir_all(&package).await?;
            archives::unpack_tar_gz(&package, &package_tar).await?;

            info!("Unpacked existing package tar: {}", package.display());

            return Ok(());
        }

        let mut store_tar = File::create(&package_tar).await?;
        if let Err(e) = store_tar.write(&response_data.package_data).await {
            return Err(anyhow::anyhow!("Failed to write tar: {:?}", e));
        } else {
            let metadata = fs::metadata(&package_tar).await?;
            let mut permissions = metadata.permissions();

            permissions.set_mode(0o444);
            fs::set_permissions(&package_tar, permissions).await?;

            let file_name = package_tar.file_name().unwrap();
            info!("Stored tar: {}", file_name.to_string_lossy());
        }

        info!("Stored package tar: {}", package_tar.display());

        fs::create_dir_all(&package).await?;
        archives::unpack_tar_gz(&package, &package_tar).await?;

        info!("Stored package: {}", package.display());
    }

    Ok(())
}
