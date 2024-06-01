use crate::api::package_service_server::PackageService;
use crate::api::{BuildRequest, BuildResponse, PrepareRequest, PrepareResponse};
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
use std::io::Write;
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
        let source_tar_path = source_dir
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

        let source_signature_decode = match hex::decode(&message.source_signature) {
            Ok(data) => data,
            Err(_) => return Err(Status::internal("hex decode of signature failed")),
        };

        let source_signature = match Signature::try_from(source_signature_decode.as_slice()) {
            Ok(signature) => signature,
            Err(e) => {
                eprintln!("Failed to decode signature: {:?}", e);
                return Err(Status::internal("Failed to decode signature"));
            }
        };

        // Used to check if the signature is valid
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

        if !source_tar_path.exists() {
            let mut source_tar = File::create(&source_tar_path)?;
            let source_tar_file_name = source_tar_path.file_name().unwrap();
            match source_tar.write_all(&message.source_data) {
                Ok(_) => {
                    println!("Source file: {}", source_tar_file_name.to_string_lossy());
                    let metadata = fs::metadata(&source_tar_path)?;
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o444);
                    fs::set_permissions(source_tar_path.clone(), permissions)?;
                }
                Err(e) => eprintln!("Failed source file: {}", e),
            }

            std::fs::create_dir_all(&source_dir)?;

            let tar_gz = File::open(&source_tar_path)?;
            let buf_reader = BufReader::new(tar_gz);
            let gz_decoder = GzDecoder::new(buf_reader);
            let mut archive = Archive::new(gz_decoder);

            archive.unpack(&source_dir)?;

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

            // Check if source hash matches
            if source_hash != message.source_hash {
                eprintln!("Source hash mismatch");
                return Err(Status::internal("Source hash mismatch"));
            }

            // TODO: only store file name for file

            match database::insert_source(&db, &source_tar_path) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Failed to insert source: {:?}", e);
                    return Err(Status::internal("Failed to insert source"));
                }
            }

            fs::remove_dir_all(&source_dir)?;
        }

        let source_id = match database::find_source_by_uri(&db, &source_tar_path) {
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
        let message = request.into_inner();

        println!("Build source id: {:?}", message.source_id);

        let db_path = store::get_database_path();
        let db = match database::connect(db_path) {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to connect to database: {:?}", e);
                return Err(Status::internal("Failed to connect to database"));
            }
        };

        let source = match database::find_source_by_id(&db, message.source_id.parse().unwrap()) {
            Ok(source) => source,
            Err(e) => {
                eprintln!("Failed to find source: {:?}", e);
                return Err(Status::internal("Failed to find source"));
            }
        };

        println!("Build source path: {}", source.uri);

        // TODO: create temp build directory

        // TODO: setup temporary build directory

        // TODO: generate build_phase script

        let mut build_phase_script: Vec<String> = Vec::new();
        build_phase_script.push("#!/bin/bash".to_string());
        build_phase_script.push(message.build_phase);
        let build_phase_script = build_phase_script.join("\n");

        println!("Build phase: {:?}", build_phase_script);

        // TODO: generate build_phase sandbox-exec profile

        // TODO: run build_phase script in sandbox

        // TODO: generate install_phase script

        // TODO: generate install_phase sandbox-exec profile

        // TODO: run install_phase script in sandbox

        let response = BuildResponse { data: Vec::new() };

        Ok(Response::new(response))
    }
}
