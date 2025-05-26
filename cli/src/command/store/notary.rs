use anyhow::Result;
use base64::{engine::general_purpose, Engine};
use rsa::pss::SigningKey;
use rsa::sha2::Sha256;
use rsa::signature::RandomizedSigner;
use rsa::signature::SignatureEncoding;
use rsa::{
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    rand_core, Pkcs1v15Encrypt,
};
use rsa::{RsaPrivateKey, RsaPublicKey};
use std::path::PathBuf;
use tokio::fs;
use tokio::fs::create_dir_all;
use tracing::warn;

const BITS: usize = 4096;

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

    let mut rng = rand_core::OsRng;

    let private_key = RsaPrivateKey::new(&mut rng, BITS).expect("failed to generate private key");

    let private_key_der = private_key
        .to_pkcs8_der()
        .expect("failed to convert private key to DER");

    private_key_der
        .write_pem_file(&private_key_path, "PRIVATE KEY", LineEnding::LF)
        .expect("failed to write private key to file");

    let public_key = private_key.to_public_key();

    public_key
        .write_public_key_pem_file(&public_key_path, LineEnding::LF)
        .expect("failed to write public key to file");

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

    let mut signing_rng = rand_core::OsRng;

    let signature = signing_key.sign_with_rng(&mut signing_rng, source_data);

    let signature_bytes = signature.to_bytes();

    Ok(signature_bytes)
}

pub async fn encrypt(public_key_path: PathBuf, data: String) -> Result<String> {
    let public_key = get_public_key(public_key_path).await?;

    let mut rng = rand_core::OsRng;

    let data_encrypted = public_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, data.as_bytes())
        .expect("failed to encrypt");

    let data_encoded = general_purpose::STANDARD.encode(&data_encrypted);

    Ok(data_encoded)
}

pub async fn decrypt(private_key_path: PathBuf, data_encoded: String) -> Result<String> {
    let private_key = get_private_key(private_key_path).await?;

    let data_encrypted = general_purpose::STANDARD
        .decode(data_encoded)
        .expect("failed to decode base64 data");

    let decrypted = private_key
        .decrypt(Pkcs1v15Encrypt, &data_encrypted)
        .expect("failed to decrypt");

    Ok(String::from_utf8(decrypted)?)
}
