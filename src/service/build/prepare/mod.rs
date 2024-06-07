use crate::api::{PrepareRequest, PrepareResponse};
use crate::database;
use crate::notary;
use crate::store;
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use tokio::fs;
use tonic::{Request, Response, Status};

pub async fn run(request: Request<PrepareRequest>) -> Result<Response<PrepareResponse>, Status> {
    let message = request.into_inner();

    let source_dir_path = store::get_source_dir_path(&message.source_name, &message.source_hash);
    let source_tar_path = store::get_source_tar_path(&source_dir_path);

    let public_key = match notary::get_public_key().await {
        Ok(key) => key,
        Err(e) => {
            eprintln!("Failed to get public key: {:?}", e);
            return Err(Status::internal("Failed to get public key"));
        }
    };
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    let source_signature_decode = match hex::decode(message.source_signature) {
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
        match source_tar.write_all(&message.source_data) {
            Ok(_) => {
                let metadata = fs::metadata(&source_tar_path).await?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o444);
                fs::set_permissions(source_tar_path.clone(), permissions).await?;
                let file_name = source_tar_path.file_name().unwrap();
                println!("Source tar: {}", file_name.to_string_lossy());
            }
            Err(e) => eprintln!("Failed source file: {}", e),
        }

        fs::create_dir_all(&source_dir_path).await?;

        match store::unpack_source(&source_dir_path, &source_tar_path) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to unpack source: {:?}", e);
                return Err(Status::internal("Failed to unpack source"));
            }
        };

        let source_files = match store::get_file_paths(&source_dir_path, &vec![]) {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Failed to get source files: {}", e);
                return Err(Status::internal("Failed to get source files"));
            }
        };

        println!("Source files: {:?}", source_files);

        let source_files_hashes = match store::get_file_hashes(&source_files) {
            Ok(hashes) => hashes,
            Err(e) => {
                eprintln!("Failed to get source files hashes: {}", e);
                return Err(Status::internal("Failed to get source files hashes"));
            }
        };

        let source_hash = match store::get_source_hash(&source_files_hashes) {
            Ok(hash) => hash,
            Err(e) => {
                eprintln!("Failed to get source hash: {}", e);
                return Err(Status::internal("Failed to get source hash"));
            }
        };

        println!("Message source hash: {}", message.source_hash);
        println!("Source hash: {}", source_hash);

        if message.source_hash != source_hash {
            eprintln!("Source hash mismatch");
            return Err(Status::internal("Source hash mismatch"));
        }

        match database::insert_source(&db, &source_hash, &message.source_name) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to insert source: {:?}", e);
                return Err(Status::internal("Failed to insert source"));
            }
        }

        fs::remove_dir_all(source_dir_path).await?;
    }

    let source_id = match database::find_source(&db, &message.source_hash, &message.source_name) {
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
        source_id: source_id,
    };

    Ok(Response::new(response))
}
