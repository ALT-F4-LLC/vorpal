use crate::api::package_service_client::PackageServiceClient;
use crate::api::{BuildRequest, PackageRequest, PackageResponse, PackageSource, PrepareRequest};
use crate::notary;
use crate::source::{resolve_source, SourceContext};
use crate::store;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::fs;
use tokio::fs::{read, remove_dir_all, remove_file, set_permissions, File};
use tokio::io::AsyncWriteExt;
use tokio_stream;
use tonic::{Request, Response, Status};
use tracing::info;

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
    let data = read(&source_tar).await?;

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
    let workdir = store::create_temp_dir().await?;
    let workdir_path = workdir.canonicalize()?;

    info!("Preparing working dir: {:?}", workdir_path);

    let source_resolver = resolve_source(&source.kind())?;
    if let Err(err) = source_resolver
        .fetch(SourceContext {
            workdir_path: workdir_path.clone(),
            source: source.clone(),
        })
        .await
    {
        eprintln!("Failed to fetch source: {:?}", err);
        return Err(err);
    }

    // At this point, any source URI should be a local file path
    let workdir_files = store::get_file_paths(&workdir_path, &source.ignore_paths)?;

    if workdir_files.is_empty() {
        return Err(anyhow::anyhow!("No source files found"));
    }

    info!("Preparing source files: {:?}", workdir_files);

    let workdir_files_hashes = store::get_file_hashes(&workdir_files)?;
    let workdir_hash = store::get_source_hash(&workdir_files_hashes)?;

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

    let source_tar = store::create_temp_file("tar.gz").await?;
    let source_tar_path = source_tar.canonicalize()?;

    info!("Creating source tar: {:?}", source_tar);

    store::compress_tar_gz(&workdir_path, &source_tar_path, &workdir_files).await?;

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
        let store_path = store::get_store_dir_path();
        let store_path_dir_name = store::get_store_dir_name(&request.name, package_hash);
        let store_path_dir = store_path.join(&store_path_dir_name);
        let store_path_tar = store_path_dir.with_extension("tar.gz");

        if store_path_dir.exists() {
            info!("Using existing source: {}", store_path_dir.display());
            return Ok(());
        }

        if store_path_tar.exists() {
            info!("Using existing tar: {}", store_path_tar.display());

            fs::create_dir_all(&store_path_dir).await?;

            store::unpack_tar_gz(&store_path_dir, &store_path_tar).await?;

            info!("Unpacked source: {}", store_path_dir.display());

            return Ok(());
        }

        let mut store_tar = File::create(&store_path_tar).await?;
        if let Err(e) = store_tar.write(&response_data.package_data).await {
            return Err(anyhow::anyhow!("Failed to write tar: {:?}", e));
        } else {
            let metadata = fs::metadata(&store_path_tar).await?;
            let mut permissions = metadata.permissions();

            permissions.set_mode(0o444);
            fs::set_permissions(&store_path_tar, permissions).await?;

            let file_name = store_path_tar.file_name().unwrap();
            info!("Stored tar: {}", file_name.to_string_lossy());
        }

        info!("Stored tar: {}", store_path_tar.display());

        fs::create_dir_all(&store_path_dir).await?;

        store::unpack_tar_gz(&store_path_dir, &store_path_tar).await?;

        info!("Unpacked source: {}", store_path_dir.display());
    }

    Ok(())
}
