use std::path::{Path, PathBuf};
use tokio::fs::{copy, create_dir_all, read, remove_dir_all, File};
use tokio::io::AsyncWriteExt;
use tracing::{error, info};
use vorpal_schema::api::package::package_service_client::PackageServiceClient;
use vorpal_schema::api::package::{BuildRequest, PackageSystem};
use vorpal_schema::api::store::store_service_client::StoreServiceClient;
use vorpal_schema::api::store::{StoreKind, StoreRequest};
use vorpal_schema::Package;
use vorpal_store::temps;
use vorpal_store::{
    archives::{compress_zstd, unpack_gzip, unpack_zstd},
    hashes::{get_hashes_digest, hash_files},
    paths::{
        get_file_paths, get_package_archive_path, get_package_path, get_private_key_path,
        get_source_archive_path,
    },
};

mod source;
mod stream;

#[derive(Clone, Debug)]
pub struct Worker {
    pub system: PackageSystem,
    pub uri: String,
}

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

pub async fn archive_source(
    package_hash: &String,
    package_name: &String,
    package_source_ignores: Vec<&String>,
    package_source_path: PathBuf,
) -> Result<(), anyhow::Error> {
    let sandbox_path = temps::create_dir().await?;

    if let Ok(Some(source_kind)) = infer::get_from_path(&package_source_path) {
        if source_kind.mime_type() == "application/gzip" {
            unpack_gzip(&sandbox_path, &package_source_path).await?;
        }
    } else if package_source_path.is_file() {
        copy(
            &package_source_path,
            sandbox_path.join(package_source_path.file_name().unwrap()),
        )
        .await?;
    } else if package_source_path.is_dir() {
        let source_paths = get_file_paths(&package_source_path, package_source_ignores.clone())?;

        if source_paths.is_empty() {
            remove_dir_all(&sandbox_path).await?;

            anyhow::bail!("Package source files not found");
        }

        for path in &source_paths {
            if path.is_dir() {
                let dest = sandbox_path.join(path.strip_prefix(&package_source_path)?);

                create_dir_all(dest).await?;

                continue;
            }

            let dest = sandbox_path.join(path.strip_prefix(&package_source_path)?);

            copy(path, &dest).await?;

            info!(
                "Package source file: {:?} -> {:?}",
                path.display(),
                dest.display()
            );
        }
    } else {
        remove_dir_all(&sandbox_path).await?;

        anyhow::bail!("source path is not a file or directory");
    }

    // At this point, any source URI should be a local file path

    let sandbox_path_files = get_file_paths(&sandbox_path, package_source_ignores)?;

    if sandbox_path_files.is_empty() {
        remove_dir_all(&sandbox_path).await?;

        anyhow::bail!("no source files found");
    }

    let source_archive_path = get_source_archive_path(package_name, package_hash);

    compress_zstd(&sandbox_path, &sandbox_path_files, &source_archive_path).await?;

    remove_dir_all(&sandbox_path).await?;

    Ok(())
}

