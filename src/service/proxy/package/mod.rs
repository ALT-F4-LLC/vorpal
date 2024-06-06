use crate::api::package_service_client::PackageServiceClient;
use crate::api::{BuildRequest, PackageRequest, PackageResponse, PrepareRequest};
use crate::notary;
use crate::store;
use rand::rngs::OsRng;
use rsa::pss::SigningKey;
use rsa::sha2::Sha256;
use rsa::signature::RandomizedSigner;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tonic::{Request, Response, Status};

pub async fn run(request: Request<PackageRequest>) -> Result<Response<PackageResponse>, Status> {
    let package_dir = store::get_package_path();
    if !package_dir.exists() {
        return Err(Status::internal("Package directory does not exist"));
    }

    let r = request.into_inner();

    println!("Preparing: {}", r.name);

    let (source_id, source_hash) = match prepare(package_dir, &r).await {
        Ok((id, hash)) => (id, hash),
        Err(e) => {
            eprintln!("Failed to prepare: {:?}", e);
            return Err(Status::internal("Failed to prepare"));
        }
    };

    println!("Building: {}-{}", r.name, source_hash);

    match build(source_id, &source_hash, &r).await {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Failed to build: {:?}", e);
            return Err(Status::internal("Failed to build"));
        }
    }

    let response = PackageResponse {
        source_id: source_id.to_string(),
        source_hash,
    };

    Ok(Response::new(response))
}

async fn prepare(
    package_dir: PathBuf,
    request: &PackageRequest,
) -> Result<(i32, String), anyhow::Error> {
    let source = Path::new(&request.source).canonicalize()?;
    let source_ignore_paths = request
        .ignore_paths
        .iter()
        .map(|i| Path::new(i).to_path_buf())
        .collect::<Vec<PathBuf>>();
    let source_files = store::get_file_paths(&source, &source_ignore_paths)?;
    let source_files_hashes = store::get_file_hashes(&source_files)?;
    let source_hash = store::get_source_hash(&source_files_hashes)?;
    let source_dir_name = store::get_package_dir_name(&request.name, &source_hash);
    let source_dir = package_dir.join(&source_dir_name).with_extension("source");
    let source_tar = source_dir.with_extension("source.tar.gz");
    if !source_tar.exists() {
        println!("Creating source tar: {:?}", source_tar);
        store::compress_files(&source, &source_tar, &source_files)?;
        fs::set_permissions(&source_tar, Permissions::from_mode(0o444)).await?;
    }

    println!("Source file: {}", source_tar.display());

    let private_key = notary::get_private_key().await?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let mut signing_rng = OsRng;
    let source_data = fs::read(&source_tar).await?;
    let source_signature = signing_key.sign_with_rng(&mut signing_rng, &source_data);

    println!("Source signature: {}", source_signature.to_string());

    let request = tonic::Request::new(PrepareRequest {
        source_data,
        source_hash: source_hash.to_string(),
        source_name: request.name.to_string(),
        source_signature: source_signature.to_string(),
    });
    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;
    let response = client.prepare(request).await?;
    let response_data = response.into_inner();

    Ok((response_data.source_id, source_hash))
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
        let store_path = store::get_store_path();
        let store_path_dir_name = store::get_package_dir_name(&request.name, package_hash);
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
        match store_tar.write_all(&response_data.package_data).await {
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
