use crate::store;
use rand;
use rand::rngs::OsRng;
use rsa::pkcs8::LineEnding;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use rsa::pss::{Signature, SigningKey};
use rsa::sha2::Sha256;
use rsa::signature::RandomizedSigner;
use rsa::{RsaPrivateKey, RsaPublicKey};
use tokio::fs;
use tracing::debug;

const BITS: usize = 2048;

pub fn generate_keys() -> Result<(), anyhow::Error> {
    let mut rng = rand::thread_rng();

    let private_key_path = store::get_private_key_path();
    let private_key = RsaPrivateKey::new(&mut rng, BITS)?;
    let private_key_der = private_key.to_pkcs8_der()?;
    private_key_der.write_pem_file(&private_key_path, "PRIVATE KEY", LineEnding::LF)?;

    debug!("private key generated: {:?}", private_key_path);

    let public_key_path = store::get_public_key_path();
    let public_key = private_key.to_public_key();
    public_key.write_public_key_pem_file(&public_key_path, LineEnding::LF)?;

    debug!("public key generated: {:?}", public_key_path);

    Ok(())
}

pub async fn get_private_key() -> Result<RsaPrivateKey, anyhow::Error> {
    let key_data = fs::read(store::get_private_key_path()).await?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPrivateKey::from_pkcs8_pem(key)?)
}

pub async fn get_public_key() -> Result<RsaPublicKey, anyhow::Error> {
    let key_data = fs::read(store::get_public_key_path()).await?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPublicKey::from_public_key_pem(key)?)
}

pub async fn sign(source_data: &[u8]) -> Result<Signature, anyhow::Error> {
    let private_key = get_private_key().await?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let mut signing_rng = OsRng;
    Ok(signing_key.sign_with_rng(&mut signing_rng, source_data))
}
