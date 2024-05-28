use crate::api::package_service_client::PackageServiceClient;
use crate::api::{Package, PrepareRequest, Status};
use crate::store;
use std::fs;
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
    let source = Path::new(&package.source).canonicalize()?;
    let source_ignore_paths = package
        .ignore_paths
        .iter()
        .map(|i| Path::new(i).to_path_buf())
        .collect();
    let source_files = store::get_file_paths(&source, source_ignore_paths)?;
    let source_files_hashes = store::get_file_hashes(source_files.clone())?;
    let source_hash = store::get_source_hash(source_files_hashes.clone())?;

    let temp_dir = format!("{}-{}", package.name, source_hash);
    let temp_dir_path = Path::new(&format!("{}/{}", store::TEMP_DIR, temp_dir)).to_path_buf();

    fs::create_dir_all(&temp_dir_path)?;

    store::copy_files(source.clone(), temp_dir_path.clone(), source_files.clone())?;

    let source_tar = store::compress_source(source.clone(), temp_dir, source_files.clone())?;

    let mut client = PackageServiceClient::connect("http://[::1]:15323").await?;

    let request = tonic::Request::new(PrepareRequest {
        source_data: fs::read(format!("{}/{}", store::TEMP_DIR, source_tar.display()))?,
        source_hash,
        source_name: package.name,
    });

    let response = client.prepare(request).await?;

    let response_data = response.into_inner();

    println!("Source ID: {:?}", response_data.source_id);

    Ok(())
}
