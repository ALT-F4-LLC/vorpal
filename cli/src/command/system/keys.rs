use crate::command::store::paths::{
    get_key_ca_key_path, get_key_ca_path, get_key_service_key_path, get_key_service_path,
    get_key_service_public_path, get_key_service_secret_path, get_root_key_dir_path,
};
use anyhow::Result;
use rcgen::{
    CertificateParams, DnType, DnValue::PrintableString, ExtendedKeyUsagePurpose, IsCa, Issuer,
    KeyPair, KeyUsagePurpose, PKCS_RSA_SHA256,
};
use tokio::fs::{create_dir_all, read_to_string, write};
use tracing::info;
use uuid::Uuid;

pub async fn generate() -> Result<()> {
    let key_dir_path = get_root_key_dir_path();

    if !key_dir_path.exists() {
        create_dir_all(key_dir_path.clone())
            .await
            .expect("failed to create key directory");
    }

    let ca_key_path = get_key_ca_key_path();

    if !ca_key_path.exists() {
        let key_pair = KeyPair::generate_for(&PKCS_RSA_SHA256).unwrap();
        let key_pair_pem = key_pair.serialize_pem();

        info!("Generating new CA keypair");

        write(ca_key_path.clone(), key_pair_pem)
            .await
            .expect("failed to write CA private key to file");
    }

    let ca_path = get_key_ca_path();

    if !ca_path.exists() {
        let mut cert_params = CertificateParams::new(vec![]).unwrap();

        cert_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);

        cert_params.distinguished_name.push(
            DnType::CountryName,
            PrintableString("US".try_into().unwrap()),
        );

        cert_params
            .distinguished_name
            .push(DnType::OrganizationName, "Vorpal");

        cert_params
            .key_usages
            .push(KeyUsagePurpose::DigitalSignature);
        cert_params.key_usages.push(KeyUsagePurpose::KeyCertSign);
        cert_params.key_usages.push(KeyUsagePurpose::CrlSign);

        let key_pair_data = read_to_string(ca_key_path.clone()).await?;
        let key_pair = KeyPair::from_pem(&key_pair_data).unwrap();

        let cert = cert_params.self_signed(&key_pair).unwrap();
        let cert_pem = cert.pem();

        info!("Generating new CA certificate");

        write(ca_path.clone(), cert_pem)
            .await
            .expect("failed to write CA certificate to file");
    }

    let service_key_path = get_key_service_key_path();

    if !service_key_path.exists() {
        let key_pair = KeyPair::generate_for(&PKCS_RSA_SHA256).unwrap();
        let key_pair_pem = key_pair.serialize_pem();

        info!("Generating new service keypair");

        write(service_key_path.clone(), key_pair_pem)
            .await
            .expect("failed to write CA private key to file");
    }

    let service_public_path = get_key_service_public_path();

    if !service_public_path.exists() {
        let key_pair_data = read_to_string(service_key_path.clone()).await?;
        let key_pair = KeyPair::from_pem(&key_pair_data).unwrap();
        let key_pair_pem = key_pair.public_key_pem();

        info!("Generating new service public keypair");

        write(service_public_path.clone(), key_pair_pem)
            .await
            .expect("failed to write CA private key to file");
    }

    let service_path = get_key_service_path();

    if !service_path.exists() {
        let ca_data = read_to_string(ca_path.clone()).await?;
        let ca_key_data = read_to_string(ca_key_path).await?;
        let ca_key = KeyPair::from_pem(&ca_key_data).unwrap();
        let ca_issuer = Issuer::from_ca_cert_pem(&ca_data, ca_key)?;

        let name = "localhost";

        let mut params =
            CertificateParams::new(vec![name.into()]).expect("we know the name is valid");

        params.distinguished_name.push(DnType::CommonName, name);

        params.use_authority_key_identifier_extension = true;

        params.key_usages.push(KeyUsagePurpose::DigitalSignature);

        params
            .extended_key_usages
            .push(ExtendedKeyUsagePurpose::ServerAuth);

        let key_pair_data = read_to_string(service_key_path.clone()).await?;
        let key_pair = KeyPair::from_pem(&key_pair_data).unwrap();

        let cert = params.signed_by(&key_pair, &ca_issuer).unwrap();
        let cert_pem = cert.pem();

        info!("Generating new service certificate");

        write(service_path.clone(), cert_pem)
            .await
            .expect("failed to write service certificate to file");
    }

    let service_secret_path = get_key_service_secret_path();

    if !service_secret_path.exists() {
        let secret = Uuid::now_v7().to_string();

        info!("Generating new service secret");

        write(service_secret_path.clone(), secret)
            .await
            .expect("failed to write service secret to file");
    }

    Ok(())
}
