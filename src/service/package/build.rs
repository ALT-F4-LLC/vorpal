use crate::api::{PackageBuildRequest, PackageBuildResponse, PackageBuildSystem};
use crate::service::get_build_system;
use crate::service::package::sandbox_default;
use crate::store::{archives, paths, temps};
use process_stream::{Process, ProcessExt};
use std::collections::HashMap;
use std::env;
use std::env::consts::{ARCH, OS};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tera::Tera;
use tokio::fs::{create_dir_all, remove_dir_all, set_permissions, write};
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tonic::{Request, Status};

pub async fn run(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    request: Request<PackageBuildRequest>,
) -> Result<(), anyhow::Error> {
    let req = request.into_inner();

    if OS != "linux" && OS != "macos" {
        anyhow::bail!("unsupported operating system");
    }

    let package_system = req.build_system();

    if package_system == PackageBuildSystem::UnknownSystem {
        anyhow::bail!("invalid build system");
    }

    let worker_system = get_build_system(format!("{}-{}", ARCH, OS).as_str());

    // TODO: investigate how to pass this and make it work cross-platform

    if package_system != worker_system {
        anyhow::bail!(
            "system mismatch: {} != {}",
            package_system.as_str_name(),
            worker_system.as_str_name(),
        );
    }

    let package_path = paths::get_package_path(&req.source_name, &req.source_hash);

    if package_path.exists() {
        tx.send(Ok(PackageBuildResponse {
            log_output: format!("package already built: {}", package_path.display()).into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;

        anyhow::bail!("package already built");
    }

    let package_tar_path = paths::get_package_tar_path(&req.source_name, &req.source_hash);

    if !package_path.exists() && package_tar_path.exists() {
        tx.send(Ok(PackageBuildResponse {
            log_output: format!(
                "package tar found (unpacking): {}",
                package_tar_path.display()
            )
            .into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;

        create_dir_all(&package_path)
            .await
            .map_err(|_| Status::internal("failed to create package dir"))?;

        if let Err(err) = archives::unpack_tar_gz(&package_path, &package_tar_path).await {
            anyhow::bail!(format!("failed to unpack package tar: {:?}", err));
        }

        anyhow::bail!("package tar unpacked");
    }

    let build_path = temps::create_dir()
        .await
        .map_err(|_| Status::internal("failed to create temp dir"))?;

    let package_source_path = paths::get_package_source_path(&req.source_name, &req.source_hash);

    if package_source_path.exists() {
        tx.send(Ok(PackageBuildResponse {
            log_output: format!("package source found: {}", package_source_path.display())
                .into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;

        paths::copy_files(&package_source_path, &build_path)
            .await
            .map_err(|e| Status::internal(format!("failed to copy source files: {:?}", e)))?;

        tx.send(Ok(PackageBuildResponse {
            log_output: format!("package source copied: {}", build_path.display()).into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;
    }

    let package_source_tar_path =
        paths::get_package_source_tar_path(&req.source_name, &req.source_hash);

    if !package_source_path.exists() && package_source_tar_path.exists() {
        tx.send(Ok(PackageBuildResponse {
            log_output: format!(
                "package source tar found: {}",
                package_source_tar_path.display()
            )
            .into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;

        create_dir_all(&package_source_path)
            .await
            .map_err(|_| Status::internal("failed to create package source dir"))?;

        tx.send(Ok(PackageBuildResponse {
            log_output: format!(
                "package source unpacking: {}",
                package_source_path.display()
            )
            .into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;

        if let Err(err) =
            archives::unpack_tar_gz(&package_source_path, &package_source_tar_path).await
        {
            anyhow::bail!(format!("failed to unpack source tar: {:?}", err));
        }

        tx.send(Ok(PackageBuildResponse {
            log_output: format!("package source copying: {}", package_source_path.display())
                .into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;

        paths::copy_files(&package_source_path, &build_path)
            .await
            .map_err(|e| Status::internal(format!("failed to copy source files: {:?}", e)))?;

        tx.send(Ok(PackageBuildResponse {
            log_output: format!("package source copied: {}", build_path.display()).into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;
    }
    let build_source_file_paths = paths::get_file_paths(&build_path, &Vec::<&str>::new())
        .map_err(|e| Status::internal(format!("failed to get source files: {:?}", e)))?;

    if build_source_file_paths.is_empty() {
        anyhow::bail!("no source files found");
    }

    // at this point we should be ready to build with source files

    tx.send(Ok(PackageBuildResponse {
        log_output: format!("package building: {}", build_path.display()).into_bytes(),
    }))
    .await
    .unwrap();

    let build_vorpal_path = build_path.join(".vorpal");

    create_dir_all(&build_vorpal_path)
        .await
        .map_err(|_| Status::internal("failed to create build vorpal dir"))?;

    let package_build_script = req
        .build_script
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");

    let build_script = [
        "#!/bin/sh",
        "set -euxo",
        "echo \"PATH: $PATH\"",
        "echo \"Starting build script\"",
        &package_build_script,
        "echo \"Finished build script\"",
    ];

    let build_script_data = build_script.join("\n");

    tx.send(Ok(PackageBuildResponse {
        log_output: format!("package build script: {}", build_script_data).into_bytes(),
    }))
    .await
    .unwrap();

    let build_script_path = build_vorpal_path.join("build.sh");

    write(&build_script_path, build_script_data)
        .await
        .map_err(|_| Status::internal("failed to write build script"))?;

    set_permissions(&build_script_path, fs::Permissions::from_mode(0o755))
        .await
        .map_err(|_| Status::internal("failed to set build script permissions"))?;

    if !build_script_path.exists() {
        anyhow::bail!("build script not found");
    }

    let mut build_environment = HashMap::new();

    for (key, value) in req.build_environment.clone() {
        build_environment.insert(key, value);
    }

    tx.send(Ok(PackageBuildResponse {
        log_output: format!("build packages: {:?}", req.build_packages).into_bytes(),
    }))
    .await
    .unwrap();

    let mut build_store_paths = vec![];

    for path in req.build_packages {
        let build_package = paths::get_package_path(&path.name, &path.hash);
        if !build_package.exists() {
            anyhow::bail!("build package not found");
        }

        let package_bin_path = build_package.join("bin");
        if package_bin_path.exists() {
            build_store_paths.push(package_bin_path.canonicalize()?.display().to_string());
        }

        build_environment.insert(
            path.name.replace('-', "_").to_string(),
            build_package.canonicalize()?.display().to_string(),
        );

        build_store_paths.push(build_package.canonicalize()?.display().to_string());
    }

    if OS == "linux" {
        let current_path = env::var("PATH").unwrap_or_default();
        let current_path_paths = current_path.split(':').collect::<Vec<&str>>();
        for path in current_path_paths {
            let path = Path::new(path);
            if path.exists() {
                let p = path.canonicalize()?;
                build_store_paths.push(p.display().to_string());
            }
        }
    }

    if OS == "macos" {
        build_store_paths.push("/usr/bin".to_string());
        build_store_paths.push("/bin".to_string());
        build_store_paths.push("/Library/Developer/CommandLineTools/usr/bin".to_string());
    }

    tx.send(Ok(PackageBuildResponse {
        log_output: format!("build store paths: {:?}", build_store_paths).into_bytes(),
    }))
    .await
    .unwrap();

    build_environment.insert("PATH".to_string(), build_store_paths.join(":"));

    let build_output_dir = temps::create_dir()
        .await
        .map_err(|_| Status::internal("failed to create temp dir"))?;

    let build_output_path = build_output_dir.canonicalize()?;

    build_environment.insert(
        "output".to_string(),
        build_output_path.display().to_string(),
    );

    tx.send(Ok(PackageBuildResponse {
        log_output: format!("build output path: {}", build_output_path.display()).into_bytes(),
    }))
    .await
    .unwrap();

    tx.send(Ok(PackageBuildResponse {
        log_output: format!("build environment: {:?}", build_environment).into_bytes(),
    }))
    .await
    .unwrap();

    if OS == "linux" {
        let mut sandbox_command = Process::new("/run/current-system/sw/bin/bwrap");

        let mut sandbox_args = vec![
            // bind (read/write) build path
            "--bind",
            build_path.to_str().unwrap(),
            build_path.to_str().unwrap(),
            // bind (read/write) output path
            "--bind",
            build_output_path.to_str().unwrap(),
            build_output_path.to_str().unwrap(),
            // change working directory
            "--chdir",
            build_path.to_str().unwrap(),
            // dev
            "--dev",
            "/dev",
            // bind (read-only) /bin to /bin
            "--ro-bind",
            "/bin",
            "/bin",
            // bind (read-only) /etc to /etc
            "--ro-bind",
            "/etc",
            "/etc",
            // bind (read-only) /home to /home
            "--ro-bind",
            "/home",
            "/home",
            // bind (read-only) /lib to /lib
            "--ro-bind",
            "/lib",
            "/lib",
            // bind (read-only) /nix to /nix
            "--ro-bind",
            "/nix",
            "/nix",
            // bind (read-only) /opt to /opt
            "--ro-bind",
            "/opt",
            "/opt",
            // bind (read-only) /run to /run
            "--ro-bind",
            "/run",
            "/run",
            // kill sandbox if parent dies
            "--die-with-parent",
            // set proc path
            "--proc",
            "/proc",
            // unshare all namespaces
            "--unshare-all",
        ];

        for store_path in &build_store_paths {
            sandbox_args.push("--ro-bind");
            sandbox_args.push(&store_path);
            sandbox_args.push(&store_path);
        }

        for (key, value) in &build_environment {
            sandbox_args.push("--setenv");
            sandbox_args.push(key);
            sandbox_args.push(value);
        }

        sandbox_args.push(build_script_path.to_str().unwrap());

        tx.send(Ok(PackageBuildResponse {
            log_output: format!("sandbox args: {:?}", sandbox_args).into_bytes(),
        }))
        .await
        .unwrap();

        sandbox_command.args(sandbox_args.clone());

        sandbox_command.current_dir(&build_path);

        let mut stream = sandbox_command.spawn_and_stream()?;

        while let Some(output) = stream.next().await {
            tx.send(Ok(PackageBuildResponse {
                log_output: output.as_bytes().to_vec(),
            }))
            .await
            .unwrap();
        }

        // TODO: properly handle error when sandbox command fails
    }

    if OS == "macos" {
        let mut sandbox_command = Process::new("/usr/bin/sandbox-exec");

        let build_profile_path = build_vorpal_path.join("sandbox.sb");

        let mut tera = Tera::default();
        tera.add_raw_template("sandbox_default", sandbox_default::SANDBOX_DEFAULT)
            .unwrap();

        let mut context = tera::Context::new();
        context.insert("tmpdir", build_path.to_str().unwrap());
        let default_profile = tera.render("sandbox_default", &context).unwrap();

        write(&build_profile_path, default_profile)
            .await
            .map_err(|_| Status::internal("failed to write sandbox profile"))?;

        if !build_profile_path.exists() {
            anyhow::bail!("sandbox profile not found");
        }

        let build_command_args = [
            "-f",
            build_profile_path.to_str().unwrap(),
            build_script_path.to_str().unwrap(),
        ];

        tx.send(Ok(PackageBuildResponse {
            log_output: format!("build args: {:?}", build_command_args).into_bytes(),
        }))
        .await
        .unwrap();

        sandbox_command.args(build_command_args);
        sandbox_command.current_dir(&build_path);

        for (key, value) in build_environment {
            sandbox_command.env(key, value);
        }

        let mut stream = sandbox_command.spawn_and_stream()?;

        while let Some(output) = stream.next().await {
            tx.send(Ok(PackageBuildResponse {
                log_output: output.as_bytes().to_vec(),
            }))
            .await
            .unwrap();
        }

        // TODO: properly handle error when sandbox command fails
    }

    let build_output_files = paths::get_file_paths(&build_output_path, &Vec::<&str>::new())
        .map_err(|_| Status::internal("failed to get sandbox output files"))?;

    if build_output_files.is_empty() {
        tx.send(Ok(PackageBuildResponse {
            log_output: format!(
                "no build output files found: {}",
                build_output_path.display()
            )
            .into_bytes(),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"))?;
        anyhow::bail!("no build output files found");
    }

    println!("build output files: {:?}", build_output_files);

    create_dir_all(&package_path)
        .await
        .map_err(|_| Status::internal("failed to create package dir"))?;

    paths::copy_files(&build_output_path, &package_path)
        .await
        .map_err(|e| Status::internal(format!("failed to copy source files: {:?}", e)))?;

    if let Err(err) =
        archives::compress_tar_gz(&build_output_path, &build_output_files, &package_tar_path).await
    {
        anyhow::bail!(format!("failed to compress package tar: {:?}", err));
    }

    tx.send(Ok(PackageBuildResponse {
        log_output: format!("package tar created: {}", package_tar_path.display()).into_bytes(),
    }))
    .await
    .unwrap();

    remove_dir_all(&build_path).await?;

    Ok(())
}
