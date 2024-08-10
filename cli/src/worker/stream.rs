// use anyhow::Result;
// use std::path::Path;
// use tokio::fs::{create_dir_all, read, File};
// use tokio::io::AsyncWriteExt;
// use tokio::sync::mpsc::Sender;
// use tonic::{Request, Status};
// use vorpal_schema::api::package_service_client::PackageServiceClient;
// use vorpal_schema::api::store_service_client::StoreServiceClient;
// use vorpal_schema::api::{PackageBuildRequest, PrepareBuildPackage, StorePath, StorePathKind};
// use vorpal_store::archives::unpack_zstd;
// use vorpal_store::paths::{get_package_archive_path, get_package_path, get_source_archive_path};
//
// const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit
//
// pub async fn prepare(
//     tx: &Sender<Result<ConfigPackageResponse, Status>>,
//     name: &str,
//     source_hash: &str,
//     source_tar_path: &Path,
//     worker: &String,
// ) -> Result<(), anyhow::Error> {
//     let source_data = read(&source_tar_path).await?;
//
//     let source_signature = notary::sign(&source_data).await?;
//
//     let mut source_chunks = vec![];
//
//     for chunk in source_data.chunks(DEFAULT_CHUNKS_SIZE) {
//         source_chunks.push(PackagePrepareRequest {
//             source_data: chunk.to_vec(),
//             source_hash: source_hash.to_string(),
//             source_name: name.to_string(),
//             source_signature: source_signature.to_string(),
//         });
//     }
//
//     let message = format!("source chunks: {}", source_chunks.len());
//
//     send(tx, message, None).await?;
//
//     let mut client = PackageServiceClient::connect(worker.to_string()).await?;
//
//     let response = client.prepare(tokio_stream::iter(source_chunks)).await?;
//
//     let mut stream = response.into_inner();
//
//     while let Some(res) = stream.message().await? {
//         if !res.log_output.is_empty() {
//             send(tx, res.log_output, None).await?;
//         }
//     }
//
//     Ok(())
// }
//
// pub async fn build(
//     tx: &Sender<Result<ConfigPackageResponse, Status>>,
//     build: &ConfigPackageBuild,
//     name: &str,
//     source_hash: &str,
//     worker: &String,
// ) -> Result<(), anyhow::Error> {
//     let mut build_packages = vec![];
//
//     for output in build.packages.clone().into_iter() {
//         build_packages.push(PrepareBuildPackage {
//             hash: output.hash,
//             name: output.name,
//         });
//     }
//
//     let build_config = PackageBuildRequest {
//         build_environment: build.environment.clone(),
//         build_image: build.image.clone(),
//         build_packages,
//         build_script: build.script.to_string(),
//         build_system: build.system,
//         source_hash: source_hash.to_string(),
//         source_name: name.to_string(),
//     };
//
//     let mut package_service = PackageServiceClient::connect(worker.to_string()).await?;
//
//     if let Ok(res) = package_service.build(build_config).await {
//         let mut build_stream = res.into_inner();
//
//         while let Some(chunk) = build_stream.message().await? {
//             if !chunk.log_output.is_empty() {
//                 send(tx, chunk.log_output, None).await?;
//             }
//         }
//
//         let package_path = get_package_path(name, source_hash);
//         let package_archive_path = get_package_archive_path(name, source_hash);
//
//         // check if package tar exists in agent (local) cache
//
//         let mut store_service = StoreServiceClient::connect(worker.to_string()).await?;
//
//         let store_package = StorePath {
//             kind: StorePathKind::Package as i32,
//             name: name.to_string(),
//             hash: source_hash.to_string(),
//         };
//
//         if !package_archive_path.exists() {
//             let fetch_response = store_service.fetch(store_package.clone()).await?;
//             let mut fetch_stream = fetch_response.into_inner();
//             let mut fetch_stream_data = Vec::new();
//
//             while let Some(chunk) = fetch_stream.message().await? {
//                 if !chunk.data.is_empty() {
//                     fetch_stream_data.extend_from_slice(&chunk.data);
//                 }
//             }
//
//             let mut package_tar = File::create(&package_archive_path).await?;
//
//             package_tar.write_all(&fetch_stream_data).await?;
//
//             let message = format!(
//                 "package fetched: {}",
//                 package_archive_path.file_name().unwrap().to_str().unwrap()
//             );
//
//             send(tx, message, None).await?;
//
//             create_dir_all(package_path.clone()).await?;
//
//             unpack_zstd(&package_path, &package_archive_path).await?;
//         }
//
//         send(
//             tx,
//             format!(
//                 "package: {}",
//                 package_path.file_name().unwrap().to_str().unwrap()
//             ),
//             Some(ConfigPackageOutput {
//                 hash: source_hash.to_string(),
//                 name: name.to_string(),
//             }),
//         )
//         .await?;
//     }
//
//     Ok(())
// }
//
// pub async fn package(
//     request: Request<ConfigPackageRequest>,
//     workers: Vec<Worker>,
// ) -> Result<(), anyhow::Error> {
//     let config = request.into_inner();
//
//     let config_source = match config.source {
//         None => return send_error(tx, "source config is required".to_string()).await,
//         Some(source) => source,
//     };
//
//     let mut config_source_hash = config_source.hash.clone().unwrap_or_default();
//
//     match config_source.kind() {
//         ConfigPackageSourceKind::UnknownSource => {
//             send_error(tx, "source kind is unknown".to_string()).await?
//         }
//         ConfigPackageSourceKind::Git => {}
//         ConfigPackageSourceKind::Http => {}
//         ConfigPackageSourceKind::Local => {
//             if config_source_hash.is_empty() {
//                 let path = Path::new(&config_source.uri).canonicalize()?;
//                 let (hash, _) = source::validate(tx, &path, &config_source).await?;
//                 config_source_hash = hash;
//             }
//         }
//     };
//
//     if config_source_hash.is_empty() {
//         send_error(tx, "source hash is required".to_string()).await?
//     }
//
//     // check package output exists in local cache
//
//     let package_path = get_package_path(&config.name, &config_source_hash);
//
//     if package_path.exists() {
//         send(
//             tx,
//             format!(
//                 "package: {}",
//                 package_path.file_name().unwrap().to_str().unwrap()
//             ),
//             Some(ConfigPackageOutput {
//                 hash: config_source_hash.clone(),
//                 name: config.name.clone(),
//             }),
//         )
//         .await?;
//
//         return Ok(());
//     }
//
//     // check package tar exists in agent (local) cache
//
//     let package_store_path = get_package_archive_path(&config.name, &config_source_hash);
//
//     if package_store_path.exists() {
//         send(
//             tx,
//             format!("package store exists: {}", package_store_path.display()),
//             None,
//         )
//         .await?;
//
//         create_dir_all(&package_path).await?;
//
//         unpack_zstd(&package_path, &package_store_path).await?;
//
//         send(
//             tx,
//             format!("package store unpacked: {}", package_path.display()),
//             Some(ConfigPackageOutput {
//                 hash: config_source_hash.clone(),
//                 name: config.name.clone(),
//             }),
//         )
//         .await?;
//
//         return Ok(());
//     }
//
//     // At this point we have no cache locally
//
//     let config_build = match config.build {
//         None => return send_error(tx, "build config is required".to_string()).await,
//         Some(build) => build,
//     };
//
//     let config_build_system = ConfigPackageBuildSystem::try_from(config_build.system)
//         .unwrap_or(ConfigPackageBuildSystem::UnknownSystem);
//
//     if config_build_system == ConfigPackageBuildSystem::UnknownSystem {
//         send_error(tx, "build config system is unknown".to_string()).await?
//     }
//
//     for worker in workers {
//         if worker.system != config_build_system {
//             let message = format!(
//                 "build config system mismatch: {:?} != {:?}",
//                 config_build_system, worker.system
//             );
//
//             send(tx, message, None).await?;
//
//             continue;
//         }
//
//         // check if package archive exists on worker
//
//         let mut worker_store = StoreServiceClient::connect(worker.uri.clone()).await?;
//
//         let worker_store_package = StorePath {
//             kind: StorePathKind::Package as i32,
//             name: config.name.clone(),
//             hash: config_source_hash.clone(),
//         };
//
//         if let Ok(res) = worker_store.path(worker_store_package.clone()).await {
//             let store_path = res.into_inner();
//
//             let message = format!("package archive cache remote: {}", store_path.uri);
//
//             send(tx, message, None).await?;
//
//             if let Ok(res) = worker_store.fetch(worker_store_package.clone()).await {
//                 let mut stream = res.into_inner();
//                 let mut stream_data = Vec::new();
//
//                 while let Some(chunk) = stream.message().await? {
//                     if !chunk.data.is_empty() {
//                         stream_data.extend_from_slice(&chunk.data);
//                     }
//                 }
//
//                 if stream_data.is_empty() {
//                     send_error(tx, "no data fetched".to_string()).await?
//                 }
//
//                 let stream_data_size = stream_data.len();
//
//                 let message = format!("package archive cache remote size: {}", stream_data_size);
//
//                 send(tx, message, None).await?;
//
//                 let package_archive_path =
//                     get_package_archive_path(&config.name, &config_source_hash);
//
//                 let mut package_archive = File::create(&package_archive_path).await?;
//
//                 package_archive.write_all(&stream_data).await?;
//
//                 let message = format!(
//                     "package tar cache remote fetched: {}",
//                     package_archive_path.display()
//                 );
//
//                 send(tx, message, None).await?;
//
//                 create_dir_all(&package_path).await?;
//
//                 match unpack_zstd(&package_path, &package_archive_path).await {
//                     Ok(_) => {}
//                     Err(_) => send_error(tx, "failed to unpack tar".to_string()).await?,
//                 }
//
//                 send(
//                     tx,
//                     format!(
//                         "package tar cache remote unpacked: {}",
//                         package_path.display()
//                     ),
//                     Some(ConfigPackageOutput {
//                         hash: config_source_hash.clone(),
//                         name: config.name.clone(),
//                     }),
//                 )
//                 .await?;
//
//                 break;
//             }
//         }
//
//         // check if package source exists in worker cache
//
//         let worker_package_source = StorePath {
//             kind: StorePathKind::Source as i32,
//             name: config.name.clone(),
//             hash: config_source_hash.clone(),
//         };
//
//         if let Ok(res) = worker_store.path(worker_package_source.clone()).await {
//             let store_path = res.into_inner();
//
//             let store_path_segments: Vec<&str> = store_path
//                 .uri
//                 .split('/')
//                 .filter(|segment| !segment.is_empty())
//                 .collect();
//
//             let store_path_file_name = store_path_segments.last().unwrap_or(&"");
//
//             let message = format!(
//                 "package source archive cache remote: {}",
//                 store_path_file_name
//             );
//
//             send(tx, message, None).await?;
//
//             build(
//                 tx,
//                 &config_build,
//                 &config.name,
//                 &config_source_hash,
//                 &worker.uri,
//             )
//             .await?;
//
//             break;
//         }
//
//         // check if source store exists in local cache
//
//         let source_archive_path = get_source_archive_path(&config.name, &config_source_hash);
//
//         if source_archive_path.exists() {
//             let message = format!("source archive: {}", source_archive_path.display());
//
//             send(tx, message, None).await?;
//
//             prepare(
//                 tx,
//                 &config.name,
//                 &config_source_hash,
//                 &source_archive_path,
//                 &worker.uri,
//             )
//             .await?;
//
//             build(
//                 tx,
//                 &config_build,
//                 &config.name,
//                 &config_source_hash,
//                 &worker.uri,
//             )
//             .await?;
//
//             break;
//         }
//
//         let source_hash = source::prepare(tx, &config_source, &source_archive_path).await?;
//
//         // TODO: check if the source archive exists on the worker already
//
//         if let Ok(res) = worker_store.path(worker_package_source.clone()).await {
//             let store_path = res.into_inner();
//
//             let store_path_segments: Vec<&str> = store_path
//                 .uri
//                 .split('/')
//                 .filter(|segment| !segment.is_empty())
//                 .collect();
//
//             let store_path_file_name = store_path_segments.last().unwrap_or(&"");
//
//             let message = format!(
//                 "package source archive cache remote: {}",
//                 store_path_file_name
//             );
//
//             send(tx, message, None).await?;
//
//             build(
//                 tx,
//                 &config_build,
//                 &config.name,
//                 &config_source_hash,
//                 &worker.uri,
//             )
//             .await?;
//
//             break;
//         }
//
//         prepare(
//             tx,
//             &config.name,
//             &source_hash,
//             &source_archive_path,
//             &worker.uri,
//         )
//         .await?;
//
//         build(tx, &config_build, &config.name, &source_hash, &worker.uri).await?;
//
//         break;
//     }
//
//     Ok(())
// }
