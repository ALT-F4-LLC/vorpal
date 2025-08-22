use anyhow::Result;
use base64::{engine::general_purpose, Engine};
use rsa::{
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    rand_core, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey,
};
use std::path::PathBuf;
use tokio::fs::read;

pub async fn get_private_key(private_key_path: PathBuf) -> Result<RsaPrivateKey> {
    let key_data = read(private_key_path)
        .await
        .expect("failed to read private key");

    let key = std::str::from_utf8(&key_data).expect("failed to convert key to string");

    Ok(RsaPrivateKey::from_pkcs8_pem(key).expect("failed to parse private key"))
}

pub async fn get_public_key(public_key_path: PathBuf) -> Result<RsaPublicKey> {
    let key_data = read(public_key_path)
        .await
        .expect("failed to read public key");

    let key = std::str::from_utf8(&key_data).expect("failed to convert key to string");

    Ok(RsaPublicKey::from_public_key_pem(key).expect("failed to parse public key"))
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
