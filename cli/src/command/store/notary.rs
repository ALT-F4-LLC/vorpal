use anyhow::{anyhow, bail, Context, Result};
use aws_lc_rs::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN},
    rsa::{
        OaepPrivateDecryptingKey, OaepPublicEncryptingKey, PrivateDecryptingKey,
        PublicEncryptingKey, OAEP_SHA256_MGF1SHA256,
    },
};
use base64::{engine::general_purpose, Engine};
use rsa::{
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey},
    rand_core::{self, RngCore},
    Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey,
};
use std::path::Path;
use tokio::fs::read;

/// Prefix of every sealed secret. Its NUL byte is outside the base64 alphabet, so
/// the base64 text of an older RSA PKCS#1 v1.5 value can never carry it.
const SEALED_MARKER: &[u8; 8] = b"\0vorpal1";

/// Largest secret `seal` accepts, in bytes.
const SEALED_SECRET_LIMIT: usize = 65_536;

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

/// Seals `secret` for the holder of the private half of `public_key`.
///
/// A fresh AES-256-GCM key encrypts the secret and RSA-OAEP-SHA256 wraps that key.
/// The sealed value is `marker || wrapped key || nonce || ciphertext || tag`, and
/// the tag authenticates every byte before it together with `associated_data`,
/// which `open` must receive unchanged.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the agent switches from `encrypt` to `seal` in the wiring change"
    )
)]
pub fn seal(public_key: &RsaPublicKey, secret: &[u8], associated_data: &[u8]) -> Result<Vec<u8>> {
    if secret.len() > SEALED_SECRET_LIMIT {
        bail!(
            "secret is {} bytes, above the limit of {SEALED_SECRET_LIMIT} bytes",
            secret.len()
        );
    }

    let public_key_der = public_key
        .to_public_key_der()
        .context("failed to encode public key")?;
    let wrapping_key = PublicEncryptingKey::from_der(public_key_der.as_bytes())
        .context("failed to load public key")?;
    let wrapping_key =
        OaepPublicEncryptingKey::new(wrapping_key).context("failed to load public key")?;

    let mut content_key = [0u8; 32];
    rand_core::OsRng
        .try_fill_bytes(&mut content_key)
        .context("failed to generate secret key")?;
    let mut nonce = [0u8; NONCE_LEN];
    rand_core::OsRng
        .try_fill_bytes(&mut nonce)
        .context("failed to generate nonce")?;

    let mut wrapped_key = vec![0; wrapping_key.ciphertext_size()];
    let wrapped_key = wrapping_key
        .encrypt(
            &OAEP_SHA256_MGF1SHA256,
            &content_key,
            &mut wrapped_key,
            None,
        )
        .context("failed to wrap secret key")?;

    let mut sealed = [SEALED_MARKER.as_slice(), wrapped_key, &nonce].concat();
    let aead_associated_data = [sealed.as_slice(), associated_data].concat();

    let content_key = UnboundKey::new(&AES_256_GCM, &content_key)
        .map(LessSafeKey::new)
        .context("failed to load secret key")?;
    let mut ciphertext = secret.to_vec();
    content_key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aead_associated_data),
            &mut ciphertext,
        )
        .context("failed to seal secret")?;

    sealed.append(&mut ciphertext);

    Ok(sealed)
}

