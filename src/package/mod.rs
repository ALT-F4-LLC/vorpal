use crate::api::package_service_client::PackageServiceClient;
use crate::api::{Package, PrepareRequest};
use crate::notary;
use crate::store;
use rsa::pss::SigningKey;
use rsa::sha2::Sha256;
use rsa::signature::RandomizedSigner;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub async fn run(package: Package) -> Result<(), anyhow::Error> {
    println!("Preparing: {:?}", package.name);

    prepare(package).await

    // TODO: build method

    // TODO status method

    // TODO: retrieve method
}

async fn prepare(package: Package) -> Result<(), anyhow::Error> {
    let vorpal_dir = store::get_home_dir();
    let package_dir = vorpal_dir.join("package");
    let source = Path::new(&package.source).canonicalize()?;
    let source_ignore_paths = package
        .ignore_paths
        .iter()
        .map(|i| Path::new(i).to_path_buf())
        .collect();
    let source_files = store::get_file_paths(&source, source_ignore_paths)?;
    let source_files_hashes = store::get_file_hashes(source_files.clone())?;
    let source_hash = store::get_source_hash(source_files_hashes.clone())?;

    let source_dir_name = store::get_package_dir_name(&package.name, &source_hash);
    let source_dir = package_dir.join(&source_dir_name).with_extension("package");
    let source_tar = Path::new(&source_dir).with_extension("tar.gz");

    // Skip if source exists
    if !source_dir.exists() {
        println!("Preparing source: {:?}", source_dir);
        fs::create_dir_all(&source_dir)?;
        store::copy_files(source.clone(), source_dir.clone(), source_files.clone())?;

        let store_files = store::get_file_paths(&source_dir, vec![])?;

        store::set_files_permissions(&store_files)?;
        fs::set_permissions(&source_dir, fs::Permissions::from_mode(0o555))?;
    }

    if !source_tar.exists() {
        println!("Creating source tar: {:?}", source_tar);
        store::compress_files(source.clone(), source_tar.clone(), source_files.clone())?;
        fs::set_permissions(&source_tar, fs::Permissions::from_mode(0o444))?;
    }

    let source_tar_bytes = fs::read(&source_tar)?;

    println!("Source tar: {:?}", source_tar);

    let private_key = match notary::get_private_key() {
        Ok(key) => key,
        Err(e) => anyhow::bail!("Failed to get private key: {:?}", e),
    };

    let signing_key = SigningKey::<Sha256>::new(private_key);

    let mut rng = rand::thread_rng();

    let source_signature = signing_key.sign_with_rng(&mut rng, &source_tar_bytes);

    println!("Source signature: {:?}", source_signature);

    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;

    let request = tonic::Request::new(PrepareRequest {
        source_data: fs::read(source_tar)?,
        source_hash,
        source_name: package.name,
        source_signature: source_signature.to_string(),
    });

    let response = client.prepare(request).await?;
    let response_data = response.into_inner();

    println!("Source ID: {:?}", response_data.source_id);

    Ok(())
}
