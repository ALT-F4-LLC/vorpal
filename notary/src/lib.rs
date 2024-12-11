use anyhow::Result;
use rand::rngs::OsRng;
use rsa::pkcs8::{
    DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding,
};
use rsa::pss::SigningKey;
use rsa::sha2::Sha256;
use rsa::signature::RandomizedSigner;
use rsa::signature::SignatureEncoding;
use rsa::{RsaPrivateKey, RsaPublicKey};
use std::path::PathBuf;
use tokio::fs;
use tokio::fs::create_dir_all;
use tracing::{info, warn};

const BITS: usize = 2048;

pub async fn generate_keys(
    key_path: PathBuf,
    private_key_path: PathBuf,
    public_key_path: PathBuf,
) -> Result<()> {
    if private_key_path.exists() {
        warn!("skipping - private key already exists");

        return Ok(());
    }

    if public_key_path.exists() {
        warn!("skipping - public key already exists");

        return Ok(());
    }

    create_dir_all(key_path.clone())
        .await
        .expect("failed to create key directory");

    info!("key directory: {:?}", key_path);

    let mut rng = rand::thread_rng();

    let private_key = RsaPrivateKey::new(&mut rng, BITS).expect("failed to generate private key");

    let private_key_der = private_key
        .to_pkcs8_der()
        .expect("failed to convert private key to DER");

    private_key_der
        .write_pem_file(&private_key_path, "PRIVATE KEY", LineEnding::LF)
        .expect("failed to write private key to file");

    info!("private key generated: {:?}", private_key_path);

    let public_key = private_key.to_public_key();

    public_key
        .write_public_key_pem_file(&public_key_path, LineEnding::LF)
        .expect("failed to write public key to file");

    info!("public key generated: {:?}", public_key_path);

    Ok(())
}

pub async fn get_private_key(private_key_path: PathBuf) -> Result<RsaPrivateKey> {
    let key_data = fs::read(private_key_path)
        .await
        .expect("failed to read private key");

    let key = std::str::from_utf8(&key_data).expect("failed to convert key to string");

    Ok(RsaPrivateKey::from_pkcs8_pem(key).expect("failed to parse private key"))
}

pub async fn get_public_key(public_key_path: PathBuf) -> Result<RsaPublicKey> {
    let key_data = fs::read(public_key_path)
        .await
        .expect("failed to read public key");

    let key = std::str::from_utf8(&key_data).expect("failed to convert key to string");

    Ok(RsaPublicKey::from_public_key_pem(key).expect("failed to parse public key"))
}

pub async fn sign(private_key_path: PathBuf, source_data: &[u8]) -> Result<Box<[u8]>> {
    let private_key = get_private_key(private_key_path).await?;

    let signing_key = SigningKey::<Sha256>::new(private_key);

    let mut signing_rng = OsRng;

    let signature = signing_key.sign_with_rng(&mut signing_rng, source_data);

    let signature_bytes = signature.to_bytes();

    Ok(signature_bytes)
}