/// Opens a value produced by `seal` with the private half of its key pair and the
/// same `associated_data`, and returns the secret bytes.
///
/// A value without the sealed marker is refused before any key is touched, with an
/// error telling the caller to prepare the secret again. Every other failure,
/// whether truncation, a wrong key, tampering, or different associated data,
/// returns the same error so its text reveals nothing about which check failed.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the worker switches from `decrypt` to `open` in the wiring change"
    )
)]
pub fn open(private_key: &RsaPrivateKey, sealed: &[u8], associated_data: &[u8]) -> Result<Vec<u8>> {
    let Some(after_marker) = sealed.strip_prefix(SEALED_MARKER.as_slice()) else {
        bail!("secret was encrypted with an older format and must be prepared again");
    };

    let private_key_der = private_key
        .to_pkcs8_der()
        .context("failed to encode private key")?;
    let unwrapping_key = PrivateDecryptingKey::from_pkcs8(private_key_der.as_bytes())
        .context("failed to load private key")?;
    let unwrapping_key =
        OaepPrivateDecryptingKey::new(unwrapping_key).context("failed to load private key")?;

    let open_failed = || anyhow!("failed to open sealed secret");

    let (wrapped_key, after_wrapped_key) = after_marker
        .split_at_checked(unwrapping_key.key_size_bytes())
        .ok_or_else(open_failed)?;
    let (nonce, ciphertext_and_tag) = after_wrapped_key
        .split_at_checked(NONCE_LEN)
        .ok_or_else(open_failed)?;
    let header_len = sealed.len() - ciphertext_and_tag.len();
    let aead_associated_data = [&sealed[..header_len], associated_data].concat();

    let mut content_key = vec![0; unwrapping_key.min_output_size()];
    let content_key = unwrapping_key
        .decrypt(&OAEP_SHA256_MGF1SHA256, wrapped_key, &mut content_key, None)
        .map_err(|_| open_failed())?;
    let content_key = UnboundKey::new(&AES_256_GCM, content_key)
        .map(LessSafeKey::new)
        .map_err(|_| open_failed())?;
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| open_failed())?;

    let mut secret = ciphertext_and_tag.to_vec();
    let secret_len = content_key
        .open_in_place(nonce, Aad::from(aead_associated_data), &mut secret)
        .map_err(|_| open_failed())?
        .len();
    secret.truncate(secret_len);

    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::{open, seal};
    use anyhow::Result;
    use rcgen::{KeyPair, PKCS_RSA_SHA256};
    use rsa::{
        pkcs8::{DecodePrivateKey, DecodePublicKey},
        RsaPrivateKey, RsaPublicKey,
    };

    /// Generates a service key pair the way `system keys generate` does and parses
    /// its PEM through the same decoders as `get_public_key` and `get_private_key`.
    fn service_keys() -> Result<(RsaPublicKey, RsaPrivateKey)> {
        let key_pair = KeyPair::generate_for(&PKCS_RSA_SHA256)?;
        let public_key = RsaPublicKey::from_public_key_pem(&key_pair.public_key_pem())?;
        let private_key = RsaPrivateKey::from_pkcs8_pem(&key_pair.serialize_pem())?;
        Ok((public_key, private_key))
    }

    #[test]
    fn round_trips_secret_at_size_limit() -> Result<()> {
        let (public_key, private_key) = service_keys()?;
        let secret = vec![0x5a; 65536];

        let sealed = seal(&public_key, &secret, b"context")?;

        assert_eq!(open(&private_key, &sealed, b"context")?, secret);
        Ok(())
    }

    mod abuse {
        use super::service_keys;
        use crate::command::store::notary::{open, seal};
        use anyhow::{bail, Result};
        use base64::{engine::general_purpose, Engine};
        use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
        use rsa::{rand_core::OsRng, Pkcs1v15Encrypt, RsaPrivateKey};
        use std::collections::BTreeSet;

        const OLDER_FORMAT: &str =
            "secret was encrypted with an older format and must be prepared again";
        const MARKER_LEN: usize = 8;
        const RSA_2048_WRAPPED_KEY_LEN: usize = 256;
        const TAG_LEN: usize = 16;

        /// Returns the error text of a failed `open`, or an error when `open` succeeds.
        fn open_failure(
            private_key: &RsaPrivateKey,
            sealed: &[u8],
            associated_data: &[u8],
        ) -> Result<String> {
            match open(private_key, sealed, associated_data) {
                Ok(_) => bail!("open accepted a value it must reject"),
                Err(error) => Ok(error.to_string()),
            }
        }

        #[test]
        fn round_trips_binary_secret_above_rsa_limit() -> Result<()> {
            let (public_key, private_key) = service_keys()?;
            let secret: Vec<u8> = (0..=u8::MAX).cycle().take(1024).collect();
            assert!(secret.contains(&b'=') && secret.contains(&b'\n'));
            assert!(std::str::from_utf8(&secret).is_err());

            let sealed = seal(&public_key, &secret, b"context")?;

            assert_eq!(open(&private_key, &sealed, b"context")?, secret);
            Ok(())
        }

        #[test]
        fn round_trips_generated_certificate_chain_and_service_account() -> Result<()> {
            let (public_key, private_key) = service_keys()?;

            let ca_key = KeyPair::generate()?;
            let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
            ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            let ca_cert = ca_params.self_signed(&ca_key)?;
            let ca_issuer = Issuer::new(ca_params, ca_key);
            let leaf_key = KeyPair::generate()?;
            let leaf_cert = CertificateParams::new(vec!["localhost".to_string()])?
                .signed_by(&leaf_key, &ca_issuer)?;
            let chain_with_key = format!(
                "{}{}{}",
                leaf_cert.pem(),
                ca_cert.pem(),
                leaf_key.serialize_pem()
            );

            let service_account = serde_json::json!({
                "type": "service_account",
                "project_id": "synthetic-project",
                "private_key_id": "0123456789abcdef",
                "private_key": leaf_key.serialize_pem(),
                "client_email": "synthetic@synthetic-project.example.com",
            })
            .to_string();

            for secret in [chain_with_key, service_account] {
                assert!(secret.len() > 245);
                let sealed = seal(&public_key, secret.as_bytes(), b"context")?;
                assert_eq!(open(&private_key, &sealed, b"context")?, secret.as_bytes());
            }
            Ok(())
        }

        #[test]
        fn seal_rejects_secret_above_size_limit() -> Result<()> {
            let (public_key, _) = service_keys()?;

            let message = match seal(&public_key, &vec![0x5a; 65537], b"context") {
                Ok(_) => bail!("seal accepted a 65537-byte secret"),
                Err(error) => error.to_string(),
            };

            assert!(message.contains("65536 bytes"), "{message}");
            Ok(())
        }

        #[test]
        fn open_rejects_pkcs1v15_ciphertext_as_older_format() -> Result<()> {
            let (public_key, private_key) = service_keys()?;
            let block = public_key.encrypt(&mut OsRng, Pkcs1v15Encrypt, b"legacy secret")?;
            let text = general_purpose::STANDARD.encode(&block);

            assert_eq!(
                open_failure(&private_key, &block, b"context")?,
                OLDER_FORMAT
            );
            assert_eq!(
                open_failure(&private_key, text.as_bytes(), b"context")?,
                OLDER_FORMAT
            );
            Ok(())
        }

        #[test]
        fn open_rejects_truncated_sealed_value() -> Result<()> {
            let (public_key, private_key) = service_keys()?;
            let sealed = seal(&public_key, b"truncated secret", b"context")?;

            let after_marker = MARKER_LEN;
            let inside_wrapped_key = MARKER_LEN + RSA_2048_WRAPPED_KEY_LEN / 2;
            let inside_tag = sealed.len() - TAG_LEN / 2;

            for len in [after_marker, inside_wrapped_key, inside_tag] {
                open_failure(&private_key, &sealed[..len], b"context")?;
            }
            Ok(())
        }

        #[test]
        fn open_rejects_every_single_byte_change() -> Result<()> {
            let (public_key, private_key) = service_keys()?;
            let sealed = seal(&public_key, b"tampered secret", b"context")?;
            let mut post_marker_failures = BTreeSet::new();

            for index in 0..sealed.len() {
                let mut tampered = sealed.clone();
                tampered[index] ^= 0x01;
                let message = open_failure(&private_key, &tampered, b"context")?;
                if index >= MARKER_LEN {
                    post_marker_failures.insert(message);
                }
            }

            assert_eq!(
                post_marker_failures.len(),
                1,
                "a change after the marker must fail with one error text: {post_marker_failures:?}"
            );
            Ok(())
        }

        #[test]
        fn open_rejects_different_associated_data() -> Result<()> {
            let (public_key, private_key) = service_keys()?;
            let sealed = seal(&public_key, b"bound secret", b"namespace/name")?;

            open_failure(&private_key, &sealed, b"namespace/other")?;
            open_failure(&private_key, &sealed, b"")?;
            Ok(())
        }
    }
}
