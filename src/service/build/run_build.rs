use crate::api::{BuildRequest, BuildResponse};
use crate::database;
use crate::service::build::sandbox_default;
use crate::store::archives;
use crate::store::paths;
use crate::store::temps;
use process_stream::{Process, ProcessExt, StreamExt};
use std::os::unix::fs::PermissionsExt;
use tera::Tera;
use tokio::fs;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use walkdir::WalkDir;

type BuildStream = ReceiverStream<Result<BuildResponse, Status>>;

pub async fn run(request: Request<BuildRequest>) -> Result<Response<BuildStream>, Status> {
    let (tx, rx) = mpsc::channel(4);

    tokio::spawn(async move {
        let message = request.into_inner();

        let db_path = paths::get_database_path();
        let db = database::connect(db_path)
            .map_err(|e| Status::internal(format!("Failed to connect to database: {:?}", e)))?;

        let source = database::find_source_by_id(&db, message.source_id)
            .map_err(|e| Status::internal(format!("Failed to find source by id: {:?}", e)))?;

        let store_path = paths::get_store_path();
        let store_output_path = store_path.join(format!("{}-{}", source.name, source.hash));
        let store_output_tar = store_output_path.with_extension("tar.gz");

        let response_chunks_size = 8192; // default grpc limit

        if store_output_path.exists() {
            tx.send(Ok(BuildResponse {
                is_archive: true,
                package_data: vec![],
                package_log: format!("using cached output: {}", store_output_path.display()),
            }))
            .await
            .unwrap();

            let package_data = fs::read(&store_output_path)
                .await
                .map_err(|_| Status::internal("Failed to read cached output"))?;

            for package_chunk in package_data.chunks(response_chunks_size) {
                tx.send(Ok(BuildResponse {
                    is_archive: false,
                    package_data: package_chunk.to_vec(),
                    package_log: "".to_string(),
                }))
                .await
                .unwrap();
            }

            return Ok(());
        }

        if store_output_tar.exists() {
            tx.send(Ok(BuildResponse {
                is_archive: true,
                package_data: vec![],
                package_log: format!("using cached output: {}", store_output_tar.display()),
            }))
            .await
            .unwrap();

            let package_data = fs::read(&store_output_tar)
                .await
                .map_err(|_| Status::internal("Failed to read cached output"))?;

            for chunk in package_data.chunks(response_chunks_size) {
                tx.send(Ok(BuildResponse {
                    is_archive: true,
                    package_data: chunk.to_vec(),
                    package_log: "".to_string(),
                }))
                .await
                .unwrap();
            }

            return Ok(());
        }

        let source_tar_path = paths::get_package_source_tar_path(&source.name, &source.hash);

        tx.send(Ok(BuildResponse {
            is_archive: false,
            package_data: vec![],
            package_log: format!("building source: {}", source_tar_path.display()),
        }))
        .await
        .unwrap();

        let source_temp_dir = temps::create_dir()
            .await
            .map_err(|_| Status::internal("Failed to create temp dir"))?;
        let source_temp_dir_path = source_temp_dir.canonicalize()?;

        if let Err(err) = archives::unpack_tar_gz(&source_temp_dir_path, &source_tar_path).await {
            return Err(Status::internal(format!(
                "Failed to unpack source tar: {:?}",
                err
            )));
        }

        let source_temp_vorpal_dir = source_temp_dir_path.join(".vorpal");

        fs::create_dir(&source_temp_vorpal_dir)
            .await
            .map_err(|_| Status::internal("Failed to create vorpal temp dir"))?;

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

        let automation_script = [
            "#!/bin/bash",
            "set -e pipefail",
            "echo \"Starting build phase\"",
            &build_phase_steps,
            "echo \"Finished build phase\"",
            "echo \"Starting install phase\"",
            &install_phase_steps,
            "echo \"Finished install phase\"",
        ];

        let automation_script_data = automation_script.join("\n");

        tx.send(Ok(BuildResponse {
            is_archive: false,
            package_data: vec![],
            package_log: format!("build script: {}", automation_script_data),
        }))
        .await
        .unwrap();

        let automation_script_path = source_temp_vorpal_dir.join("automation.sh");

        fs::write(&automation_script_path, automation_script_data)
            .await
            .map_err(|_| Status::internal("Failed to write automation script"))?;

        let metadata = fs::metadata(&automation_script_path).await?;
        let mut permissions = metadata.permissions();

        permissions.set_mode(0o755);

        fs::set_permissions(&automation_script_path, permissions)
            .await
            .map_err(|_| Status::internal("Failed to set automation script permissions"))?;

        let os_type = std::env::consts::OS;
        if os_type != "macos" {
            return Err(Status::unimplemented("unsupported OS"));
        }

        let sandbox_profile_path = source_temp_vorpal_dir.join("sandbox.sb");

        let mut tera = Tera::default();
        tera.add_raw_template("sandbox_default", sandbox_default::SANDBOX_DEFAULT)
            .unwrap();

        let mut context = tera::Context::new();
        context.insert("tmpdir", source_temp_dir_path.to_str().unwrap());
        let sandbox_profile = tera.render("sandbox_default", &context).unwrap();

        fs::write(&sandbox_profile_path, sandbox_profile)
            .await
            .map_err(|_| Status::internal("Failed to write sandbox profile"))?;

        let sandbox_command_args = [
            "-f",
            sandbox_profile_path.to_str().unwrap(),
            automation_script_path.to_str().unwrap(),
        ];

        tx.send(Ok(BuildResponse {
            is_archive: false,
            package_data: vec![],
            package_log: format!("sandbox command args: {:?}", sandbox_command_args),
        }))
        .await
        .unwrap();

        let sandbox_output_path = source_temp_vorpal_dir.join("output");

        tx.send(Ok(BuildResponse {
            is_archive: false,
            package_data: vec![],
            package_log: format!("sandbox output path: {}", sandbox_output_path.display()),
        }))
        .await
        .unwrap();

        let mut sandbox_command = Process::new("/usr/bin/sandbox-exec");
        sandbox_command.args(sandbox_command_args);
        sandbox_command.current_dir(&source_temp_dir_path);
        // sandbox_command.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        sandbox_command.env("OUTPUT", sandbox_output_path.to_str().unwrap());

        let mut stream = sandbox_command.spawn_and_stream()?;

        while let Some(output) = stream.next().await {
            tx.send(Ok(BuildResponse {
                is_archive: false,
                package_data: vec![],
                package_log: format!("sandbox output: {}", output),
            }))
            .await
            .unwrap();
        }

        if sandbox_output_path.is_file() {
            fs::copy(&sandbox_output_path, &store_output_path).await?;
        }

        if sandbox_output_path.is_dir() {
            for entry in WalkDir::new(&sandbox_output_path) {
                let entry = entry.map_err(|e| {
                    Status::internal(format!("Failed to walk sandbox output: {:?}", e))
                })?;
                let output_path = entry.path().strip_prefix(&sandbox_output_path).unwrap();
                let output_store_path = store_output_path.join(output_path);
                if entry.path().is_dir() {
                    fs::create_dir_all(&output_store_path)
                        .await
                        .map_err(|_| Status::internal("Failed to create sandbox output dir"))?;
                } else {
                    fs::copy(&entry.path(), &output_store_path)
                        .await
                        .map_err(|_| Status::internal("Failed to copy sandbox output file"))?;
                }
            }

            let store_output_files = paths::get_file_paths(&store_output_path, &Vec::<&str>::new())
                .map_err(|_| Status::internal("Failed to get sandbox output files"))?;

            if let Err(err) = archives::compress_tar_gz(
                &store_output_path,
                &store_output_tar,
                &store_output_files,
            )
            .await
            {
                return Err(Status::internal(format!(
                    "Failed to compress sandbox output: {:?}",
                    err
                )));
            }
        }

        fs::remove_dir_all(&source_temp_dir_path).await?;

        let is_archive = store_output_tar.exists();
        let package_data_path = if is_archive {
            store_output_tar
        } else {
            store_output_path
        };

        tx.send(Ok(BuildResponse {
            is_archive: false,
            package_data: vec![],
            package_log: format!("build completed: {}", package_data_path.display()),
        }))
        .await
        .unwrap();

        let package_data = fs::read(&package_data_path).await?;

        for package_chunk in package_data.chunks(response_chunks_size) {
            tx.send(Ok(BuildResponse {
                is_archive,
                package_data: package_chunk.to_vec(),
                package_log: "".to_string(),
            }))
            .await
            .unwrap();
        }

        if let Err(e) = db.close() {
            return Err(Status::internal(format!(
                "Failed to close database: {:?}",
                e.1
            )));
        }

        Ok(())
    });

    Ok(Response::new(ReceiverStream::new(rx)))
}
