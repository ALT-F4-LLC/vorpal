use crate::api::package_service_client::PackageServiceClient;
use crate::api::{
    BuildRequest, PackageRequest, PackageResponse, PackageSourceKind, PrepareRequest,
};
use crate::notary;
use crate::store;
use rand::rngs::OsRng;
use rsa::pss::SigningKey;
use rsa::sha2::Sha256;
use rsa::signature::RandomizedSigner;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
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
    let source = match &request.source {
        Some(s) => s,
        None => return Err(anyhow::anyhow!("Source is required")),
    };

    if source.kind == PackageSourceKind::Unknown as i32 {
        return Err(anyhow::anyhow!("Unknown source kind"));
    }

    let work_dir = tempdir()?;
    let work_dir_path = work_dir.path().canonicalize()?;

    println!("Preparing work dir: {:?}", work_dir_path);

    if source.kind == PackageSourceKind::Git as i32 {
        // TODO: check if source exists already with hash
        // TODO: if not, download git source
        // TODO: if so, extract source to work_dir
    }

    if source.kind == PackageSourceKind::Http as i32 {
        // TODO: check if source exists already with hash
        // TODO: if not, download http source
        // TODO: if so, check mime type for compression
        // TODO: if compressed, extract source to work_dir
        // TODO: if not, move source to work_dir
    }

    if source.kind == PackageSourceKind::Local as i32 {
        let source_path = Path::new(&source.uri).canonicalize()?;

        println!("Preparing source path: {:?}", source_path);

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            println!("Preparing source kind: {:?}", source_kind);

            if source_kind.mime_type() == "application/gzip" {
                println!("Preparing packed source: {:?}", work_dir);
                store::unpack_source(&work_dir_path, &source_path)?;
            }
        }

        if source_path.is_dir() {
            let files = store::get_file_paths(&source_path, &source.ignore_paths)?;

            if files.is_empty() {
                return Err(anyhow::anyhow!("No source files found"));
            }

            for src in &files {
                if src.is_dir() {
                    let dest = work_dir_path.join(src.strip_prefix(&source_path)?);
                    fs::create_dir_all(dest).await?;
                    continue;
                }

                let dest = work_dir_path.join(src.file_name().unwrap());
                fs::copy(src, &dest).await?;
                println!(
                    "Preparing source file: {:?} -> {:?}",
                    src.display(),
                    dest.display()
                );
            }
        }
    }

    // At this point, any source URI should be a local file path

    let work_dir_files = store::get_file_paths(&work_dir_path.to_path_buf(), &vec![])?;

    if work_dir_files.is_empty() {
        return Err(anyhow::anyhow!("No source files found"));
    }

    println!("Preparing source files: {:?}", work_dir_files);

    let work_dir_files_hashes = store::get_file_hashes(&work_dir_files)?;

    let work_dir_hash = store::get_source_hash(&work_dir_files_hashes)?;

    if work_dir_hash.is_empty() {
        return Err(anyhow::anyhow!("Failed to get source hash"));
    }

    println!("Source hash: {}", work_dir_hash);

    if let Some(request_hash) = &source.hash {
        if &work_dir_hash != request_hash {
            println!("Hash mismatch: {} != {}", request_hash, work_dir_hash);
            return Err(anyhow::anyhow!("Hash mismatch"));
        }
    }

    let source_dir_name = store::get_package_dir_name(&request.name, &work_dir_hash);
    let source_dir = package_dir.join(&source_dir_name).with_extension("source");
    let source_tar = source_dir.with_extension("source.tar.gz");
    if !source_tar.exists() {
        println!("Creating source tar: {:?}", source_tar);
        store::compress_files(&work_dir_path.to_path_buf(), &source_tar, &work_dir_files)?;
        fs::set_permissions(&source_tar, Permissions::from_mode(0o444)).await?;
    }

    println!("Source tar: {}", source_tar.display());

    let private_key = notary::get_private_key().await?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let mut signing_rng = OsRng;
    let source_data = fs::read(&source_tar).await?;
    let source_signature = signing_key.sign_with_rng(&mut signing_rng, &source_data);

    println!("Source tar signature: {}", source_signature.to_string());

    let request = tonic::Request::new(PrepareRequest {
        source_data,
        source_hash: work_dir_hash.to_string(),
        source_name: request.name.to_string(),
        source_signature: source_signature.to_string(),
    });
    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;
    let response = client.prepare(request).await?;
    let response_data = response.into_inner();

    fs::remove_dir_all(&work_dir).await?;

    Ok((response_data.source_id, work_dir_hash))
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
