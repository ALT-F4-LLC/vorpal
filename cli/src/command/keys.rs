use crate::command::store::{
    notary::generate_keys,
    paths::{get_root_key_dir_path, get_key_private_path, get_key_public_path},
};
use anyhow::{bail, Result};
use tracing::warn;

pub async fn generate() -> Result<()> {
    let key_dir_path = get_root_key_dir_path();
    let private_key_path = get_key_private_path();
    let public_key_path = get_key_public_path();

    if private_key_path.exists() && public_key_path.exists() {
        warn!("Keys already exist: {}", key_dir_path.display());

        return Ok(());
    }

    if private_key_path.exists() && !public_key_path.exists() {
        bail!("private key exists but public key is missing");
    }

    if !private_key_path.exists() && public_key_path.exists() {
        bail!("public key exists but private key is missing");
    }

    generate_keys(key_dir_path, private_key_path, public_key_path).await
}
