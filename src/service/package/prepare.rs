use crate::api::{PackagePrepareRequest, PackagePrepareResponse};
use crate::notary;
use crate::store::archives::compress_gzip;
use crate::store::{hashes, paths, temps};
use async_compression::tokio::bufread::GzipDecoder;
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::convert::TryFrom;
use tokio::fs::remove_dir_all;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tokio_tar::Archive;
use tonic::{Request, Status, Streaming};
use tracing::debug;

async fn send_error(
    tx: &Sender<Result<PackagePrepareResponse, Status>>,
    message: String,
) -> Result<(), anyhow::Error> {
    debug!("send_error: {}", message);

    tx.send(Err(Status::internal(message.clone()))).await?;

    anyhow::bail!(message);
}

async fn send(
    tx: &Sender<Result<PackagePrepareResponse, Status>>,
    log_output: Vec<u8>,
) -> Result<(), anyhow::Error> {
    debug!("send: {:?}", String::from_utf8(log_output.clone()).unwrap());

    tx.send(Ok(PackagePrepareResponse { log_output })).await?;

    Ok(())
}

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
        let chunk = chunk?;

        source_chunks += 1;
        source_data.extend_from_slice(&chunk.source_data);
        source_hash = chunk.source_hash;
        source_name = chunk.source_name;
        source_signature = chunk.source_signature;
    }

    if source_hash.is_empty() {
        send_error(tx, "package source hash is empty".to_string()).await?
    }

    if source_name.is_empty() {
        send_error(tx, "package source name is empty".to_string()).await?
    }

    if source_signature.is_empty() {
        send_error(tx, "package source signature is empty".to_string()).await?
    }

    send(
        tx,
        format!("package source chunks received: {}", source_chunks).into_bytes(),
    )
    .await?;

    let source_tar_path = paths::get_package_source_tar_path(&source_name, &source_hash);

    if source_tar_path.exists() {
        send_error(
            tx,
            format!(
                "package source tar already exists: {}",
                source_tar_path.display()
            ),
        )
        .await?
    }

    // at this point we should be ready to prepare the source

    let public_key = notary::get_public_key().await?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    let signature_decode = match hex::decode(source_signature) {
        Ok(signature) => signature,
        Err(e) => return send_error(tx, format!("failed to decode signature: {:?}", e)).await,
    };

    let signature = Signature::try_from(signature_decode.as_slice())?;

    verifying_key.verify(&source_data, &signature)?;

    let source_sandbox_path = temps::create_dir().await?;

    send(
        tx,
        format!("package source sandbox: {}", source_sandbox_path.display()).into_bytes(),
    )
    .await?;

    let gz_decoder = GzipDecoder::new(source_data.as_slice());

    let mut archive = Archive::new(gz_decoder);

    archive.unpack(&source_sandbox_path).await?;

    let sandbox_file_paths = paths::get_file_paths(&source_sandbox_path, &Vec::<&str>::new())?;

    send(
        tx,
        format!("source sandbox files: {:?}", sandbox_file_paths.len()).into_bytes(),
    )
    .await?;

    let sandbox_file_paths_hashes = hashes::get_files(&sandbox_file_paths)?;

    let sandbox_source_hash = hashes::get_source(&sandbox_file_paths_hashes)?;

    send(
        tx,
        format!("source hash computed: {}", sandbox_source_hash).into_bytes(),
    )
    .await?;

    send(
        tx,
        format!("source hash provided: {}", source_hash).into_bytes(),
    )
    .await?;

    if source_hash != sandbox_source_hash {
        remove_dir_all(source_sandbox_path.clone()).await?;

        send_error(
            tx,
            format!(
                "source hash mismatch: {} != {}",
                source_hash, sandbox_source_hash
            )
            .to_string(),
        )
        .await?
    }

    compress_gzip(&source_sandbox_path, &sandbox_file_paths, &source_tar_path).await?;

    remove_dir_all(source_sandbox_path).await?;

    send(
        tx,
        format!("package source tar: {}", source_tar_path.display()).into_bytes(),
    )
    .await?;

    Ok(())
}
