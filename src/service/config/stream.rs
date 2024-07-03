use crate::api::package_service_client::PackageServiceClient;
use crate::api::store_service_client::StoreServiceClient;
use crate::api::{
    ConfigPackageBuild, ConfigPackageBuildSystem, ConfigPackageOutput, ConfigPackageResponse,
    ConfigPackageSourceKind, PackageBuildRequest, PackagePrepareRequest, PrepareBuildPackage,
    StorePath, StorePathKind,
};
use crate::notary;
use crate::service::config::source;
use crate::service::config::{ConfigPackageRequest, ConfigWorker};
use crate::store::archives::unpack_zstd;
use crate::store::paths::{
    get_package_archive_path, get_package_path, get_package_source_archive_path,
};
use anyhow::Result;
use std::path::Path;
use tokio::fs::{create_dir_all, read, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Sender;
use tonic::{Request, Status};
use tracing::debug;

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
    log_output: String,
    package_output: Option<ConfigPackageOutput>,
) -> Result<(), anyhow::Error> {
    debug!("send: {:?}", log_output);

    tx.send(Ok(ConfigPackageResponse {
        log_output,
        package_output,
    }))
    .await?;

    Ok(())
}

pub async fn prepare(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    name: &str,
    source_hash: &str,
    source_tar_path: &Path,
    worker: &String,
) -> Result<(), anyhow::Error> {
    let source_data = read(&source_tar_path).await?;

    let source_signature = notary::sign(&source_data).await?;

    let message = format!("package source tar: {}", source_tar_path.display());

    send(tx, message.into(), None).await?;

    let mut request_chunks = vec![];

    let request_chunks_size = 8192; // default grpc limit

    for chunk in source_data.chunks(request_chunks_size) {
        request_chunks.push(PackagePrepareRequest {
            source_data: chunk.to_vec(),
            source_name: name.to_string(),
            source_hash: source_hash.to_string(),
            source_signature: source_signature.to_string(),
        });
    }

    let message = format!("package source chunks: {}", request_chunks.len());

    send(tx, message.into(), None).await?;

    let mut client = PackageServiceClient::connect(worker.to_string()).await?;

    let response = client.prepare(tokio_stream::iter(request_chunks)).await?;

    let mut stream = response.into_inner();

    while let Some(res) = stream.message().await? {
        if !res.log_output.is_empty() {
            send(tx, res.log_output, None).await?;
        }
    }

    Ok(())
}

pub async fn build(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    build: &ConfigPackageBuild,
    name: &str,
    source_hash: &str,
    store_package_path: &StorePath,
    store_service: &mut StoreServiceClient<tonic::transport::channel::Channel>,
    worker: &String,
) -> Result<(), anyhow::Error> {
    let mut build_packages = vec![];

    for output in build.packages.clone().into_iter() {
        build_packages.push(PrepareBuildPackage {
            hash: output.hash,
            name: output.name,
        });
    }

    let build_config = PackageBuildRequest {
        build_environment: build.environment.clone(),
        build_sandbox: build.sandbox,
        build_packages,
        build_script: build.script.to_string(),
        build_system: build.system,
        source_name: name.to_string(),
        source_hash: source_hash.to_string(),
    };

    let package_path = get_package_path(name, source_hash);

    let mut package_service = PackageServiceClient::connect(worker.to_string()).await?;

    if let Ok(res) = package_service.build(build_config).await {
        let mut build_stream = res.into_inner();

        while let Some(chunk) = build_stream.message().await? {
            if !chunk.log_output.is_empty() {
                send(tx, chunk.log_output, None).await?;
            }
        }

        let package_archive_path = get_package_archive_path(name, source_hash);

        // check if package tar exists in agent (local) cache

        if !package_archive_path.exists() {
            let fetch_response = store_service.fetch(store_package_path.clone()).await?;
            let mut fetch_stream = fetch_response.into_inner();
            let mut fetch_stream_data = Vec::new();

            while let Some(chunk) = fetch_stream.message().await? {
                if !chunk.data.is_empty() {
                    fetch_stream_data.extend_from_slice(&chunk.data);
                }
            }

            let mut package_tar = File::create(&package_archive_path).await?;

            package_tar.write_all(&fetch_stream_data).await?;

            let message = format!(
                "package archive fetched: {}",
                package_archive_path.file_name().unwrap().to_str().unwrap()
            );

            send(tx, message.into(), None).await?;

            create_dir_all(package_path.clone()).await?;

            unpack_zstd(&package_path, &package_archive_path).await?;
        }
    }

    send(
        tx,
        format!(
            "package output: {}",
            package_path.file_name().unwrap().to_str().unwrap()
        ),
        Some(ConfigPackageOutput {
            hash: source_hash.to_string(),
            name: name.to_string(),
        }),
    )
    .await?;

    Ok(())
}

