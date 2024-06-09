use crate::api::{PrepareRequest, PrepareResponse};
use crate::database;
use crate::notary;
use crate::store::archives;
use crate::store::hashes;
use crate::store::paths;
use futures_util::StreamExt;
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::os::unix::fs::PermissionsExt;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tonic::{Response, Status, Streaming};
use tracing::info;

pub async fn run(
    mut stream: Streaming<PrepareRequest>,
) -> Result<Response<PrepareResponse>, Status> {
    let mut source_data: Vec<u8> = Vec::new();

    let mut source_hash = String::new();
    let mut source_name = String::new();
    let mut source_signature = String::new();
    let mut source_chunks = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Status::internal(format!("Stream error: {}", e)))?;

        info!("chunk size: {}", chunk.source_data.len());

        if source_chunks == 0 {
            source_hash = chunk.source_hash;
            source_name = chunk.source_name;
            source_signature = chunk.source_signature;
            source_chunks += 1;
        }

        source_data.extend_from_slice(&chunk.source_data);
    }

    info!("processed chunks: {}", source_chunks);

    let public_key = notary::get_public_key()
        .await
        .map_err(|_| Status::internal("Failed to get public key"))?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    if source_signature.is_empty() {
        return Err(Status::internal("Source signature is empty"));
    }

    let signature_decode = hex::decode(source_signature)
        .map_err(|_| Status::internal("hex decode of signature failed"))?;

    let signature = Signature::try_from(signature_decode.as_slice())
        .map_err(|_| Status::internal("Failed to decode signature"))?;

    verifying_key
        .verify(&source_data, &signature)
        .map_err(|_| Status::internal("Failed to verify signature"))?;

    if source_hash.is_empty() {
        return Err(Status::internal("Source hash is empty"));
    }

    if source_name.is_empty() {
        return Err(Status::internal("Source name is empty"));
    }

    let package_source_path = paths::get_package_source_path(&source_name, &source_hash);
    let package_source_tar_path = paths::get_package_source_tar_path(&source_name, &source_hash);

    let db = database::connect(paths::get_database_path())
        .map_err(|_| Status::internal("Failed to connect to database"))?;

    if !package_source_tar_path.exists() {
        let mut source_tar = File::create(&package_source_tar_path).await?;
        if let Err(e) = source_tar.write_all(&source_data).await {
            return Err(Status::internal(format!(
                "Failed to write source tar: {}",
                e
            )));
        } else {
            let metadata = fs::metadata(&package_source_tar_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o444);
            fs::set_permissions(&package_source_tar_path, permissions).await?;
            let file_name = package_source_tar_path.file_name().unwrap();
            info!("source tar created: {}", file_name.to_string_lossy());
        }

        fs::create_dir_all(&package_source_path).await?;

        if let Err(err) =
            archives::unpack_tar_gz(&package_source_path, &package_source_tar_path).await
        {
            return Err(Status::internal(format!(
                "Failed to unpack source tar: {}",
                err
            )));
        }

        let source_file_paths = paths::get_file_paths(&package_source_path, &Vec::<&str>::new())
            .map_err(|e| Status::internal(format!("Failed to get source files: {:?}", e)))?;

        info!("source files: {:?}", source_file_paths);

        let source_files_hashes = hashes::get_files(&source_file_paths)
            .map_err(|e| Status::internal(format!("Failed to get source file hashes: {:?}", e)))?;

        let source_hash_computed = hashes::get_source(&source_files_hashes)
            .map_err(|e| Status::internal(format!("Failed to get source hash: {:?}", e)))?;

        info!("message source hash: {}", source_hash);
        info!("computed source hash: {}", source_hash_computed);

        if source_hash != source_hash_computed {
            return Err(Status::internal("source hash mismatch"));
        }

        database::insert_source(&db, &source_hash, &source_name)
            .map_err(|e| Status::internal(format!("Failed to insert source: {:?}", e)))?;

        fs::remove_dir_all(package_source_path).await?;
    }

    let source_id = database::find_source(&db, &source_hash, &source_name)
        .map(|source| source.id)
        .map_err(|e| Status::internal(format!("Failed to find source: {:?}", e.to_string())))?;

    if let Err(e) = db.close() {
        return Err(Status::internal(format!(
            "Failed to close database: {:?}",
            e.1
        )));
    }

    let response = PrepareResponse { source_id };

    Ok(Response::new(response))
}
