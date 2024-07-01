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
use crate::store::{archives, paths};
use anyhow::Result;
use std::path::Path;
use tokio::fs;
use tokio::fs::read;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Sender;
use tonic::{Request, Status};
use tracing::debug;

pub async fn send_error(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    message: String,
) -> Result<(), anyhow::Error> {
    debug!("send_error: {}", message);

    tx.send(Err(Status::internal(message.clone()))).await?;

    anyhow::bail!(message)
}

pub async fn send(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    log_output: Vec<u8>,
    package_output: Option<ConfigPackageOutput>,
) -> Result<(), anyhow::Error> {
    debug!("send: {}", String::from_utf8_lossy(&log_output));

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
    let data = read(&source_tar_path).await?;

    let signature = notary::sign(&data).await?;

    send(
        tx,
        format!("source tar signature: {}", signature).into_bytes(),
        None,
    )
    .await?;

    let mut request_chunks = vec![];
    let request_chunks_size = 8192; // default grpc limit

    for chunk in data.chunks(request_chunks_size) {
        request_chunks.push(PackagePrepareRequest {
            source_data: chunk.to_vec(),
            source_name: name.to_string(),
            source_hash: source_hash.to_string(),
            source_signature: signature.to_string(),
        });
    }

    send(
        tx,
        format!("source chunks: {}", request_chunks.len()).into_bytes(),
        None,
    )
    .await?;

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

    let package_path = paths::get_package_path(name, source_hash);

    let mut package_service = PackageServiceClient::connect(worker.to_string()).await?;

    if let Ok(res) = package_service.build(build_config).await {
        let mut build_stream = res.into_inner();

        while let Some(chunk) = build_stream.message().await? {
            if !chunk.log_output.is_empty() {
                tx.send(Ok(ConfigPackageResponse {
                    log_output: chunk.log_output,
                    package_output: None,
                }))
                .await?;
            }
        }

        let package_tar_path = paths::get_package_tar_path(name, source_hash);

        if !package_tar_path.exists() {
            let fetch_response = store_service.fetch(store_package_path.clone()).await?;
            let mut fetch_stream = fetch_response.into_inner();
            let mut fetch_stream_data = Vec::new();

            while let Some(chunk) = fetch_stream.message().await? {
                if !chunk.data.is_empty() {
                    fetch_stream_data.extend_from_slice(&chunk.data);
                }
            }

            let mut package_tar = File::create(&package_tar_path).await?;

            if let Err(e) = package_tar.write(&fetch_stream_data).await {
                send_error(tx, e.to_string()).await?
            }

            send(
                tx,
                format!("tar fetched: {}", package_tar_path.display()).into_bytes(),
                None,
            )
            .await?;

            fs::create_dir_all(package_path.clone()).await?;

            archives::unpack_tar_gz(&package_path, &package_tar_path).await?;

            tx.send(Ok(ConfigPackageResponse {
                log_output: format!("tar unpacked: {}", package_path.display()).into_bytes(),
                package_output: Some(ConfigPackageOutput {
                    hash: source_hash.to_string(),
                    name: name.to_string(),
                }),
            }))
            .await?;
        }

        // TODO: check if build failed
    }

    tx.send(Ok(ConfigPackageResponse {
        log_output: format!("output: {}", package_path.display()).into_bytes(),
        package_output: Some(ConfigPackageOutput {
            hash: source_hash.to_string(),
            name: name.to_string(),
        }),
    }))
    .await?;

    Ok(())
}

