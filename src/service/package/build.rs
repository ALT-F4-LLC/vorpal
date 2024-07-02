use crate::api::{PackageBuildRequest, PackageBuildResponse, PackageBuildSystem};
use crate::service::get_build_system;
use crate::service::package::sandbox_default;
use crate::store::archives::{compress_gzip, unpack_gzip};
use crate::store::paths::get_file_paths;
use crate::store::{archives, paths, temps};
use process_stream::{Process, ProcessExt};
use std::collections::HashMap;
use std::env;
use std::env::consts::{ARCH, OS};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tera::Tera;
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, set_permissions, write};
use tokio::process::Command;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tonic::{Request, Status};
use tracing::{debug, warn};

async fn send(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    log_output: Vec<u8>,
) -> Result<(), anyhow::Error> {
    debug!("send: {:?}", String::from_utf8(log_output.clone()).unwrap());

    tx.send(Ok(PackageBuildResponse { log_output })).await?;

    Ok(())
}

async fn send_error(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    message: String,
) -> Result<(), anyhow::Error> {
    debug!("send_error: {}", message);

    tx.send(Err(Status::internal(message.clone()))).await?;

    anyhow::bail!(message);
}

pub async fn run(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    request: Request<PackageBuildRequest>,
) -> Result<(), anyhow::Error> {
    let request = request.into_inner();

    if OS != "linux" && OS != "macos" {
        send_error(tx, format!("unsupported operating system: {}", OS)).await?
    }

    let package_system = request.build_system();

    if package_system == PackageBuildSystem::UnknownSystem {
        send_error(tx, "unsupported build system".to_string()).await?
    }

    let worker_system = get_build_system(format!("{}-{}", ARCH, OS).as_str());

    if package_system != worker_system {
        let message = format!(
            "build system mismatch: {} != {}",
            package_system.as_str_name(),
            worker_system.as_str_name()
        );

        send_error(tx, message.into()).await?
    }

    let package_path = paths::get_package_path(&request.source_name, &request.source_hash);

    // If package exists, return

    if package_path.exists() {
        let message = format!("package already built: {}", package_path.display());

        send_error(tx, message.into()).await?
    }

    let package_tar_path = paths::get_package_tar_path(&request.source_name, &request.source_hash);

    // If package tar exists, unpack it to package path

    if package_tar_path.exists() {
        let message = format!("package tar found: {}", package_tar_path.display());

        send(tx, message.into()).await?;

        create_dir_all(&package_path).await?;

        if let Err(err) = archives::unpack_gzip(&package_path, &package_tar_path).await {
            send_error(tx, format!("failed to unpack package tar: {:?}", err)).await?
        }
    }

    let package_source_tar_path =
        paths::get_package_source_tar_path(&request.source_name, &request.source_hash);

    if !package_source_tar_path.exists() {
        let message = format!(
            "package source tar not found: {}",
            package_source_tar_path.display()
        );

        send_error(tx, message.into()).await?
    }

    let message = format!("package source tar: {}", package_source_tar_path.display());

    send(tx, message.into()).await?;

    let package_build_path = temps::create_dir().await?;
    let package_build_path = package_build_path.canonicalize()?;

    if let Err(err) = unpack_gzip(&package_build_path, &package_source_tar_path).await {
        let message = format!("failed to unpack package source tar: {:?}", err);

        send_error(tx, message.into()).await?
    }

    let message = format!("package source unpacked: {}", package_build_path.display());

    send(tx, message.into()).await?;

    let source_file_paths = get_file_paths(&package_build_path, &Vec::<&str>::new())?;

    if source_file_paths.is_empty() {
        send_error(tx, "no source files found".to_string()).await?
    }

    let message = format!("package source files: {}", source_file_paths.len());

    send(tx, message.into()).await?;

    // At this point, we have source files in the build path

    let message = format!("package building: {}", package_build_path.display());

    send(tx, message.into()).await?;

    // Create build script

    let package_build_script = request
        .build_script
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");

    let package_build_script_shell = [
        "#!/bin/sh",
        "set -ex",
        "echo \"PATH: $PATH\"",
        "echo \"Starting build script\"",
        &package_build_script,
        "echo \"Finished build script\"",
    ];

    let package_build_script_shell_data = package_build_script_shell.join("\n");

    let message = format!("package build script: {}", package_build_script_shell_data);

    send(tx, message.into()).await?;

    let package_build_script_path = temps::create_file("sh").await?;
    let package_build_script_path = package_build_script_path.canonicalize()?;

    write(&package_build_script_path, package_build_script_shell_data).await?;

    set_permissions(&package_build_script_path, Permissions::from_mode(0o755)).await?;

    if !package_build_script_path.exists() {
        send_error(tx, "build script not found".to_string()).await?
    }

    // Create build environment

    let mut build_environment = HashMap::new();

    for (key, value) in request.build_environment.clone() {
        build_environment.insert(key, value);
    }

    let mut build_store_paths = vec![];

    let mut build_bin_paths = vec![];

    for path in request.build_packages {
        let package_path = paths::get_package_path(&path.name, &path.hash);

        if !package_path.exists() {
            let message = format!("build package not found: {}", package_path.display());

            send_error(tx, message.into()).await?
        }

        build_environment.insert(
            path.name.replace('-', "_").to_string(),
            package_path.canonicalize()?.display().to_string(),
        );

        build_store_paths.push(package_path.canonicalize()?.display().to_string());

        let package_bin_path = package_path.join("bin");

        if package_bin_path.exists() {
            build_bin_paths.push(package_bin_path.canonicalize()?.display().to_string());
        }
    }

    if OS == "linux" {
        let current_path = env::var("PATH").unwrap_or_default();
        let current_path_paths = current_path.split(':').collect::<Vec<&str>>();
        for path in current_path_paths {
            let path = Path::new(path);
            if path.exists() {
                let p = path.canonicalize()?;
                build_bin_paths.push(p.display().to_string());
            }
        }
    }

    if OS == "macos" {
        build_bin_paths.push("/usr/bin".to_string());
        build_bin_paths.push("/bin".to_string());
        build_bin_paths.push("/Library/Developer/CommandLineTools/usr/bin".to_string());
    }

    let message = format!("build store paths: {:?}", build_bin_paths);

    send(tx, message.into()).await?;

    build_environment.insert("PATH".to_string(), build_bin_paths.join(":"));

    let package_build_output_path = temps::create_dir().await?;
    let package_build_output_path = package_build_output_path.canonicalize()?;

    build_environment.insert(
        "output".to_string(),
        package_build_output_path.display().to_string(),
    );

    let message = format!("build environment: {:?}", build_environment);

    send(tx, message.into()).await?;

    let message = format!("build output path: {}", package_build_output_path.display());

    send(tx, message.into()).await?;

    // At this point, we have build script and environment

    if OS == "linux" {
        let mut sandbox_command = Process::new("/run/current-system/sw/bin/bwrap");

        // Set sandbox args

        let mut sandbox_args = vec![
            // bind (read/write) build path
            "--bind",
            package_build_path.to_str().unwrap(),
            package_build_path.to_str().unwrap(),
            // bind (read/write) output path
            "--bind",
            package_build_output_path.to_str().unwrap(),
            package_build_output_path.to_str().unwrap(),
            // change working directory
            "--chdir",
            package_build_path.to_str().unwrap(),
            // dev
            "--dev",
            "/dev",
            // bind (read-only) build.sh to build.sh
            "--ro-bind",
            package_build_script_path.to_str().unwrap(),
            package_build_script_path.to_str().unwrap(),
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
            // bind (read-only) /usr/bin to /usr/bin
            "--ro-bind",
            "/usr/bin",
            "/usr/bin",
            // kill sandbox if parent dies
            "--die-with-parent",
            // set proc path
            "--proc",
            "/proc",
            // unshare all namespaces
            "--unshare-all",
        ];

        // Allow store paths to be read-only

        for store_path in &build_store_paths {
            sandbox_args.push("--ro-bind");
            sandbox_args.push(&store_path);
            sandbox_args.push(&store_path);
        }

        // Set build environment

        for (key, value) in &build_environment {
            sandbox_args.push("--setenv");
            sandbox_args.push(key);
            sandbox_args.push(value);
        }

        sandbox_args.push(package_build_script_path.to_str().unwrap());

        sandbox_command.args(sandbox_args.clone());

        send(tx, format!("sandbox args: {:?}", sandbox_args).into()).await?;

        // Set sandbox current dir

        sandbox_command.current_dir(&package_build_path);

        let message = format!("sandbox current dir: {}", package_build_path.display());

        send(tx, message.into()).await?;

        let mut stream = sandbox_command.spawn_and_stream()?;

        while let Some(output) = stream.next().await {
            send(tx, output.as_bytes().to_vec()).await?;

            if let Some(success) = output.is_success() {
                if !success {
                    remove_dir_all(&package_build_path).await?;

                    remove_dir_all(&package_build_output_path).await?;

                    remove_file(&package_build_script_path).await?;

                    send_error(tx, "sandbox command failed".to_string()).await?
                }
            }
        }
    }

    if OS == "macos" {
        // Create sandbox profile

        let build_profile_path = temps::create_file("sb").await?;
        let build_profile_path = build_profile_path.canonicalize()?;

        let mut tera = Tera::default();

        tera.add_raw_template("sandbox_default", sandbox_default::SANDBOX_DEFAULT)?;

        let mut context = tera::Context::new();

        context.insert("tmpdir", package_build_path.to_str().unwrap());

        let default_profile = tera.render("sandbox_default", &context)?;

        write(&build_profile_path, default_profile).await?;

        if !build_profile_path.exists() {
            send_error(tx, "sandbox profile not found".to_string()).await?
        }

        let mut sandbox_command = Process::new("/usr/bin/sandbox-exec");

        // Set sandbox args

        let build_command_args = [
            "-f",
            build_profile_path.to_str().unwrap(),
            package_build_script_path.to_str().unwrap(),
        ];

        sandbox_command.args(build_command_args);

        let message = format!("sandbox build args: {:?}", build_command_args);

        send(tx, message.into()).await?;

        // Set sandbox environment

        for (key, value) in build_environment {
            sandbox_command.env(key, value);
        }

        // Set sandbox current dir

        sandbox_command.current_dir(&package_build_path);

        let message = format!("sandbox current dir: {}", package_build_path.display());

        send(tx, message.into()).await?;

        let mut stream = sandbox_command.spawn_and_stream()?;

        while let Some(output) = stream.next().await {
            send(tx, output.as_bytes().to_vec()).await?;

            if let Some(success) = output.is_success() {
                if !success {
                    remove_dir_all(&package_build_path).await?;

                    remove_dir_all(&package_build_output_path).await?;

                    remove_file(&package_build_script_path).await?;

                    send_error(tx, "sandbox command failed".to_string()).await?
                }
            }
        }

        remove_file(&build_profile_path).await?;
    }

    remove_file(&package_build_script_path).await?;

    remove_dir_all(&package_build_path).await?;

    let package_build_output_files =
        paths::get_file_paths(&package_build_output_path, &Vec::<&str>::new())?;

    if package_build_output_files.is_empty() {
        send_error(tx, "no build output files found".to_string()).await?
    }

    let message = format!("build output files: {}", package_build_output_files.len());

    send(tx, message.into()).await?;

    // TODO: handle patchelf for all files that require it

    for path in &package_build_output_files {
        if !path.is_file() {
            debug!("skipping non-file: {:?}", path.display());
            continue;
        }

        let output = Command::new("patchelf")
            .arg("--print-rpath")
            .arg(path)
            .output()
            .await?;

        if !output.status.success() {
            warn!("failed to print patchelf rpath: {:?}", output.status);
            continue;
        }

        let rpath = String::from_utf8(output.stdout)?;

        if rpath.is_empty() {
            continue;
        }

        let rpath_paths = rpath.trim().split(':').collect::<Vec<&str>>();

        let mut rpath_paths_new = vec![];

        for rpath_path in rpath_paths {
            if rpath_path.starts_with(package_build_output_path.to_str().unwrap()) {
                rpath_paths_new.push(rpath_path.replace(
                    package_build_output_path.to_str().unwrap(),
                    package_path.to_str().unwrap(),
                ));
            } else {
                rpath_paths_new.push(rpath_path.to_string());
            }
        }

        if rpath_paths_new.is_empty() {
            continue;
        }

        let rpath_new = rpath_paths_new.join(":");

        let output = Command::new("patchelf")
            .arg("--set-rpath")
            .arg(&rpath_new)
            .arg(path)
            .output()
            .await?;

        if !output.status.success() {
            continue;
        }
    }

    // Update all files with package_build_output paths

    for path in &package_build_output_files {
        if !path.is_file() {
            debug!("skipping non-file: {:?}", path.display());
            continue;
        }

        let data = read(path).await?;

        let prev = match String::from_utf8(data) {
            Ok(data) => data,
            Err(_) => {
                let message = format!(
                    "failed to convert file data to string: {:?}",
                    path.display()
                );
                debug!("{}", message);
                continue;
            }
        };

        let next = prev.replace(
            package_build_output_path.to_str().unwrap(),
            package_path.to_str().unwrap(),
        );

        if prev == next {
            debug!("skipping unchanged file: {:?}", path.display());
            continue;
        }

        write(path, next).await?;
    }

    // Create package tar from build output files

    if let Err(err) = compress_gzip(
        &package_build_output_path,
        &package_build_output_files,
        &package_tar_path,
    )
    .await
    {
        send_error(tx, format!("failed to compress package tar: {:?}", err)).await?
    }

    remove_dir_all(&package_build_output_path).await?;

    let message = format!("package tar created: {}", package_tar_path.display());

    send(tx, message.into()).await?;

    // Unpack package tar to package path

    create_dir_all(&package_path).await?;

    if let Err(err) = unpack_gzip(&package_path, &package_tar_path).await {
        send_error(tx, format!("failed to unpack package tar: {:?}", err)).await?
    }

    Ok(())
}
