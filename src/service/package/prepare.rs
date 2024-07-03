use crate::api::{PackagePrepareRequest, PackagePrepareResponse};
use crate::notary::get_public_key;
use crate::store::paths::get_package_source_archive_path;
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::convert::TryFrom;
use tokio::fs::write;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
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
    log_output: String,
) -> Result<(), anyhow::Error> {
    debug!("{}", log_output);

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
        format!("package source chunks received: {}", source_chunks),
    )
    .await?;

    let source_archive_path = get_package_source_archive_path(&source_name, &source_hash);

    if source_archive_path.exists() {
        send_error(
            tx,
            format!(
                "package source tar already exists: {}",
                source_archive_path.display()
            ),
        )
        .await?
    }

    // at this point we should be ready to prepare the source

    let public_key = get_public_key().await?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    let signature_decode = match hex::decode(source_signature) {
        Ok(signature) => signature,
        Err(e) => return send_error(tx, format!("failed to decode signature: {:?}", e)).await,
    };

    let signature = Signature::try_from(signature_decode.as_slice())?;

    verifying_key.verify(&source_data, &signature)?;

    write(&source_archive_path, &source_data).await?;

    send(
        tx,
        format!("package source tar: {}", source_archive_path.display()),
    )
    .await?;

    Ok(())
}
