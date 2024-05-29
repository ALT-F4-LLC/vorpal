use crate::api::package_service_client::PackageServiceClient;
use crate::api::{Package, PrepareRequest, Status};
use crate::store;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use uuid::Uuid;

pub struct Source {
    pub hash: String,
    pub id: Uuid,
    pub name: String,
    pub uri: String,
}

pub struct Build {
    pub build_phase: String,
    pub error: Option<String>, // populated on failed
    pub id: Uuid,
    pub ignore_paths: Vec<String>,
    pub install_phase: String,
    pub package_id: Option<Uuid>, // populated on completed
    pub source_id: Uuid,
    pub status: Status,
}

pub async fn run(package: Package) -> Result<(), anyhow::Error> {
    println!("Preparing: {:?}", package.name);

    prepare(package).await
}

async fn prepare(package: Package) -> Result<(), anyhow::Error> {
    let store_home = dirs::home_dir().expect("Home directory not found");
    let store_dir = store_home.join(".vorpal/package");

    let source = Path::new(&package.source).canonicalize()?;
    let source_ignore_paths = package
        .ignore_paths
        .iter()
        .map(|i| Path::new(i).to_path_buf())
        .collect();
    let source_files = store::get_file_paths(&source, source_ignore_paths)?;
    let source_files_hashes = store::get_file_hashes(source_files.clone())?;

    // TODO: enrypt `source_hash` with a signing key
    let source_hash = store::get_source_hash(source_files_hashes.clone())?;
    let source_dir_name = store::get_package_dir_name(&package.name, &source_hash);
    let source_dir = store_dir.join(&source_dir_name).with_extension("package");
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

    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;

    let request = tonic::Request::new(PrepareRequest {
        source_data: fs::read(source_tar)?,
        source_hash,
        source_name: package.name,
    });

    let response = client.prepare(request).await?;

    let response_data = response.into_inner();

    println!("Source ID: {:?}", response_data.source_id);

    Ok(())
}