pub async fn build(
    package: &Package,
    config_hash: &String,
    // packages: &Vec<Package>,
    target: PackageSystem,
    workers: &Vec<Worker>,
) -> anyhow::Result<()> {
    let mut package_hash = config_hash.clone();

    let mut source_hash = String::new();

    if let Some(source) = &package.source {
        let (hash, _) = hash_files(Path::new(source), &package.source_ignores).await?;
        source_hash = hash;
    }

    if !source_hash.is_empty() {
        package_hash = get_hashes_digest(vec![package_hash, source_hash.clone()])?;
    }

    info!("Package hash: {}", package_hash);

    // LOCALLY: Check package exists

    let package_path = get_package_path(&package.name, &package_hash);

    if package_path.exists() {
        info!("Package exists: {:?}", package_path);

        return Ok(());
    }

    // LOCALLY: Check package archive exists

    let package_archive_path = get_package_archive_path(&package.name, &package_hash);

    if package_archive_path.exists() {
        info!("Package archive exists: {:?}", package_archive_path);

        create_dir_all(&package_path).await?;

        unpack_zstd(&package_path, &package_archive_path).await?;

        info!("Package archive unpacked: {:?}", package_path);

        return Ok(());
    }

    // LOCALLY: No cache exists

    for worker in workers {
        if worker.system != target {
            error!("Worker system mismatch: {:?}", worker.system);

            continue;
        }

        // REMOTE: Check package exists

        let mut worker_store = StoreServiceClient::connect(worker.uri.clone()).await?;

        let worker_package = StoreRequest {
            kind: StoreKind::Package as i32,
            name: package.name.clone(),
            hash: package_hash.clone(),
        };

        if let Ok(_) = worker_store.exists(worker_package.clone()).await {
            info!("Package cache remote exists");

            let worker_store_package = StoreRequest {
                kind: StoreKind::Package as i32,
                name: package.name.clone(),
                hash: package_hash.clone(),
            };

            let mut stream = worker_store
                .pull(worker_store_package.clone())
                .await?
                .into_inner();

            let mut stream_data = Vec::new();

            while let Some(chunk) = stream.message().await? {
                if !chunk.data.is_empty() {
                    stream_data.extend_from_slice(&chunk.data);
                }
            }

            if stream_data.is_empty() {
                error!("Package archive cache remote empty");
            }

            let stream_data_size = stream_data.len();

            info!(
                "Package archive cache remote fetched: {} bytes",
                stream_data_size
            );

            let mut package_archive = File::create(&package_archive_path).await?;

            package_archive.write_all(&stream_data).await?;

            create_dir_all(&package_path).await?;

            unpack_zstd(&package_path, &package_archive_path).await?;

            info!("Package archive unpacked: {:?}", package_path);

            return Ok(());
        }

        // Start building package

        let mut request_stream: Vec<BuildRequest> = vec![];

        if let Some(source) = &package.source {
            let worker_source = StoreRequest {
                kind: StoreKind::Source as i32,
                name: package.name.clone(),
                hash: package_hash.clone(),
            };

            // REMOTE: Check package source exists for initial archive

            match worker_store.exists(worker_source.clone()).await {
                Ok(_) => info!("Package source exists on worker"),
                Err(status) => {
                    if status.code() == tonic::Code::NotFound {
                        info!("Package source remote missing");

                        let source_path = Path::new(source).to_path_buf();

                        if !source_path.exists() {
                            anyhow::bail!("Package source path not found: {:?}", source_path);
                        }

                        let git = "git".to_string();
                        let gitignore = ".gitignore".to_string();
                        let direnv = ".direnv".to_string();
                        let mut ignores: Vec<&String> = vec![&git, &gitignore, &direnv];

                        for ignore in &package.source_ignores {
                            ignores.push(ignore);
                        }

                        archive_source(&package_hash, &package.name, ignores, source_path).await?;
                    }
                }
            }

            // REMOTE: Check package source exists for cli and worker on same host

            match worker_store.exists(worker_source.clone()).await {
                Ok(_) => info!("Package source exists on worker"),
                Err(status) => {
                    if status.code() == tonic::Code::NotFound {
                        let source_archive_path =
                            get_source_archive_path(&package.name, &package_hash);

                        let source_data = read(&source_archive_path).await?;

                        let private_key_path = get_private_key_path();

                        if !private_key_path.exists() {
                            anyhow::bail!("Private key not found: {:?}", private_key_path);
                        }

                        let source_signature =
                            vorpal_notary::sign(private_key_path, &source_data).await?;

                        for chunk in source_data.chunks(DEFAULT_CHUNKS_SIZE) {
                            request_stream.push(BuildRequest {
                                package_environment: package.environment.clone(),
                                package_hash: source_hash.to_string(),
                                package_image: None,
                                package_input: package.input.clone(),
                                package_name: package.name.clone(),
                                package_packages: vec![],
                                package_sandbox: false,
                                package_script: package.script.clone(),
                                package_source_data: Some(chunk.to_vec()),
                                package_source_data_signature: Some(source_signature.to_string()),
                                package_systems: package
                                    .systems
                                    .iter()
                                    .map(|s| s.clone() as i32)
                                    .collect(),
                                package_target: target as i32,
                            });
                        }
                    }
                }
            }
        }

        request_stream.push(BuildRequest {
            package_environment: package.environment.clone(),
            package_hash: package_hash.to_string(),
            package_image: None,
            package_input: package.input.clone(),
            package_name: package.name.clone(),
            package_packages: vec![],
            package_sandbox: false,
            package_script: package.script.clone(),
            package_source_data: None,
            package_source_data_signature: None,
            package_systems: package.systems.iter().map(|s| s.clone() as i32).collect(),
            package_target: target as i32,
        });

        let mut service = PackageServiceClient::connect(worker.uri.clone()).await?;

        let response = service.build(tokio_stream::iter(request_stream)).await?;

        let mut stream = response.into_inner();

        while let Some(res) = stream.message().await? {
            if !res.package_log.is_empty() {
                info!("Package log: {}", res.package_log);
            }
        }
    }

    Ok(())
}
