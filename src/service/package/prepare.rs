use crate::api::{PackagePrepareRequest, PackagePrepareResponse};
use crate::notary;
use crate::store::{archives, hashes, paths, temps};
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::convert::TryFrom;
use std::os::unix::fs::PermissionsExt;
use tokio::fs::{create_dir_all, metadata, remove_dir_all, set_permissions, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tonic::{Request, Status, Streaming};

pub async fn run(
    tx: &Sender<Result<PackagePrepareResponse, Status>>,
    request: Request<Streaming<PackagePrepareRequest>>,
) -> Result<(), anyhow::Error> {
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
        anyhow::bail!("source hash is empty");
    }

    if source_name.is_empty() {
        anyhow::bail!("source name is empty");
    }

    if source_signature.is_empty() {
        anyhow::bail!("source signature is empty");
    }

    tx.send(Ok(PackagePrepareResponse {
        log_output: format!("source chunks received: {}", source_chunks).into_bytes(),
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

        anyhow::bail!("package source already prepared");
    }

    let package_source_tar_path = paths::get_package_source_tar_path(&source_name, &source_hash);

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

        anyhow::bail!("package source tar already prepared");
    }

    // at this point we should be ready to prepare the source

    let public_key = notary::get_public_key()
        .await
        .map_err(|_| Status::internal("failed to get public key"))?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    let signature_decode = hex::decode(source_signature).map_err(|err| {
        println!("hex decode error: {:?}", err);
        Status::internal("hex decode of signature failed")
    })?;

    let signature = Signature::try_from(signature_decode.as_slice())
        .map_err(|_| Status::internal("failed to decode signature"))?;

    verifying_key
        .verify(&source_data, &signature)
        .map_err(|_| Status::internal("failed to verify signature"))?;

    let mut source_tar = File::create(&package_source_tar_path).await?;

    if let Err(e) = source_tar.write_all(&source_data).await {
        anyhow::bail!("failed to write source tar: {:?}", e);
    } else {
        let metadata = metadata(&package_source_tar_path).await?;

        let mut permissions = metadata.permissions();
        permissions.set_mode(0o444);

        set_permissions(package_source_tar_path.clone(), permissions).await?;

        let file_name = package_source_tar_path.file_name().unwrap();

        tx.send(Ok(PackagePrepareResponse {
            log_output: format!("source tar created: {}", file_name.to_string_lossy()).into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;
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

    if let Err(err) = archives::unpack_tar_gz(&temp_source_path, &package_source_tar_path).await {
        anyhow::bail!("failed to unpack source tar: {:?}", err);
    }

    let temp_file_paths = paths::get_file_paths(&temp_source_path, &Vec::<&str>::new())
        .map_err(|e| Status::internal(format!("failed to get source files: {:?}", e)))?;

    tx.send(Ok(PackagePrepareResponse {
        log_output: format!("source files: {:?}", temp_file_paths.len()).into_bytes(),
    }))
    .await
    .unwrap();

    let temp_files_hashes = hashes::get_files(&temp_file_paths)
        .map_err(|e| Status::internal(format!("failed to get source file hashes: {:?}", e)))?;

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
        anyhow::bail!("source hash mismatch");
    }

    remove_dir_all(temp_source_path).await?;

    Ok(())
}
