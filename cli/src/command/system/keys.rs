use crate::command::store::paths::{
    get_key_ca_key_path, get_key_service_key_path, get_key_service_path,
    get_key_service_public_path, get_key_service_secret_path, get_root_key_dir_path,
};
use anyhow::{Context, Result};
use rcgen::{
    CertificateParams, DnType, DnValue::PrintableString, ExtendedKeyUsagePurpose, IsCa, Issuer,
    KeyPair, KeyUsagePurpose, PKCS_RSA_SHA256,
};
use std::path::Path;
use tokio::fs::{create_dir_all, read_to_string, write};
use tracing::info;
use uuid::Uuid;
use vorpal_sdk::context::get_key_ca_path;

/// Generates the CA private key at `ca_key_path`, if it does not already exist.
async fn generate_ca_key(ca_key_path: &Path) -> Result<()> {
    if ca_key_path.exists() {
        return Ok(());
    }

    let key_pair =
        KeyPair::generate_for(&PKCS_RSA_SHA256).context("failed to generate CA keypair")?;
    let key_pair_pem = key_pair.serialize_pem();

    info!("Generating new CA keypair");

    write(ca_key_path, key_pair_pem)
        .await
        .context("failed to write CA private key to file")?;

    Ok(())
}

/// Generates the self-signed CA certificate at `ca_path` from the key at `ca_key_path`,
/// if the certificate does not already exist.
async fn generate_ca_cert(ca_path: &Path, ca_key_path: &Path) -> Result<()> {
    if ca_path.exists() {
        return Ok(());
    }

    let mut cert_params =
        CertificateParams::new(vec![]).context("failed to create CA certificate params")?;

    cert_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);

    cert_params.distinguished_name.push(
        DnType::CountryName,
        PrintableString(
            "US".try_into()
                .context("failed to encode CA country name")?,
        ),
    );

    cert_params
        .distinguished_name
        .push(DnType::OrganizationName, "Vorpal");

    cert_params
        .key_usages
        .push(KeyUsagePurpose::DigitalSignature);
    cert_params.key_usages.push(KeyUsagePurpose::KeyCertSign);
    cert_params.key_usages.push(KeyUsagePurpose::CrlSign);

    let key_pair_data = read_to_string(ca_key_path).await?;
    let key_pair = KeyPair::from_pem(&key_pair_data).context("failed to parse CA private key")?;

    let cert = cert_params
        .self_signed(&key_pair)
        .context("failed to self-sign CA certificate")?;
    let cert_pem = cert.pem();

    info!("Generating new CA certificate");

    write(ca_path, cert_pem)
        .await
        .context("failed to write CA certificate to file")?;

    Ok(())
}

/// Generates the service private key at `service_key_path`, if it does not already exist.
async fn generate_service_key(service_key_path: &Path) -> Result<()> {
    if service_key_path.exists() {
        return Ok(());
    }

    let key_pair =
        KeyPair::generate_for(&PKCS_RSA_SHA256).context("failed to generate service keypair")?;
    let key_pair_pem = key_pair.serialize_pem();

    info!("Generating new service keypair");

    write(service_key_path, key_pair_pem)
        .await
        .context("failed to write service private key to file")?;

    Ok(())
}

/// Generates the service public key at `service_public_path` from the key at
/// `service_key_path`, if the public key does not already exist.
async fn generate_service_public_key(
    service_public_path: &Path,
    service_key_path: &Path,
) -> Result<()> {
    if service_public_path.exists() {
        return Ok(());
    }

    let key_pair_data = read_to_string(service_key_path).await?;
    let key_pair =
        KeyPair::from_pem(&key_pair_data).context("failed to parse service private key")?;
    let key_pair_pem = key_pair.public_key_pem();

    info!("Generating new service public keypair");

    write(service_public_path, key_pair_pem)
        .await
        .context("failed to write service public key to file")?;

    Ok(())
}

/// Generates the service certificate at `service_path`, signed by the CA at `ca_path`
/// (using `ca_key_path`) with the service key at `service_key_path`, if the certificate
/// does not already exist.
async fn generate_service_cert(
    service_path: &Path,
    service_key_path: &Path,
    ca_path: &Path,
    ca_key_path: &Path,
) -> Result<()> {
    if service_path.exists() {
        return Ok(());
    }

    let ca_data = read_to_string(ca_path).await?;
    let ca_key_data = read_to_string(ca_key_path).await?;
    let ca_key = KeyPair::from_pem(&ca_key_data).context("failed to parse CA private key")?;
    let ca_issuer = Issuer::from_ca_cert_pem(&ca_data, ca_key)?;

    let name = "localhost";

    let mut params = CertificateParams::new(vec![name.into()])
        .context("failed to create service certificate params")?;

    params.distinguished_name.push(DnType::CommonName, name);

    params.use_authority_key_identifier_extension = true;

    params.key_usages.push(KeyUsagePurpose::DigitalSignature);

    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);

    let key_pair_data = read_to_string(service_key_path).await?;
    let key_pair =
        KeyPair::from_pem(&key_pair_data).context("failed to parse service private key")?;

    let cert = params
        .signed_by(&key_pair, &ca_issuer)
        .context("failed to sign service certificate")?;
    let cert_pem = cert.pem();

    info!("Generating new service certificate");

    write(service_path, cert_pem)
        .await
        .context("failed to write service certificate to file")?;

    Ok(())
}

/// Generates the service secret at `service_secret_path`, if it does not already exist.
async fn generate_service_secret(service_secret_path: &Path) -> Result<()> {
    if service_secret_path.exists() {
        return Ok(());
    }

    let secret = Uuid::now_v7().to_string();

    info!("Generating new service secret");

    write(service_secret_path, secret)
        .await
        .context("failed to write service secret to file")?;

    Ok(())
}

/// Generates the CA keypair/certificate, service keypair/certificate, and service
/// secret under the root key directory, if they do not already exist.
pub async fn generate() -> Result<()> {
    let key_dir_path = get_root_key_dir_path();

    if !key_dir_path.exists() {
        create_dir_all(&key_dir_path)
            .await
            .context("failed to create key directory")?;
    }

    let ca_key_path = get_key_ca_key_path();
    generate_ca_key(&ca_key_path).await?;

    let ca_path = get_key_ca_path();
    generate_ca_cert(&ca_path, &ca_key_path).await?;

    let service_key_path = get_key_service_key_path();
    generate_service_key(&service_key_path).await?;

    let service_public_path = get_key_service_public_path();
    generate_service_public_key(&service_public_path, &service_key_path).await?;

    let service_path = get_key_service_path();
    generate_service_cert(&service_path, &service_key_path, &ca_path, &ca_key_path).await?;

    let service_secret_path = get_key_service_secret_path();
    generate_service_secret(&service_secret_path).await?;

    Ok(())
}
