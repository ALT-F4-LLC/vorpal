use crate::api::{BuildRequest, BuildResponse};
use crate::database;
use crate::store;
use tonic::{Request, Response, Status};

pub async fn run(request: Request<BuildRequest>) -> Result<Response<BuildResponse>, Status> {
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