pub async fn package(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    request: Request<ConfigPackageRequest>,
    workers: Vec<ConfigWorker>,
) -> Result<(), anyhow::Error> {
    let config = request.into_inner();

    let source = match config.source {
        None => return send_error(tx, "source config is required".to_string()).await,
        Some(source) => source,
    };

    let mut source_hash = source.hash.clone().unwrap_or_default();

    match source.kind() {
        ConfigPackageSourceKind::UnknownSource => {
            send_error(tx, "package source kind is unknown".to_string()).await?
        }
        ConfigPackageSourceKind::Git => {}
        ConfigPackageSourceKind::Http => {}
        ConfigPackageSourceKind::Local => {
            if source_hash.is_empty() {
                let path = Path::new(&source.uri).canonicalize()?;
                let (hash, _) = source::validate(tx, &path, &source).await?;
                let message = format!("package source local hash: {}", hash);
                send(tx, message.into(), None).await?;
                source_hash = hash;
            }
        }
    };

    if source_hash.is_empty() {
        send_error(tx, "package source hash is required".to_string()).await?
    }

    // check package_path exists in agent (local) cache

    let package_path = get_package_path(&config.name, &source_hash);

    if package_path.exists() {
        send(
            tx,
            format!(
                "package: {}",
                package_path.file_name().unwrap().to_str().unwrap()
            )
            .into(),
            Some(ConfigPackageOutput {
                hash: source_hash.clone(),
                name: config.name.clone(),
            }),
        )
        .await?;

        return Ok(());
    }

    // check package tar exists in agent (local) cache

    let package_archive_path = get_package_archive_path(&config.name, &source_hash);

    if !package_path.exists() && package_archive_path.exists() {
        send(
            tx,
            format!("package archive exists: {}", package_archive_path.display()),
            None,
        )
        .await?;

        create_dir_all(&package_path).await?;

        unpack_zstd(&package_path, &package_archive_path).await?;

        send(
            tx,
            format!("package archive unpacked: {}", package_path.display()),
            Some(ConfigPackageOutput {
                hash: source_hash.clone(),
                name: config.name.clone(),
            }),
        )
        .await?;

        return Ok(());
    }

    let config_build = match config.build {
        None => return send_error(tx, "package build config is required".to_string()).await,
        Some(build) => build,
    };

    let config_build_system = ConfigPackageBuildSystem::try_from(config_build.system)
        .unwrap_or(ConfigPackageBuildSystem::UnknownSystem);

    if config_build_system == ConfigPackageBuildSystem::UnknownSystem {
        send_error(tx, "build system is unknown".to_string()).await?
    }

    let message = format!("package build system: {:?}", config_build.system());

    send(tx, message.into(), None).await?;

    for worker in workers {
        if worker.system != config_build_system {
            let message = format!(
                "package build system mismatch: {:?} != {:?}",
                config_build_system, worker.system
            );

            send(tx, message, None).await?;

            continue;
        }

        // check if package tar exists in worker caches

        let mut store_service = StoreServiceClient::connect(worker.uri.clone()).await?;

        let store_package_path = StorePath {
            kind: StorePathKind::Package as i32,
            name: config.name.clone(),
            hash: source_hash.clone(),
        };

        if let Ok(res) = store_service.path(store_package_path.clone()).await {
            let store_path = res.into_inner();

            let message = format!("package archive cache remote: {}", store_path.uri);

            send(tx, message.into(), None).await?;

            if let Ok(res) = store_service.fetch(store_package_path.clone()).await {
                let mut stream = res.into_inner();
                let mut stream_data = Vec::new();

                while let Some(chunk) = stream.message().await? {
                    if !chunk.data.is_empty() {
                        stream_data.extend_from_slice(&chunk.data);
                    }
                }

                if stream_data.is_empty() {
                    send_error(tx, "no data fetched".to_string()).await?
                }

                let stream_data_size = stream_data.len();

                let message = format!("package archive cache remote size: {}", stream_data_size);

                send(tx, message.into(), None).await?;

                let mut package_archive = File::create(&package_archive_path).await?;

                package_archive.write_all(&stream_data).await?;

                let message = format!(
                    "package tar cache remote fetched: {}",
                    package_archive_path.display()
                );

                send(tx, message.into(), None).await?;

                create_dir_all(&package_path).await?;

                match unpack_zstd(&package_path, &package_archive_path).await {
                    Ok(_) => {}
                    Err(_) => send_error(tx, "failed to unpack tar".to_string()).await?,
                }

                send(
                    tx,
                    format!(
                        "package tar cache remote unpacked: {}",
                        package_path.display()
                    ),
                    Some(ConfigPackageOutput {
                        hash: source_hash.clone(),
                        name: config.name.clone(),
                    }),
                )
                .await?;

                break;
            }
        }

        // check if package source exists in worker cache

        let store_package_source_path = StorePath {
            kind: StorePathKind::Source as i32,
            name: config.name.clone(),
            hash: source_hash.clone(),
        };

        if let Ok(res) = store_service.path(store_package_source_path.clone()).await {
            let store_path = res.into_inner();

            let store_path_segments: Vec<&str> = store_path
                .uri
                .split('/')
                .filter(|segment| !segment.is_empty())
                .collect();

            let store_path_file_name = store_path_segments.last().unwrap_or(&"");

            let message = format!(
                "package source archive cache remote: {}",
                store_path_file_name
            );

            send(tx, message.into(), None).await?;

            build(
                tx,
                &config_build,
                &config.name,
                &source_hash,
                &store_package_path,
                &mut store_service,
                &worker.uri,
            )
            .await?;

            break;
        }

        // check if package source tar exists in agent (local) cache

        let package_source_archive_path =
            get_package_source_archive_path(&config.name, &source_hash);

        if package_source_archive_path.exists() {
            let message = format!(
                "package source tar cache: {}",
                package_source_archive_path.display()
            );

            send(tx, message.into(), None).await?;

            prepare(
                tx,
                &config.name,
                &source_hash,
                &package_source_archive_path,
                &worker.uri,
            )
            .await?;

            build(
                tx,
                &config_build,
                &config.name,
                &source_hash,
                &store_package_path,
                &mut store_service,
                &worker.uri,
            )
            .await?;

            break;
        }

        let source_hash = source::prepare(tx, &source, &package_source_archive_path).await?;

        // check if package source exists in worker cache (same as agent)

        if let Ok(res) = store_service.path(store_package_source_path).await {
            let store_path = res.into_inner();

            send(
                tx,
                format!("package source tar cache remote: {}", store_path.uri),
                None,
            )
            .await?;

            build(
                tx,
                &config_build,
                &config.name,
                &source_hash,
                &store_package_path,
                &mut store_service,
                &worker.uri,
            )
            .await?;

            break;
        }

        prepare(
            tx,
            &config.name,
            &source_hash,
            &package_source_archive_path,
            &worker.uri,
        )
        .await?;

        build(
            tx,
            &config_build,
            &config.name,
            &source_hash,
            &store_package_path,
            &mut store_service,
            &worker.uri,
        )
        .await?;

        break;
    }

    Ok(())
}
