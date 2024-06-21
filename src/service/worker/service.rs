use crate::api::package_service_server::PackageService;
use crate::api::store_service_server::StoreService;
use crate::api::{
    PackageBuildRequest, PackageBuildResponse, PackagePrepareRequest, PackagePrepareResponse,
    StoreFetchResponse, StorePath, StorePathKind, StorePathResponse,
};
use crate::notary;
use crate::service::worker::sandbox_default;
use crate::store::{archives, hashes, paths, temps};
use anyhow::Result;
use process_stream::{Process, ProcessExt};
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tera::Tera;
use tokio::fs::{create_dir_all, metadata, read, remove_dir_all, set_permissions, write, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use tracing::info;

#[derive(Debug, Default)]
pub struct Package {}

#[derive(Debug, Default)]
pub struct Store {}

#[tonic::async_trait]
impl StoreService for Store {
    type FetchStream = ReceiverStream<Result<StoreFetchResponse, Status>>;

    async fn fetch(
        &self,
        request: Request<StorePath>,
    ) -> Result<Response<Self::FetchStream>, Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let req = request.into_inner();

            let package_chunks_size = 8192;

            if req.kind == StorePathKind::Unknown as i32 {
                return Err(Status::invalid_argument("invalid store path kind"));
            }

            if req.kind == StorePathKind::Package as i32 {
                let package_tar_path = paths::get_package_tar_path(&req.name, &req.hash);

                if !package_tar_path.exists() {
                    return Err(Status::not_found("package archive not found"));
                }

                info!("serving package: {}", package_tar_path.display());

                let data = read(&package_tar_path)
                    .await
                    .map_err(|_| Status::internal("failed to read cached package"))?;

                for package_chunk in data.chunks(package_chunks_size) {
                    tx.send(Ok(StoreFetchResponse {
                        data: package_chunk.to_vec(),
                    }))
                    .await
                    .unwrap();
                }

                return Ok(());
            }

            let package_source_tar_path = paths::get_package_source_tar_path(&req.name, &req.hash);

            if !package_source_tar_path.exists() {
                return Err(Status::not_found("package source tar not found"));
            }

            info!(
                "serving package source: {}",
                package_source_tar_path.display()
            );

            let data = read(&package_source_tar_path)
                .await
                .map_err(|_| Status::internal("failed to read cached package"))?;

            for package_chunk in data.chunks(package_chunks_size) {
                tx.send(Ok(StoreFetchResponse {
                    data: package_chunk.to_vec(),
                }))
                .await
                .unwrap();
            }

            Ok(())
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn path(
        &self,
        request: Request<StorePath>,
    ) -> Result<Response<StorePathResponse>, Status> {
        let req = request.into_inner();

        if req.kind == StorePathKind::Unknown as i32 {
            return Err(Status::invalid_argument("invalid store path kind"));
        }

        if req.kind == StorePathKind::Package as i32 {
            let package_path = paths::get_package_path(&req.name, &req.hash);

            if !package_path.exists() {
                return Err(Status::not_found("package archive not found"));
            }

            return Ok(Response::new(StorePathResponse {
                uri: package_path.to_string_lossy().to_string(),
            }));
        }

        let package_source_tar_path = paths::get_package_source_tar_path(&req.name, &req.hash);

        if !package_source_tar_path.exists() {
            return Err(Status::not_found("package source tar not found"));
        }

        Ok(Response::new(StorePathResponse {
            uri: package_source_tar_path.to_string_lossy().to_string(),
        }))
    }
}

#[tonic::async_trait]
impl PackageService for Package {
    type BuildStream = ReceiverStream<Result<PackageBuildResponse, Status>>;
    type PrepareStream = ReceiverStream<Result<PackagePrepareResponse, Status>>;

    async fn prepare(
        &self,
        request: Request<Streaming<PackagePrepareRequest>>,
    ) -> Result<Response<Self::PrepareStream>, Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let mut source_data: Vec<u8> = Vec::new();
            let mut source_hash = String::new();
            let mut source_name = String::new();
            let mut source_signature = String::new();
            let mut source_chunks = 0;

            let mut stream = request.into_inner();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| Status::internal(format!("stream error: {}", e)))?;

                source_chunks += 1;
                source_data.extend_from_slice(&chunk.source_data);
                source_hash = chunk.source_hash;
                source_name = chunk.source_name;
                source_signature = chunk.source_signature;
            }

            if source_hash.is_empty() {
                return Err(Status::internal("source hash is empty"));
            }

            if source_name.is_empty() {
                return Err(Status::internal("source name is empty"));
            }

            if source_signature.is_empty() {
                return Err(Status::internal("source signature is empty"));
            }

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!("package source chunks received: {}", source_chunks)
                    .into_bytes(),
            }))
            .await
            .unwrap();

            let package_source_path = paths::get_package_path(&source_name, &source_hash);

            if package_source_path.exists() {
                tx.send(Ok(PackagePrepareResponse {
                    log_output: format!(
                        "package source already prepared: {}",
                        package_source_path.display()
                    )
                    .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;
                return Err(Status::already_exists("package source already prepared"));
            }

            let package_source_tar_path =
                paths::get_package_source_tar_path(&source_name, &source_hash);

            if package_source_tar_path.exists() {
                tx.send(Ok(PackagePrepareResponse {
                    log_output: format!(
                        "package source tar already prepared: {}",
                        package_source_tar_path.display()
                    )
                    .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;
                return Err(Status::already_exists(
                    "package source tar already prepared",
                ));
            }

            // at this point we should be ready to prepare the source

            let public_key = notary::get_public_key()
                .await
                .map_err(|_| Status::internal("failed to get public key"))?;

            let verifying_key = VerifyingKey::<Sha256>::new(public_key);

            let signature_decode = hex::decode(source_signature)
                .map_err(|_| Status::internal("hex decode of signature failed"))?;

            let signature = Signature::try_from(signature_decode.as_slice())
                .map_err(|_| Status::internal("failed to decode signature"))?;

            verifying_key
                .verify(&source_data, &signature)
                .map_err(|_| Status::internal("failed to verify signature"))?;

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!(
                    "source tar not found: {}",
                    package_source_tar_path.display()
                )
                .into_bytes(),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"))?;

            let mut source_tar = File::create(&package_source_tar_path).await?;
            if let Err(e) = source_tar.write_all(&source_data).await {
                return Err(Status::internal(format!(
                    "failed to write source tar: {}",
                    e
                )));
            } else {
                let metadata = metadata(&package_source_tar_path).await?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o444);
                set_permissions(package_source_tar_path.clone(), permissions).await?;
                let file_name = package_source_tar_path.file_name().unwrap();
                tx.send(Ok(PackagePrepareResponse {
                    log_output: format!("source tar created: {}", file_name.to_string_lossy())
                        .into_bytes(),
                }))
                .await
                .unwrap();
            }

            let temp_source_path = temps::create_dir()
                .await
                .map_err(|_| Status::internal("failed to create temp dir"))?;

            create_dir_all(&temp_source_path).await?;

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!("package source unpacking: {}", temp_source_path.display())
                    .into_bytes(),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"))?;

            if let Err(err) =
                archives::unpack_tar_gz(&temp_source_path, &package_source_tar_path).await
            {
                return Err(Status::internal(format!(
                    "failed to unpack source tar: {}",
                    err
                )));
            }

            let temp_file_paths = paths::get_file_paths(&temp_source_path, &Vec::<&str>::new())
                .map_err(|e| Status::internal(format!("failed to get source files: {:?}", e)))?;

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!("source files: {:?}", temp_file_paths.len()).into_bytes(),
            }))
            .await
            .unwrap();

            let temp_files_hashes = hashes::get_files(&temp_file_paths).map_err(|e| {
                Status::internal(format!("failed to get source file hashes: {:?}", e))
            })?;

            let temp_hash_computed = hashes::get_source(&temp_files_hashes)
                .map_err(|e| Status::internal(format!("failed to get source hash: {:?}", e)))?;

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!("source hash: {}", source_hash).into_bytes(),
            }))
            .await
            .unwrap();

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!("source hash expected: {}", temp_hash_computed).into_bytes(),
            }))
            .await
            .unwrap();

            if source_hash != temp_hash_computed {
                return Err(Status::internal("package source hash mismatch"));
            }

            remove_dir_all(temp_source_path).await?;

            Ok(())
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn build(
        &self,
        request: Request<PackageBuildRequest>,
    ) -> Result<Response<Self::BuildStream>, Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let req = request.into_inner();

            let package_path = paths::get_package_path(&req.source_name, &req.source_hash);

            if package_path.exists() {
                tx.send(Ok(PackageBuildResponse {
                    log_output: format!("package already built: {}", package_path.display())
                        .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;
                return Err(Status::already_exists("package already built"));
            }

            let package_tar_path = paths::get_package_tar_path(&req.source_name, &req.source_hash);

            if !package_path.exists() && package_tar_path.exists() {
                tx.send(Ok(PackageBuildResponse {
                    log_output: format!(
                        "package tar found (unpacking): {}",
                        package_tar_path.display()
                    )
                    .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;

                create_dir_all(&package_path)
                    .await
                    .map_err(|_| Status::internal("failed to create package dir"))?;

                if let Err(err) = archives::unpack_tar_gz(&package_path, &package_tar_path).await {
                    return Err(Status::internal(format!(
                        "failed to unpack source tar: {:?}",
                        err
                    )));
                }

                return Err(Status::internal("package already built"));
            }

            let build_path = temps::create_dir()
                .await
                .map_err(|_| Status::internal("failed to create temp dir"))?;

            let package_source_path =
                paths::get_package_source_path(&req.source_name, &req.source_hash);

            if package_source_path.exists() {
                tx.send(Ok(PackageBuildResponse {
                    log_output: format!("package source found: {}", package_source_path.display())
                        .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;

                paths::copy_files(&package_source_path, &build_path)
                    .await
                    .map_err(|e| {
                        Status::internal(format!("failed to copy source files: {:?}", e))
                    })?;

                tx.send(Ok(PackageBuildResponse {
                    log_output: format!("package source copied: {}", build_path.display())
                        .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;
            }

            let package_source_tar_path =
                paths::get_package_source_tar_path(&req.source_name, &req.source_hash);

            if !package_source_path.exists() && package_source_tar_path.exists() {
                tx.send(Ok(PackageBuildResponse {
                    log_output: format!(
                        "package source tar found: {}",
                        package_source_tar_path.display()
                    )
                    .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;

                create_dir_all(&package_source_path)
                    .await
                    .map_err(|_| Status::internal("failed to create package source dir"))?;

                tx.send(Ok(PackageBuildResponse {
                    log_output: format!(
                        "package source unpacking: {}",
                        package_source_path.display()
                    )
                    .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;

                if let Err(err) =
                    archives::unpack_tar_gz(&package_source_path, &package_source_tar_path).await
                {
                    return Err(Status::internal(format!(
                        "failed to unpack source tar: {:?}",
                        err
                    )));
                }

                tx.send(Ok(PackageBuildResponse {
                    log_output: format!(
                        "package source copying: {}",
                        package_source_path.display()
                    )
                    .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;

                paths::copy_files(&package_source_path, &build_path)
                    .await
                    .map_err(|e| {
                        Status::internal(format!("failed to copy source files: {:?}", e))
                    })?;

                tx.send(Ok(PackageBuildResponse {
                    log_output: format!("package source copied: {}", build_path.display())
                        .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;
            }

            let build_source_file_paths = paths::get_file_paths(&build_path, &Vec::<&str>::new())
                .map_err(|e| {
                Status::internal(format!("failed to get source files: {:?}", e))
            })?;

            if build_source_file_paths.is_empty() {
                return Err(Status::internal("no source files found"));
            }

            // at this point we should be ready to build with source files

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("package building: {}", build_path.display()).into_bytes(),
            }))
            .await
            .unwrap();

            let build_vorpal_path = build_path.join(".vorpal");

            create_dir_all(&build_vorpal_path)
                .await
                .map_err(|_| Status::internal("failed to create build vorpal dir"))?;

            let package_build_script = req
                .build_script
                .trim()
                .split('\n')
                .map(|line| line.trim())
                .collect::<Vec<&str>>()
                .join("\n");

            let build_script = [
                "#!/bin/bash",
                "set -e pipefail",
                "echo \"PATH: $PATH\"",
                "echo \"Starting build script\"",
                &package_build_script,
                "echo \"Finished build script\"",
            ];

            let build_script_data = build_script.join("\n");

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("package build script: {}", build_script_data).into_bytes(),
            }))
            .await
            .unwrap();

            let build_script_path = build_vorpal_path.join("build.sh");

            write(&build_script_path, build_script_data)
                .await
                .map_err(|_| Status::internal("failed to write build script"))?;

            set_permissions(&build_script_path, fs::Permissions::from_mode(0o755))
                .await
                .map_err(|_| Status::internal("failed to set build script permissions"))?;

            let build_profile_path = build_vorpal_path.join("sandbox.sb");

            let mut tera = Tera::default();
            tera.add_raw_template("sandbox_default", sandbox_default::SANDBOX_DEFAULT)
                .unwrap();

            let mut context = tera::Context::new();
            context.insert("tmpdir", build_path.to_str().unwrap());
            let default_profile = tera.render("sandbox_default", &context).unwrap();

            write(&build_profile_path, default_profile)
                .await
                .map_err(|_| Status::internal("failed to write sandbox profile"))?;

            if !build_profile_path.exists() {
                return Err(Status::internal("build profile not found"));
            }

            if !build_script_path.exists() {
                return Err(Status::internal("build script not found"));
            }

            let build_command_args = [
                "-f",
                build_profile_path.to_str().unwrap(),
                build_script_path.to_str().unwrap(),
            ];

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("build args: {:?}", build_command_args).into_bytes(),
            }))
            .await
            .unwrap();

            let mut build_environment = HashMap::new();

            for (key, value) in req.build_environment.clone() {
                build_environment.insert(key, value);
            }

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("build packages: {:?}", req.build_packages).into_bytes(),
            }))
            .await
            .unwrap();

            let mut build_store_paths = vec![];

            for path in req.build_packages {
                let build_package = paths::get_package_path(&path.name, &path.hash);
                if !build_package.exists() {
                    return Err(Status::internal("build package not found"));
                }

                let package_bin_path = build_package.join("bin");
                if package_bin_path.exists() {
                    build_store_paths.push(package_bin_path.canonicalize()?.display().to_string());
                }

                build_environment.insert(
                    path.name.replace('-', "_").to_string(),
                    build_package.canonicalize()?.display().to_string(),
                );
            }

            let os_type = env::consts::OS;

            if os_type != "macos" {
                return Err(Status::unimplemented("unsupported os (macos only)"));
            }

            if os_type == "macos" {
                build_store_paths.push("/usr/bin".to_string());
                build_store_paths.push("/bin".to_string());
                build_store_paths.push("/Library/Developer/CommandLineTools/usr/bin".to_string());
            }

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("build store paths: {:?}", build_store_paths).into_bytes(),
            }))
            .await
            .unwrap();

            build_environment.insert("PATH".to_string(), build_store_paths.join(":"));

            let build_output_dir = temps::create_dir()
                .await
                .map_err(|_| Status::internal("failed to create temp dir"))?;

            let build_output_path = build_output_dir.canonicalize()?;

            build_environment.insert(
                "output".to_string(),
                build_output_path.display().to_string(),
            );

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("build output path: {}", build_output_path.display())
                    .into_bytes(),
            }))
            .await
            .unwrap();

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("build environment: {:?}", build_environment).into_bytes(),
            }))
            .await
            .unwrap();

            let mut sandbox_command = Process::new("/usr/bin/sandbox-exec");
            sandbox_command.args(build_command_args);
            sandbox_command.current_dir(&build_path);

            for (key, value) in build_environment {
                sandbox_command.env(key, value);
            }

            let mut stream = sandbox_command.spawn_and_stream()?;

            while let Some(output) = stream.next().await {
                tx.send(Ok(PackageBuildResponse {
                    log_output: output.as_bytes().to_vec(),
                }))
                .await
                .unwrap();
            }

            // TODO: properly handle error when sandbox command fails

            if let Err(err) = sandbox_command.status().await {
                tx.send(Ok(PackageBuildResponse {
                    log_output: format!("build failed: {:?}", err).into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;
                return Err(Status::internal("sandbox command failed"));
            }

            let build_output_files = paths::get_file_paths(&build_output_path, &Vec::<&str>::new())
                .map_err(|_| Status::internal("failed to get sandbox output files"))?;

            if build_output_files.is_empty() {
                tx.send(Ok(PackageBuildResponse {
                    log_output: format!(
                        "no build output files found: {}",
                        build_output_path.display()
                    )
                    .into_bytes(),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"))?;
                return Err(Status::internal("no build output files found"));
            }

            create_dir_all(&package_path)
                .await
                .map_err(|_| Status::internal("failed to create package dir"))?;

            paths::copy_files(&build_output_path, &package_path)
                .await
                .map_err(|e| Status::internal(format!("failed to copy source files: {:?}", e)))?;

            if let Err(err) = archives::compress_tar_gz(
                &build_output_path,
                &build_output_files,
                &package_tar_path,
            )
            .await
            {
                return Err(Status::internal(format!(
                    "failed to compress sandbox output: {:?}",
                    err
                )));
            }

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("package tar created: {}", package_tar_path.display())
                    .into_bytes(),
            }))
            .await
            .unwrap();

            remove_dir_all(&build_path).await?;

            Ok(())
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
