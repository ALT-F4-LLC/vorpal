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
use std::convert::TryFrom;
use std::os::unix::fs::PermissionsExt;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Response, Status, Streaming};

type PrepareStream = ReceiverStream<Result<PrepareResponse, Status>>;

pub async fn run(mut stream: Streaming<PrepareRequest>) -> Result<Response<PrepareStream>, Status> {
    let (tx, rx) = mpsc::channel(4);

    tokio::spawn(async move {
        let mut source_data: Vec<u8> = Vec::new();
        let mut source_hash = String::new();
        let mut source_name = String::new();
        let mut source_signature = String::new();
        let mut source_chunks = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Status::internal(format!("Stream error: {}", e)))?;

            if source_chunks == 0 {
                source_hash = chunk.source_hash;
                source_name = chunk.source_name;
                source_signature = chunk.source_signature;
            }

            source_chunks += 1;
            source_data.extend_from_slice(&chunk.source_data);
        }

        tx.send(Ok(PrepareResponse {
            source_id: 0,
            source_log: format!("source chunks received: {}", source_chunks),
        }))
        .await
        .unwrap();

        let db = database::connect(paths::get_database_path())
            .map_err(|_| Status::internal("failed to connect to database"))?;

        let public_key = notary::get_public_key()
            .await
            .map_err(|_| Status::internal("failed to get public key"))?;

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        if source_signature.is_empty() {
            return Err(Status::internal("source signature is empty"));
        }

        let signature_decode = hex::decode(source_signature)
            .map_err(|_| Status::internal("hex decode of signature failed"))?;

        let signature = Signature::try_from(signature_decode.as_slice())
            .map_err(|_| Status::internal("failed to decode signature"))?;

        verifying_key
            .verify(&source_data, &signature)
            .map_err(|_| Status::internal("failed to verify signature"))?;

        if source_hash.is_empty() {
            return Err(Status::internal("source hash is empty"));
        }

        if source_name.is_empty() {
            return Err(Status::internal("source name is empty"));
        }

        let package_source_path = paths::get_package_source_path(&source_name, &source_hash);
        let package_source_tar_path =
            paths::get_package_source_tar_path(&source_name, &source_hash);

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
                fs::set_permissions(package_source_tar_path.clone(), permissions).await?;
                let file_name = package_source_tar_path.file_name().unwrap();
                tx.send(Ok(PrepareResponse {
                    source_id: 0,
                    source_log: format!("source tar created: {}", file_name.to_string_lossy()),
                }))
                .await
                .unwrap();
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

            let source_file_paths =
                paths::get_file_paths(&package_source_path, &Vec::<&str>::new()).map_err(|e| {
                    Status::internal(format!("Failed to get source files: {:?}", e))
                })?;

            tx.send(Ok(PrepareResponse {
                source_id: 0,
                source_log: format!("source files: {:?}", source_file_paths.len()),
            }))
            .await
            .unwrap();

            let source_files_hashes = hashes::get_files(&source_file_paths).map_err(|e| {
                Status::internal(format!("Failed to get source file hashes: {:?}", e))
            })?;

            let source_hash_computed = hashes::get_source(&source_files_hashes)
                .map_err(|e| Status::internal(format!("Failed to get source hash: {:?}", e)))?;

            tx.send(Ok(PrepareResponse {
                source_id: 0,
                source_log: format!("source hash: {}", source_hash),
            }))
            .await
            .unwrap();

            tx.send(Ok(PrepareResponse {
                source_id: 0,
                source_log: format!("source hash expected: {}", source_hash_computed),
            }))
            .await
            .unwrap();

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

        tx.send(Ok(PrepareResponse {
            source_id,
            source_log: "".to_string(),
        }))
        .await
        .unwrap();

        if let Err(e) = db.close() {
            return Err(Status::internal(format!(
                "Failed to close database: {:?}",
                e.1
            )));
        }

        Ok(())
    });

    Ok(Response::new(ReceiverStream::new(rx)))
}