pub async fn package(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    request: Request<ConfigPackageRequest>,
    workers: Vec<ConfigWorker>,
) -> Result<(), anyhow::Error> {
    let config = request.into_inner();

    send(tx, config.name.clone().into_bytes(), None).await?;

    let config_source = match config.source {
        None => anyhow::bail!("source config is required"),
        Some(source) => source,
    };

    let mut config_source_hash = config_source.hash.clone().unwrap_or_default();

    match config_source.kind() {
        ConfigPackageSourceKind::UnknownSource => {
            send_error(tx, "source kind is unknown".to_string()).await?
        }
        ConfigPackageSourceKind::Git => {}
        ConfigPackageSourceKind::Http => {}
        ConfigPackageSourceKind::Local => {
            if config_source_hash.is_empty() {
                let path = Path::new(&config_source.uri).canonicalize()?;
                let (hash, _) = source::validate(tx, &path, &config_source).await?;
                send(tx, format!("source hash: {}", hash).into_bytes(), None).await?;
                config_source_hash = hash;
            }
        }
    };

    if config_source_hash.is_empty() {
        send_error(tx, "source hash is required".to_string()).await?
    }

    // check package_path exists in agent (local) cache

    let package_path = paths::get_package_path(&config.name, &config_source_hash);

    if package_path.exists() {
        send(
            tx,
            format!("cache: {}", package_path.display()).into_bytes(),
            Some(ConfigPackageOutput {
                hash: config_source_hash.clone(),
                name: config.name.clone(),
            }),
        )
        .await?;

        return Ok(());
    }

    // check package tar exists in agent (local) cache

    let package_tar_path = paths::get_package_tar_path(&config.name, &config_source_hash);

    if !package_path.exists() && package_tar_path.exists() {
        send(
            tx,
            format!("tar cache: {}", package_tar_path.display()).into_bytes(),
            None,
        )
        .await?;

        fs::create_dir_all(&package_path).await?;

        archives::unpack_tar_gz(&package_path, &package_tar_path).await?;

        send(
            tx,
            format!("tar cache unpacked: {}", package_path.display()).into_bytes(),
            Some(ConfigPackageOutput {
                hash: config_source_hash.clone(),
                name: config.name.clone(),
            }),
        )
        .await?;

        return Ok(());
    }

    // check if package exists in worker (remote) cache

    let config_build = match config.build {
        None => return send_error(tx, "build config is required".to_string()).await,
        Some(build) => build,
    };

    let config_build_system = ConfigPackageBuildSystem::try_from(config_build.system)
        .unwrap_or(ConfigPackageBuildSystem::UnknownSystem);

    if config_build_system == ConfigPackageBuildSystem::UnknownSystem {
        send_error(tx, "build system is unknown".to_string()).await?
    }

    send(
        tx,
        format!("build system: {:?}", config_build.system()).into_bytes(),
        None,
    )
    .await?;

    for worker in workers {
        if worker.system != config_build_system {
            send(
                tx,
                format!(
                    "build system mismatch: {:?} != {:?}",
                    worker.system, config_build_system
                )
                .into_bytes(),
                None,
            )
            .await?;

            continue;
        }

        let mut store_service = StoreServiceClient::connect(worker.uri.clone()).await?;

        let store_package_path = StorePath {
            kind: StorePathKind::Package as i32,
            name: config.name.clone(),
            hash: config_source_hash.clone(),
        };

        if let Ok(res) = store_service.path(store_package_path.clone()).await {
            let store_path = res.into_inner();

            send(
                tx,
                format!("remote cache: {}", store_path.uri).into_bytes(),
                None,
            )
            .await?;

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

                let mut package_tar = File::create(&package_tar_path).await?;

                if let Err(e) = package_tar.write(&stream_data).await {
                    send_error(tx, e.to_string()).await?
                }

                send(
                    tx,
                    format!("tar fetched: {}", package_tar_path.display()).into_bytes(),
                    None,
                )
                .await?;

                fs::create_dir_all(&package_path).await?;

                match archives::unpack_tar_gz(&package_path, &package_tar_path).await {
                    Ok(_) => {}
                    Err(_) => send_error(tx, "failed to unpack tar".to_string()).await?,
                }

                send(
                    tx,
                    format!("tar unpacked: {}", package_path.display()).into_bytes(),
                    Some(ConfigPackageOutput {
                        hash: config_source_hash.clone(),
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
            hash: config_source_hash.clone(),
        };

        if let Ok(res) = store_service.path(store_package_source_path.clone()).await {
            let store_path = res.into_inner();

            send(
                tx,
                format!("source cache: {}", store_path.uri).into_bytes(),
                None,
            )
            .await?;

            build(
                tx,
                &config_build,
                &config.name,
                &config_source_hash,
                &store_package_path,
                &mut store_service,
                &worker.uri,
            )
            .await?;

            break;
        }

        let package_source_tar_path =
            paths::get_package_source_tar_path(&config.name, &config_source_hash);

        if package_source_tar_path.exists() {
            send(
                tx,
                format!("source tar cache: {}", package_source_tar_path.display()).into_bytes(),
                None,
            )
            .await?;

            prepare(
                tx,
                &config.name,
                &config_source_hash,
                &package_source_tar_path,
                &worker.uri,
            )
            .await?;

            build(
                tx,
                &config_build,
                &config.name,
                &config_source_hash,
                &store_package_path,
                &mut store_service,
                &worker.uri,
            )
            .await?;

            break;
        }

        let source_hash = source::prepare(
            tx,
            &config.name,
            &config_source,
            &config_source_hash,
            &package_source_tar_path,
        )
        .await?;

        send(
            tx,
            format!("source tar: {}", package_source_tar_path.display()).into_bytes(),
            None,
        )
        .await?;

        // check if package source exists in worker cache (same as agent)

        if let Ok(res) = store_service.path(store_package_source_path).await {
            let store_path = res.into_inner();

            send(
                tx,
                format!("source cache: {}", store_path.uri).into_bytes(),
                None,
            )
            .await?;

            build(
                tx,
                &config_build,
                &config.name,
                &config_source_hash,
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
            &package_source_tar_path,
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
