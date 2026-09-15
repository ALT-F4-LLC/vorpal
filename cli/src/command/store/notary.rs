use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine};
use rsa::{
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    rand_core, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey,
};
use std::path::Path;
use tokio::fs::read;

/// Loads and parses an RSA private key in PKCS#8 PEM format from `private_key_path`.
pub async fn get_private_key(private_key_path: &Path) -> Result<RsaPrivateKey> {
    let key_data = read(private_key_path)
        .await
        .context("failed to read private key")?;

    let key = std::str::from_utf8(&key_data).context("failed to convert key to string")?;

    RsaPrivateKey::from_pkcs8_pem(key).context("failed to parse private key")
}

/// Loads and parses an RSA public key in PEM format from `public_key_path`.
pub async fn get_public_key(public_key_path: &Path) -> Result<RsaPublicKey> {
    let key_data = read(public_key_path)
        .await
        .context("failed to read public key")?;

    let key = std::str::from_utf8(&key_data).context("failed to convert key to string")?;

    RsaPublicKey::from_public_key_pem(key).context("failed to parse public key")
}

/// Encrypts `data` with the RSA public key at `public_key_path` and returns the
/// ciphertext, base64-encoded.
pub async fn encrypt(public_key_path: &Path, data: String) -> Result<String> {
    let public_key = get_public_key(public_key_path).await?;

    let mut rng = rand_core::OsRng;

    let data_encrypted = public_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, data.as_bytes())
        .context("failed to encrypt")?;

    let data_encoded = general_purpose::STANDARD.encode(&data_encrypted);

    Ok(data_encoded)
}

/// Decrypts base64-encoded ciphertext `data_encoded` with the RSA private key at
/// `private_key_path` and returns the plaintext.
pub async fn decrypt(private_key_path: &Path, data_encoded: String) -> Result<String> {
    let private_key = get_private_key(private_key_path).await?;

    let data_encrypted = general_purpose::STANDARD
        .decode(data_encoded)
        .context("failed to decode base64 data")?;

    let decrypted = private_key
        .decrypt(Pkcs1v15Encrypt, &data_encrypted)
        .context("failed to decrypt")?;

    Ok(String::from_utf8(decrypted)?)
}
