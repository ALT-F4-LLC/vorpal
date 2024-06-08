use crate::api::{BuildRequest, BuildResponse};
use crate::database;
use crate::store;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;
use tera::Tera;
use tokio::fs;
use tokio::process::Command;
use tonic::{Request, Response, Status};
use walkdir::WalkDir;

mod sandbox_default;

pub async fn run(request: Request<BuildRequest>) -> Result<Response<BuildResponse>, Status> {
    let message = request.into_inner();

    println!("Build source id: {:?}", message.source_id);

    for path in &message.build_deps {
        println!("Build dependency: {}", path);
    }

    let db_path = store::get_database_path();
    let db = match database::connect(db_path) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Failed to connect to database: {:?}", e);
            return Err(Status::internal("Failed to connect to database"));
        }
    };

    let source = match database::find_source_by_id(&db, message.source_id) {
        Ok(source) => source,
        Err(e) => {
            eprintln!("Failed to find source: {:?}", e);
            return Err(Status::internal("Failed to find source"));
        }
    };

    let store_path = store::get_store_dir_path();
    let store_output_path = store_path.join(format!("{}-{}", source.name, source.hash));
    let store_output_tar = store_output_path.with_extension("tar.gz");

    if store_output_path.exists() && store_output_path.is_dir() && store_output_tar.exists() {
        println!("Using cached output: {}", store_output_tar.display());

        let response_data = fs::read(&store_output_tar).await?;
        let response = BuildResponse {
            is_compressed: true,
            package_data: response_data,
        };

        return Ok(Response::new(response));
    }

    if store_output_path.exists() && store_output_path.is_file() {
        println!("Using cached output: {}", store_output_path.display());

        let response_data = fs::read(&store_output_path).await?;
        let response = BuildResponse {
            is_compressed: false,
            package_data: response_data,
        };

        return Ok(Response::new(response));
    }

    let source_tar_path = store::get_source_tar_path(&source.name, &source.hash);

    println!("Build source tar: {}", source_tar_path.display());

    let source_temp_dir = TempDir::new()?;
    let source_temp_dir_path = source_temp_dir.into_path().canonicalize()?;

    match store::unpack_source(&source_temp_dir_path, &source_tar_path) {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Failed to unpack source: {:?}", e);
            return Err(Status::internal("Failed to unpack source"));
        }
    };

    let source_temp_vorpal_dir = source_temp_dir_path.join(".vorpal");

    fs::create_dir(&source_temp_vorpal_dir).await?;

    let build_phase_steps = message
        .build_phase
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");
    let install_phase_steps = message
        .install_phase
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");

    let mut automation_script: Vec<String> = Vec::new();

    automation_script.push("#!/bin/bash".to_string());
    automation_script.push("set -e pipefail".to_string());
    automation_script.push("echo \"Starting build phase\"".to_string());
    automation_script.push(build_phase_steps);
    automation_script.push("echo \"Finished build phase\"".to_string());
    automation_script.push("echo \"Starting install phase\"".to_string());
    automation_script.push(install_phase_steps);
    automation_script.push("echo \"Finished install phase\"".to_string());

    let automation_script = automation_script.join("\n");
    let automation_script_path = source_temp_vorpal_dir.join("automation.sh");

    fs::write(&automation_script_path, &automation_script).await?;

    println!("Build script: {}", automation_script);

    let metadata = fs::metadata(&automation_script_path).await?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&automation_script_path, permissions).await?;

    let os_type = std::env::consts::OS;
    if os_type != "macos" {
        eprintln!("Unsupported OS: {}", os_type);
        return Err(Status::internal("Unsupported OS (currently only macOS)"));
    }

    let sandbox_profile_path = source_temp_vorpal_dir.join("sandbox.sb");

    let mut tera = Tera::default();
    tera.add_raw_template("sandbox_default", sandbox_default::SANDBOX_DEFAULT)
        .unwrap();

    let mut context = tera::Context::new();
    context.insert("tmpdir", source_temp_dir_path.to_str().unwrap());
    let sandbox_profile = tera.render("sandbox_default", &context).unwrap();

    fs::write(&sandbox_profile_path, sandbox_profile.clone()).await?;

    let sandbox_command_args = vec![
        "-f",
        sandbox_profile_path.to_str().unwrap(),
        automation_script_path.to_str().unwrap(),
    ];

    println!("Build args: {:?}", sandbox_command_args);

    let sandbox_output_path = source_temp_vorpal_dir.join("output");

    println!("Build output path: {}", sandbox_output_path.display());

    let mut sandbox_command = Command::new("/usr/bin/sandbox-exec");
    sandbox_command.args(sandbox_command_args);
    sandbox_command.current_dir(&source_temp_dir_path);
    sandbox_command.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    sandbox_command.env("OUTPUT", sandbox_output_path.to_str().unwrap());

    let sandbox_output = sandbox_command.output().await?;
    let sandbox_stdout = String::from_utf8_lossy(&sandbox_output.stdout);

    // TODO: stream output
    println!("{}", sandbox_stdout.trim());

    let sandbox_stderr = String::from_utf8_lossy(&sandbox_output.stderr);
    if sandbox_stderr.len() > 0 {
        eprintln!("Build stderr: {:?}", sandbox_stderr);
        return Err(Status::internal("Build failed"));
    }

    if sandbox_output_path.is_dir() {
        for entry in WalkDir::new(&sandbox_output_path) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    eprintln!("Failed to walk sandbox output: {:?}", e);
                    return Err(Status::internal("Failed to walk sandbox output"));
                }
            };
            let output_path = entry.path().strip_prefix(&sandbox_output_path).unwrap();
            let output_store_path = store_output_path.join(output_path);
            if entry.path().is_dir() {
                fs::create_dir_all(&output_store_path).await?;
            } else {
                fs::copy(&entry.path(), &output_store_path).await?;
            }
        }

        let store_output_files = match store::get_file_paths(&store_output_path, &Vec::new()) {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Failed to get sandbox output files: {:?}", e);
                return Err(Status::internal("Failed to get sandbox output files"));
            }
        };

        match store::compress_files(&store_output_path, &store_output_tar, &store_output_files) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to compress sandbox output: {:?}", e);
                return Err(Status::internal("Failed to compress sandbox output"));
            }
        };
    } else {
        fs::copy(&sandbox_output_path, &store_output_path).await?;
    }

    fs::remove_dir_all(&source_temp_dir_path).await?;

    let package_data_path = if store_output_tar.exists() {
        store_output_tar.clone()
    } else {
        store_output_path.clone()
    };

    println!("Build output: {}", package_data_path.display());

    let response = BuildResponse {
        is_compressed: store_output_tar.exists(),
        package_data: fs::read(&package_data_path).await?,
    };

    Ok(Response::new(response))
}
