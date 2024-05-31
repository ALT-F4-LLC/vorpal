use crate::api::package_service_server::PackageService;
use crate::api::{
    BuildRequest, BuildResponse, PrepareRequest, PrepareResponse, RetrieveRequest,
    RetrieveResponse, Status as BuildStatus, StatusRequest, StatusResponse,
};
use crate::database;
use crate::notary;
use crate::store;
use anyhow::Result;
use flate2::read::GzDecoder;
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use tar::Archive;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct Packager {}

#[tonic::async_trait]
impl PackageService for Packager {
    async fn prepare(
        &self,
        request: Request<PrepareRequest>,
    ) -> Result<Response<PrepareResponse>, Status> {
        let message = request.into_inner();
        let store_dir = store::get_store_dir();
        let source_dir = store_dir
            .join(&format!("{}-{}", message.source_name, message.source_hash))
            .with_extension("package")
            .to_path_buf();
        let source_tar = source_dir
            .join(source_dir.with_extension("source.tar.gz"))
            .to_path_buf();
        let public_key = match notary::get_public_key() {
            Ok(key) => key,
            Err(e) => {
                eprintln!("Failed to get public key: {:?}", e);
                return Err(Status::internal("Failed to get public key"));
            }
        };
        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        let source_signature = match Signature::try_from(message.source_signature.as_bytes()) {
            Ok(signature) => signature,
            Err(e) => {
                eprintln!("Failed to decode signature: {:?}", e);
                return Err(Status::internal("Failed to decode signature"));
            }
        };

        match verifying_key.verify(&message.source_data, &source_signature) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to verify signature: {:?}", e);
                return Err(Status::internal("Failed to verify signature"));
            }
        };

        let db_path = store::get_database_path();

        let db = match database::connect(db_path) {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to connect to database: {:?}", e);
                return Err(Status::internal("Failed to connect to database"));
            }
        };

        if !source_tar.exists() {
            match fs::write(&source_tar, message.source_data) {
                Ok(_) => {
                    println!("Source tar: {}", source_tar.display());
                    let metadata = fs::metadata(&source_tar)?;
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o444);
                    fs::set_permissions(source_tar.clone(), permissions)?;
                }
                Err(e) => eprintln!("Failed tar: {}", e),
            }

            std::fs::create_dir_all(&source_dir)?;

            let tar_gz = File::open(&source_tar)?;
            let buf_reader = BufReader::new(tar_gz);
            let gz_decoder = GzDecoder::new(buf_reader);
            let mut archive = Archive::new(gz_decoder);

            println!("Source dir: {}", source_dir.display());

            archive.unpack(&source_dir)?;

            fs::set_permissions(&source_dir, fs::Permissions::from_mode(0o555))?;

            let source_files = match store::get_file_paths(&source_dir, vec![]) {
                Ok(files) => files,
                Err(e) => {
                    eprintln!("Failed to get source files: {}", e);
                    return Err(Status::internal("Failed to get source files"));
                }
            };

            match store::set_files_permissions(&source_files) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Failed to set files permissions: {}", e);
                    return Err(Status::internal("Failed to set files permissions"));
                }
            }

            let source_files_hashes = match store::get_file_hashes(source_files) {
                Ok(hashes) => hashes,
                Err(e) => {
                    eprintln!("Failed to get source files hashes: {}", e);
                    return Err(Status::internal("Failed to get source files hashes"));
                }
            };

            let _source_hash_calc = match store::get_source_hash(source_files_hashes) {
                Ok(hash) => hash,
                Err(e) => {
                    eprintln!("Failed to get source hash: {}", e);
                    return Err(Status::internal("Failed to get source hash"));
                }
            };

            // let source_hash_enc = match notary::encrypt(&public_key, source_hash_calc.as_bytes()) {
            //     Ok(hash) => hash,
            //     Err(e) => {
            //         eprintln!("Failed to encrypt source hash: {}", e);
            //         return Err(Status::internal("Failed to encrypt source hash"));
            //     }
            // };
            //
            // if source_hash != source_hash_enc {
            //     return Err(Status::invalid_argument("Signing mismatch"));
            // }
            //
            match database::insert_source(&db, &source_tar) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Failed to insert source: {:?}", e);
                    return Err(Status::internal("Failed to insert source"));
                }
            }
        }

        let source_id = match database::find_source(&db, &source_tar) {
            Ok(source) => source.id,
            Err(e) => {
                eprintln!("Failed to find source: {:?}", e);
                return Err(Status::internal("Failed to find source"));
            }
        };

        match db.close() {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to close database: {:?}", e),
        }

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
