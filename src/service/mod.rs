use crate::api::package_service_server::PackageService;
use crate::api::{
    BuildRequest, BuildResponse, PrepareRequest, PrepareResponse, RetrieveRequest,
    RetrieveResponse, Status as BuildStatus, StatusRequest, StatusResponse,
};
use crate::store;
use anyhow::Result;
use dirs;
use flate2::read::GzDecoder;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use tar::Archive;
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct Packager {}

#[tonic::async_trait]
impl PackageService for Packager {
    async fn prepare(
        &self,
        request: Request<PrepareRequest>,
    ) -> Result<Response<PrepareResponse>, Status> {
        let message = request.into_inner();
        let store_home = dirs::home_dir().expect("Home directory not found");
        let store_dir = store_home.join(".vorpal/store");
        let source_dir = store_dir
            .join(&format!("{}-{}", message.source_name, message.source_hash))
            .with_extension("package")
            .to_path_buf();
        let source_tar = source_dir
            .join(source_dir.with_extension("source.tar.gz"))
            .to_path_buf();

        if !source_tar.exists() {
            match fs::write(&source_tar, message.source_data) {
                Ok(_) => println!("Source tar: {}", source_tar.display()),
                Err(e) => eprintln!("Failed tar: {}", e),
            }

            std::fs::create_dir_all(&source_dir)?;

            let tar_gz = File::open(source_tar)?;
            let buf_reader = BufReader::new(tar_gz);
            let gz_decoder = GzDecoder::new(buf_reader);
            let mut archive = Archive::new(gz_decoder);

            println!("Source dir: {}", source_dir.display());

            archive.unpack(&source_dir)?;
        }

        let source_files = match store::get_file_paths(&source_dir, vec![]) {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Failed to get source files: {}", e);
                return Err(Status::internal("Failed to get source files"));
            }
        };

        let source_files_hashes = match store::get_file_hashes(source_files) {
            Ok(hashes) => hashes,
            Err(e) => {
                eprintln!("Failed to get source files hashes: {}", e);
                return Err(Status::internal("Failed to get source files hashes"));
            }
        };

        let source_hash = match store::get_source_hash(source_files_hashes) {
            Ok(hash) => hash,
            Err(e) => {
                eprintln!("Failed to get source hash: {}", e);
                return Err(Status::internal("Failed to get source hash"));
            }
        };

        // TODO: decrypt `source_hash` with a signing key
        if source_hash != message.source_hash {
            return Err(Status::invalid_argument("Signing hash mismatch"));
        }

        let source_id = Uuid::now_v7();

        let response = PrepareResponse {
            source_id: source_id.to_string(),
        };

        Ok(Response::new(response))
    }

    async fn build(
        &self,
        request: Request<BuildRequest>,
    ) -> Result<Response<BuildResponse>, Status> {
        println!("[PackageBuild]: {:?}", request);

        let response = BuildResponse {
            build_id: "456".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        println!("[PackageStatus]: {:?}", request);

        let response = StatusResponse {
            logs: vec!["log1".to_string(), "log2".to_string()],
            status: BuildStatus::Created.into(),
        };

        Ok(Response::new(response))
    }

    async fn retrieve(
        &self,
        request: Request<RetrieveRequest>,
    ) -> Result<Response<RetrieveResponse>, Status> {
        println!("[PackageRetrieve]: {:?}", request);

        let response = RetrieveResponse { data: Vec::new() };

        Ok(Response::new(response))
    }
}
