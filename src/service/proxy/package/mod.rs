use crate::api::package_service_client::PackageServiceClient;
use crate::api::{
    BuildRequest, PackageRequest, PackageResponse, PackageSource, PackageSourceKind, PrepareRequest,
};
use crate::notary;
use crate::store;
use git2::build::RepoBuilder;
use git2::{Cred, RemoteCallbacks};
use reqwest;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use tempfile::{tempdir, NamedTempFile};
use tokio::fs;
use tokio::fs::{copy, create_dir_all, read, set_permissions, write, File};
use tokio::io::AsyncWriteExt;
use tokio_stream;
use tonic::{Request, Response, Status};
use url::Url;

pub async fn run(request: Request<PackageRequest>) -> Result<Response<PackageResponse>, Status> {
    let req = request.into_inner();

    let req_source = req.source.as_ref().ok_or_else(|| {
        eprintln!("Source is required");
        Status::invalid_argument("Source is required")
    })?;

    println!("Preparing: {}", req.name);

    let (source_id, source_hash) = prepare(&req.name, req_source).await.map_err(|e| {
        eprintln!("Failed to prepare: {:?}", e);
        Status::internal(e.to_string())
    })?;

    println!("Building: {}-{}", req.name, source_hash);

    build(source_id, &source_hash, &req).await.map_err(|e| {
        eprintln!("Failed to build: {:?}", e);
        Status::internal(e.to_string())
    })?;

    let response = PackageResponse {
        source_id: source_id.to_string(),
        source_hash,
    };

    Ok(Response::new(response))
}

async fn prepare_source(
    source_name: &str,
    source_hash: &str,
    source_tar: &PathBuf,
) -> Result<(i32, String), anyhow::Error> {
    let data = read(source_tar).await?;

    let signature = notary::sign(&data).await?;

    println!("Source tar signature: {}", signature);

    let mut request_chunks = vec![];
    let request_chunks_size = 2 * 1024 * 1024; // default grpc limit

    for chunk in data.chunks(request_chunks_size) {
        request_chunks.push(PrepareRequest {
            source_data: chunk.to_vec(),
            source_hash: source_hash.to_string(),
            source_name: source_name.to_string(),
            source_signature: signature.to_string(),
        });
    }

    println!("Request chunks: {}", request_chunks.len());

    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;
    let response = client.prepare(tokio_stream::iter(request_chunks)).await?;
    let res = response.into_inner();

    println!("Source ID: {}", res.source_id);

    Ok((res.source_id, source_hash.to_string()))
}

async fn prepare(name: &String, source: &PackageSource) -> Result<(i32, String), anyhow::Error> {
    let work_dir = tempdir()?;
    let workdir_path = work_dir.path().canonicalize()?;

    println!("Preparing working dir: {:?}", workdir_path);

    if source.kind == PackageSourceKind::Unknown as i32 {
        return Err(anyhow::anyhow!("Unknown source kind"));
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
        println!("Downloading source: {:?}", &source.uri);

        let url = Url::parse(&source.uri)?;

        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(anyhow::anyhow!("Invalid HTTP source URL"));
        }

        let response = reqwest::get(url.as_str()).await?.bytes().await?;
        let response_bytes = response.as_ref();

        if let Some(source_kind) = infer::get(response_bytes) {
            println!("Preparing source kind: {:?}", source_kind);

            match source_kind.mime_type() {
                "application/gzip" => {
                    let temp_file = NamedTempFile::new()?;
                    write(&temp_file, response_bytes).await?;
                    store::unpack_source(&workdir_path, temp_file.path())?;
                    println!("Prepared packed source: {:?}", workdir_path);
                }
                _ => {
                    let source_file_name = url.path_segments().unwrap().last();
                    let source_file = source_file_name.unwrap();
                    write(&source_file, response_bytes).await?;
                    println!("Prepared source file: {:?}", source_file);
                }
            }
        }
    }

    if source.kind == PackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        println!("Preparing source path: {:?}", source_path);

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            println!("Preparing source kind: {:?}", source_kind);

            if source_kind.mime_type() == "application/gzip" {
                println!("Preparing packed source: {:?}", work_dir);
                store::unpack_source(&workdir_path, &source_path)?;
            }
        }

        if source_path.is_file() {
            let dest = workdir_path.join(source_path.file_name().unwrap());
            copy(&source_path, &dest).await?;
            println!(
                "Preparing source file: {:?} -> {:?}",
                source_path.display(),
                dest.display()
            );
        }

        if source_path.is_dir() {
            let files = store::get_file_paths(&source_path, &source.ignore_paths)?;

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
                println!(
                    "Preparing source file: {:?} -> {:?}",
                    src.display(),
                    dest.display()
                );
            }
        }
    }

    // At this point, any source URI should be a local file path

    let workdir_files = store::get_file_paths(&workdir_path.to_path_buf(), &source.ignore_paths)?;

    if workdir_files.is_empty() {
        return Err(anyhow::anyhow!("No source files found"));
    }

    println!("Preparing source files: {:?}", workdir_files);

    let workdir_files_hashes = store::get_file_hashes(&workdir_files)?;
    let workdir_hash = store::get_source_hash(&workdir_files_hashes)?;

    if workdir_hash.is_empty() {
        return Err(anyhow::anyhow!("Failed to get source hash"));
    }

    println!("Source hash: {}", workdir_hash);

    if let Some(request_hash) = &source.hash {
        if &workdir_hash != request_hash {
            let message = format!("Hash mismatch: {} != {}", request_hash, workdir_hash);
            return Err(anyhow::anyhow!("{}", message));
        }
    }

    let source_tar = NamedTempFile::new()?;
    let source_tar_path = source_tar.path().canonicalize()?;

    println!("Creating source tar: {:?}", source_tar);

    store::compress_files(&workdir_path, &source_tar_path, &workdir_files)?;

    set_permissions(&source_tar, Permissions::from_mode(0o444)).await?;

    println!("Source tar: {}", source_tar_path.display());

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
            println!("Using existing source: {}", store_path_dir.display());
            return Ok(());
        }

        if store_path_tar.exists() {
            println!("Using existing tar: {}", store_path_tar.display());

            fs::create_dir_all(&store_path_dir).await?;

            store::unpack_source(&store_path_dir, &store_path_tar)?;

            println!("Unpacked source: {}", store_path_dir.display());

            return Ok(());
        }

        let mut store_tar = File::create(&store_path_tar).await?;
        match store_tar.write(&response_data.package_data).await {
            Ok(_) => {
                let metadata = fs::metadata(&store_path_tar).await?;
                let mut permissions = metadata.permissions();

                permissions.set_mode(0o444);
                fs::set_permissions(store_path_tar.clone(), permissions).await?;

                let file_name = store_path_tar.file_name().unwrap();
                println!("Stored tar: {}", file_name.to_string_lossy());
            }
            Err(e) => eprintln!("Failed source file: {}", e),
        }

        println!("Stored tar: {}", store_path_tar.display());

        fs::create_dir_all(&store_path_dir).await?;

        store::unpack_source(&store_path_dir, &store_path_tar)?;

        println!("Unpacked source: {}", store_path_dir.display());
    }

    Ok(())
}
