use crate::store;
use rand;
use rsa::pkcs8::LineEnding;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use rsa::{RsaPrivateKey, RsaPublicKey};
use tokio::fs;

pub fn generate_keys() -> Result<(), anyhow::Error> {
    let mut rng = rand::thread_rng();

    let bits = 2048;
    let private_key_path = store::get_private_key_path();
    let private_key = RsaPrivateKey::new(&mut rng, bits)?;
    let private_key_der = private_key.to_pkcs8_der()?;
    private_key_der.write_pem_file(&private_key_path, "PRIVATE KEY", LineEnding::LF)?;

    println!("Private key generated: {}", private_key_path.display());

    let public_key_path = store::get_public_key_path();
    let public_key = private_key.to_public_key();
    public_key.write_public_key_pem_file(&public_key_path, LineEnding::LF)?;

    println!("Public key generated: {}", public_key_path.display());

    Ok(())
}

pub async fn get_private_key() -> Result<RsaPrivateKey, anyhow::Error> {
    let key_data = fs::read(store::get_private_key_path()).await?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPrivateKey::from_pkcs8_pem(&key)?)
}

pub async fn get_public_key() -> Result<RsaPublicKey, anyhow::Error> {
    let key_data = fs::read(store::get_public_key_path()).await?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPublicKey::from_public_key_pem(key)?)
}
