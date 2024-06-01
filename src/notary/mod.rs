use crate::store;
use rand;
use rsa::pkcs8::LineEnding;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use rsa::{RsaPrivateKey, RsaPublicKey};
use std::error::Error;
use std::path::PathBuf;

#[cfg(feature = "pem")]
use rsa::pkcs8::LineEnding;

pub fn generate_keys(
    private_key_path: PathBuf,
    public_key_path: PathBuf,
) -> Result<(), Box<dyn Error>> {
    let mut rng = rand::thread_rng();

    let bits = 4096;
    let private_key = RsaPrivateKey::new(&mut rng, bits).expect("failed to generate a key");
    let private_key_der = private_key.to_pkcs8_der()?;
    private_key_der.write_pem_file(&private_key_path, "PRIVATE KEY", LineEnding::LF)?;

    let public_key = private_key.to_public_key();
    public_key.write_public_key_pem_file(&public_key_path, LineEnding::LF)?;

    println!("Generated signing certificates.");

    Ok(())
}

pub fn get_private_key() -> Result<RsaPrivateKey, Box<dyn std::error::Error>> {
    let key_data = std::fs::read(store::get_private_key_path())?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPrivateKey::from_pkcs8_pem(&key)?)
}

pub fn get_public_key() -> Result<RsaPublicKey, Box<dyn std::error::Error>> {
    let key_data = std::fs::read(store::get_public_key_path())?;
    let key = std::str::from_utf8(&key_data)?;
    Ok(RsaPublicKey::from_public_key_pem(key)?)
}

// pub fn load_signing_key(
//     signing_key_path: &PathBuf,
// ) -> Result<SigningKey<Sha256>, Box<dyn std::error::Error>> {
//     let signing_key_data = std::fs::read(signing_key_path)?;
//     let signing_key = SigningKey::<Sha256>::from_pkcs8_der(signing_key_data)?;
//     Ok(signing_key)
// }

// pub fn encrypt(
//     public_key: &PKey<openssl::pkey::Public>,
//     data: &[u8],
// ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
//     let rsa = public_key.rsa()?;
//     let mut buf: Vec<u8> = vec![0; rsa.size() as usize];
//     let encrypted_len = rsa.public_encrypt(data, &mut buf, openssl::rsa::Padding::PKCS1)?;
//     buf.truncate(encrypted_len);
//     Ok(buf)
// }

// pub fn decrypt(
//     private_key: &PKey<openssl::pkey::Private>,
//     encrypted_data: &[u8],
// ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
//     let rsa = private_key.rsa()?;
//     let mut buf: Vec<u8> = vec![0; rsa.size() as usize];
//     let decrypted_len =
//         rsa.private_decrypt(encrypted_data, &mut buf, openssl::rsa::Padding::PKCS1)?;
//     buf.truncate(decrypted_len);
//     Ok(buf)
// }
