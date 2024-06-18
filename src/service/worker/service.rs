use crate::api::package_service_server::PackageService;
use crate::api::store_service_server::StoreService;
use crate::api::{
    PackageBuildRequest, PackageBuildResponse, PackagePrepareRequest, PackagePrepareResponse,
    StoreFetchResponse, StorePath, StorePathKind, StorePathResponse,
};
use crate::notary;
use crate::service::worker::sandbox_default;
use crate::store::archives;
use crate::store::hashes;
use crate::store::paths;
use crate::store::temps;
use anyhow::Result;
use process_stream::{Process, ProcessExt};
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::convert::TryFrom;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tera::Tera;
use tokio::fs::{
    copy, create_dir, create_dir_all, metadata, read, remove_dir_all, set_permissions, write, File,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use tracing::info;
use walkdir::WalkDir;

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
                log_output: format!("package source chunks received: {}", source_chunks),
            }))
            .await
            .unwrap();

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

            let package_source_tar_path =
                paths::get_package_source_tar_path(&source_name, &source_hash);

            if !package_source_tar_path.exists() {
                tx.send(Ok(PackagePrepareResponse {
                    log_output: format!(
                        "source tar not found: {}",
                        package_source_tar_path.display()
                    ),
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
                        log_output: format!("source tar created: {}", file_name.to_string_lossy()),
                    }))
                    .await
                    .unwrap();
                }
            }

            let temp_source_path = temps::create_dir()
                .await
                .map_err(|_| Status::internal("failed to create temp dir"))?;

            create_dir_all(&temp_source_path).await?;

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
                log_output: format!("source files: {:?}", temp_file_paths.len()),
            }))
            .await
            .unwrap();

            let temp_files_hashes = hashes::get_files(&temp_file_paths).map_err(|e| {
                Status::internal(format!("failed to get source file hashes: {:?}", e))
            })?;

            let temp_hash_computed = hashes::get_source(&temp_files_hashes)
                .map_err(|e| Status::internal(format!("failed to get source hash: {:?}", e)))?;

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!("source hash: {}", source_hash),
            }))
            .await
            .unwrap();

            tx.send(Ok(PackagePrepareResponse {
                log_output: format!("source hash expected: {}", temp_hash_computed),
            }))
            .await
            .unwrap();

            if source_hash != temp_hash_computed {
                return Err(Status::internal("source hash mismatch"));
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

            let package_source_tar_path =
                paths::get_package_source_tar_path(&req.source_name, &req.source_hash);

            if !package_source_tar_path.exists() {
                return Err(Status::internal(
                    "package source tar not found - must be prepared first",
                ));
            }

            tx.send(Ok(PackageBuildResponse {
                log_output: format!(
                    "package source building: {}",
                    package_source_tar_path.display()
                ),
            }))
            .await
            .unwrap();

            let package_build_path = temps::create_dir()
                .await
                .map_err(|_| Status::internal("failed to create temp dir"))?;
            let package_build_path = package_build_path.canonicalize()?;

            if let Err(err) =
                archives::unpack_tar_gz(&package_build_path, &package_source_tar_path).await
            {
                return Err(Status::internal(format!(
                    "failed to unpack source tar: {:?}",
                    err
                )));
            }

            let build_vorpal_dir = package_build_path.join(".vorpal");

            create_dir(&build_vorpal_dir)
                .await
                .map_err(|_| Status::internal("failed to create vorpal temp dir"))?;

            let build_phase_steps = req
                .build_script
                .trim()
                .split('\n')
                .map(|line| line.trim())
                .collect::<Vec<&str>>()
                .join("\n");

            let automation_script = [
                "#!/bin/bash",
                "set -e pipefail",
                "echo \"Starting build script\"",
                &build_phase_steps,
                "echo \"Finished build script\"",
            ];

            let automation_script_data = automation_script.join("\n");

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("package build script: {}", automation_script_data),
            }))
            .await
            .unwrap();

            let sandbox_build_script_path = build_vorpal_dir.join("automation.sh");

            write(&sandbox_build_script_path, automation_script_data)
                .await
                .map_err(|_| Status::internal("failed to write automation script"))?;

            set_permissions(
                &sandbox_build_script_path,
                fs::Permissions::from_mode(0o755),
            )
            .await
            .map_err(|_| Status::internal("failed to set automation script permissions"))?;

            let os_type = std::env::consts::OS;
            if os_type != "macos" {
                return Err(Status::unimplemented("unsupported OS"));
            }

            let sandbox_profile_path = build_vorpal_dir.join("sandbox.sb");

            let mut tera = Tera::default();
            tera.add_raw_template("sandbox_default", sandbox_default::SANDBOX_DEFAULT)
                .unwrap();

            let mut context = tera::Context::new();
            context.insert("tmpdir", package_build_path.to_str().unwrap());
            let sandbox_profile = tera.render("sandbox_default", &context).unwrap();

            write(&sandbox_profile_path, sandbox_profile)
                .await
                .map_err(|_| Status::internal("failed to write sandbox profile"))?;

            if !sandbox_profile_path.exists() {
                return Err(Status::internal("sandbox profile not found"));
            }

            if !sandbox_build_script_path.exists() {
                return Err(Status::internal("automation script not found"));
            }

            let sandbox_command_args = [
                "-f",
                sandbox_profile_path.to_str().unwrap(),
                sandbox_build_script_path.to_str().unwrap(),
            ];

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("package sandbox args: {:?}", sandbox_command_args),
            }))
            .await
            .unwrap();

            let sandbox_output_path = build_vorpal_dir.join("output");

            create_dir_all(&sandbox_output_path)
                .await
                .map_err(|_| Status::internal("failed to create sandbox output dir"))?;

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("package sandbox log: {}", sandbox_output_path.display()),
            }))
            .await
            .unwrap();

            let mut sandbox_command = Process::new("/usr/bin/sandbox-exec");
            sandbox_command.args(sandbox_command_args);
            sandbox_command.current_dir(&package_build_path);
            // sandbox_command.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
            sandbox_command.env("OUTPUT", sandbox_output_path.to_str().unwrap());

            let mut stream = sandbox_command.spawn_and_stream()?;

            while let Some(output) = stream.next().await {
                tx.send(Ok(PackageBuildResponse {
                    log_output: format!("package sandbox response: {}", output),
                }))
                .await
                .unwrap();
            }

            let sandbox_output_files =
                paths::get_file_paths(&sandbox_output_path, &Vec::<&str>::new())
                    .map_err(|_| Status::internal("failed to get sandbox output files"))?;

            if sandbox_output_files.is_empty() {
                return Err(Status::internal("sandbox output is empty"));
            }

            let package_path = paths::get_package_path(&req.source_name, &req.source_hash);

            for entry in WalkDir::new(&sandbox_output_path) {
                let entry = entry.map_err(|e| {
                    Status::internal(format!("failed to walk sandbox output: {:?}", e))
                })?;

                let output_path = entry.path().strip_prefix(&sandbox_output_path).unwrap();
                let output_store_path = package_path.join(output_path);

                if entry.path().is_dir() {
                    create_dir_all(&output_store_path)
                        .await
                        .map_err(|_| Status::internal("failed to create sandbox output dir"))?;
                } else {
                    copy(&entry.path(), &output_store_path)
                        .await
                        .map_err(|_| Status::internal("failed to copy sandbox output file"))?;
                }
            }

            remove_dir_all(&package_build_path).await?;

            let package_tar_path = paths::get_package_tar_path(&req.source_name, &req.source_hash);

            let package_files = paths::get_file_paths(&package_path, &Vec::<&str>::new())
                .map_err(|_| Status::internal("failed to get sandbox output files"))?;

            if let Err(err) =
                archives::compress_tar_gz(&package_path, &package_tar_path, &package_files).await
            {
                return Err(Status::internal(format!(
                    "failed to compress sandbox output: {:?}",
                    err
                )));
            }

            tx.send(Ok(PackageBuildResponse {
                log_output: format!("package tar created: {}", package_tar_path.display()),
            }))
            .await
            .unwrap();

            Ok(())
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
