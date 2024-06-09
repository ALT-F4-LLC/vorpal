use crate::api::{PrepareRequest, PrepareResponse};
use crate::database;
use crate::notary;
use crate::store;
use futures_util::StreamExt;
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use tokio::fs;
use tonic::{Response, Status, Streaming};
use tracing::{error, info};

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

    let db = database::connect(store::get_database_path())
        .map_err(|_| Status::internal("Failed to connect to database"))?;

    if source_hash.is_empty() {
        return Err(Status::internal("Source hash is empty"));
    }

    if source_name.is_empty() {
        return Err(Status::internal("Source name is empty"));
    }

    let source_dir_path = store::get_source_dir_path(&source_name, &source_hash);
    let source_tar_path = store::get_source_tar_path(&source_name, &source_hash);

    if !source_tar_path.exists() {
        let mut source_tar = File::create(&source_tar_path)?;
        if let Err(e) = source_tar.write_all(&source_data) {
            error!("Failed source file: {}", e)
        } else {
            let metadata = fs::metadata(&source_tar_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o444);
            fs::set_permissions(&source_tar_path, permissions).await?;
            let file_name = source_tar_path.file_name().unwrap();
            info!("Source tar: {}", file_name.to_string_lossy());
        }

        fs::create_dir_all(&source_dir_path).await?;

        store::unpack_source(&source_dir_path, &source_tar_path).map_err(|e| {
            error!("Failed to unpack source: {:?}", e);
            Status::internal("Failed to unpack source")
        })?;

        let source_files =
            store::get_file_paths(&source_dir_path, &Vec::<&str>::new()).map_err(|e| {
                error!("Failed to get source files: {}", e);
                Status::internal("Failed to get source files")
            })?;

        info!("source files: {:?}", source_files);

        let source_files_hashes = store::get_file_hashes(&source_files).map_err(|e| {
            error!("Failed to get source files hashes: {}", e);
            Status::internal("Failed to get source files hashes")
        })?;

        let source_hash_computed = store::get_source_hash(&source_files_hashes).map_err(|e| {
            error!("Failed to get source hash: {}", e);
            Status::internal("Failed to get source hash")
        })?;

        info!("message source hash: {}", source_hash);
        info!("computed source hash: {}", source_hash_computed);

        if source_hash != source_hash_computed {
            error!("source hash mismatch");
            return Err(Status::internal("Source hash mismatch"));
        }

        database::insert_source(&db, &source_hash, &source_name).map_err(|e| {
            error!("Failed to insert source: {:?}", e);
            Status::internal("Failed to insert source")
        })?;

        fs::remove_dir_all(source_dir_path).await?;
    }

    let source_id = database::find_source(&db, &source_hash, &source_name)
        .map(|source| source.id)
        .map_err(|e| {
            error!("Failed to find source: {:?}", e);
            Status::internal("Failed to find source")
        })?;

    if let Err(e) = db.close() {
        error!("Failed to close database: {:?}", e)
    }

    let response = PrepareResponse { source_id };

    Ok(Response::new(response))
}
