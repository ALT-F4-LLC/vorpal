use rand::rngs::OsRng;
use rsa::pkcs8::LineEnding;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use rsa::pss::{Signature, SigningKey};
use rsa::sha2::Sha256;
use rsa::signature::RandomizedSigner;
use rsa::{RsaPrivateKey, RsaPublicKey};
use std::path::PathBuf;
use tokio::fs;
use tokio::fs::create_dir_all;
use tracing::info;

const BITS: usize = 2048;

pub async fn generate_keys(
    key_path: PathBuf,
    private_key_path: PathBuf,
    public_key_path: PathBuf,
) -> Result<(), anyhow::Error> {
    if private_key_path.exists() {
        info!("skipping - private key already exists");
        return Ok(());
    }

    if public_key_path.exists() {
        info!("skipping - public key already exists");
        return Ok(());
    }

    create_dir_all(key_path.clone()).await?;

    info!("key directory: {:?}", key_path);

    let mut rng = rand::thread_rng();

    let private_key = RsaPrivateKey::new(&mut rng, BITS)?;
    let private_key_der = private_key.to_pkcs8_der()?;
    private_key_der.write_pem_file(&private_key_path, "PRIVATE KEY", LineEnding::LF)?;

    info!("private key generated: {:?}", private_key_path);

    let public_key = private_key.to_public_key();
    public_key.write_public_key_pem_file(&public_key_path, LineEnding::LF)?;

    info!("public key generated: {:?}", public_key_path);

    Ok(())
}

pub async fn get_private_key(private_key_path: PathBuf) -> Result<RsaPrivateKey, anyhow::Error> {
    let key_data = fs::read(private_key_path).await?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPrivateKey::from_pkcs8_pem(key)?)
}

pub async fn get_public_key(public_key_path: PathBuf) -> Result<RsaPublicKey, anyhow::Error> {
    let key_data = fs::read(public_key_path).await?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPublicKey::from_public_key_pem(key)?)
}

pub async fn sign(
    private_key_path: PathBuf,
    source_data: &[u8],
) -> Result<Signature, anyhow::Error> {
    let private_key = get_private_key(private_key_path).await?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let mut signing_rng = OsRng;
    Ok(signing_key.sign_with_rng(&mut signing_rng, source_data))
}
