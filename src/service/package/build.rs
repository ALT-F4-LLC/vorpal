use crate::api::{PackageBuildRequest, PackageBuildResponse, PackageBuildSystem};
use crate::service::get_build_system;
use crate::store::archives::{compress_zstd, unpack_zstd};
use crate::store::paths::{
    get_container_dir_path, get_file_paths, get_image_dir_path, get_package_path,
    get_package_store_path, get_source_store_path,
};
use crate::store::temps::{create_dir, create_file};
use oci_spec::runtime;
use oci_spec::runtime::{Mount, Root, Spec};
use process_stream::{Process, ProcessExt};
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tokio::fs::{create_dir_all, remove_dir_all, remove_file, set_permissions, write};
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tonic::{Request, Status};
use tracing::debug;
use uuid::Uuid;

async fn send(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    log_output: String,
) -> Result<(), anyhow::Error> {
    debug!("{}", log_output);

    tx.send(Ok(PackageBuildResponse { log_output })).await?;

    Ok(())
}

async fn send_error(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    message: String,
) -> Result<(), anyhow::Error> {
    debug!("{}", message);

    tx.send(Err(Status::internal(message.clone()))).await?;

    anyhow::bail!(message);
}

pub async fn set_bundle_config(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    container_id: &str,
    env_var: &HashMap<String, String>,
    package_path: &PathBuf,
    sandbox_build_script_path: &PathBuf,
    sandbox_bundle_path: &PathBuf,
    sandbox_package_path: &PathBuf,
    sandbox_source_path: &PathBuf,
    store_paths: &Vec<String>,
) -> Result<(), anyhow::Error> {
    let spec_path = sandbox_bundle_path.join("config.json");

    let mut spec = Spec::load(spec_path.clone())?;

    // Set hostname

    spec.set_hostname(Some(container_id.to_string()));

    // Set mounts

    let spec_mounts = spec.mounts().as_ref().unwrap_or(&vec![]).clone();

    let mut mounts: Vec<Mount> = Vec::new();

    mounts.extend(spec_mounts.clone());

    // Add build script mount to sandbox

    mounts.push(
        Mount::default()
            .set_source(Some(sandbox_build_script_path.to_path_buf()))
            .set_destination(PathBuf::from("/sandbox/build.sh"))
            .set_typ(Some("bind".to_string()))
            .set_options(Some(vec![
                "nodev".to_string(),
                "nosuid".to_string(),
                "rbind".to_string(),
                "ro".to_string(),
            ]))
            .clone(),
    );

    // Add source mount to sandbox

    mounts.push(
        Mount::default()
            .set_source(Some(sandbox_source_path.clone()))
            .set_destination(PathBuf::from("/sandbox/source"))
            .set_typ(Some("bind".to_string()))
            .set_options(Some(vec![
                "nodev".to_string(),
                "nosuid".to_string(),
                "rbind".to_string(),
                "rw".to_string(),
            ]))
            .clone(),
    );

    // Add package mount to sandbox

    mounts.push(
        Mount::default()
            .set_source(Some(sandbox_package_path.to_path_buf()))
            .set_destination(package_path.to_path_buf())
            .set_typ(Some("bind".to_string()))
            .set_options(Some(vec![
                "nodev".to_string(),
                "nosuid".to_string(),
                "rbind".to_string(),
                "rw".to_string(),
            ]))
            .clone(),
    );

    // Add store mounts to sandbox

    for store_path in store_paths {
        let path = PathBuf::from(store_path);

        if !path.exists() {
            let message = format!("store path not found: {}", path.display());

            send_error(tx, message.into()).await?
        }

        mounts.push(
            Mount::default()
                .set_source(Some(path.clone()))
                .set_destination(path)
                .set_typ(Some("bind".to_string()))
                .set_options(Some(vec![
                    "nodev".to_string(),
                    "nosuid".to_string(),
                    "rbind".to_string(),
                    "ro".to_string(),
                ]))
                .clone(),
        );
    }

    // Add tmpfs mount to sandbox

    mounts.push(
        Mount::default()
            .set_destination("/tmp".to_string().into())
            .set_typ(Some("tmpfs".to_string()))
            .set_options(Some(vec![
                "nodev".to_string(),
                "nosuid".to_string(),
                "rw".to_string(),
            ]))
            .clone(),
    );

    // Set process

    let process_args = vec!["bash".to_string(), "/sandbox/build.sh".to_string()];

    let mut process_env = Vec::new();

    let spec_env = spec
        .process()
        .as_ref()
        .unwrap()
        .env()
        .as_ref()
        .unwrap()
        .clone();

    // Update PATH

    let mut bin_paths = vec![];

    for build_package_path in store_paths {
        let package_path = PathBuf::from(build_package_path);

        if !package_path.exists() {
            let message = format!("package not found: {}", package_path.display());

            send_error(tx, message.into()).await?
        }

        let package_bin_path = package_path.join("bin");

        if package_bin_path.exists() {
            bin_paths.push(package_bin_path.display().to_string());
        }
    }

    for env in spec_env {
        if env.starts_with("PATH=") {
            let path_value = env.split('=').collect::<Vec<&str>>()[1];

            if bin_paths.is_empty() {
                process_env.push(env.clone());

                continue;
            }

            process_env.push(format!("PATH={}:{}", bin_paths.join(":"), path_value));

            continue;
        }

        process_env.push(env.clone());
    }

    // Update environment variables

    for (key, value) in env_var {
        process_env.push(format!("{}={}", key, value));
    }

    // Set process

    let mut process = runtime::Process::default();

    process.set_args(Some(process_args));
    process.set_cwd(PathBuf::from("/sandbox/source"));
    process.set_env(Some(process_env));

    // Set root directory

    let mut root = Root::default();
    let root_path = sandbox_bundle_path.join("rootfs");
    let root = root
        .set_path(root_path.to_path_buf())
        .set_readonly(Some(true));

    // Save spec

    spec.set_mounts(Some(mounts.clone()));
    spec.set_process(Some(process.clone()));
    spec.set_root(Some(root.clone()));

    let _ = spec.save(spec_path);

    Ok(())
}

pub async fn run(
    tx: &Sender<Result<PackageBuildResponse, Status>>,
    request: Request<PackageBuildRequest>,
) -> Result<(), anyhow::Error> {
    if OS != "linux" {
        send_error(tx, format!("unsupported operating system: {}", OS)).await?
    }

    let request = request.into_inner();

    let package_build_system = request.build_system();

    if package_build_system == PackageBuildSystem::UnknownSystem {
        send_error(tx, "unsupported build system".to_string()).await?
    }

    let worker_build_system = get_build_system(format!("{}-{}", ARCH, OS).as_str());

    if package_build_system != worker_build_system {
        let message = format!(
            "build system mismatch: {} != {}",
            package_build_system.as_str_name(),
            worker_build_system.as_str_name()
        );

        send_error(tx, message.into()).await?
    }

    let package_path = get_package_path(&request.source_name, &request.source_hash);

    // If package exists, return

    if package_path.exists() {
        let message = format!("package already built: {}", package_path.display());

        send_error(tx, message.into()).await?
    }

    let package_store_path = get_package_store_path(&request.source_name, &request.source_hash);

    // If package tar exists, unpack it to package path

    if package_store_path.exists() {
        let message = format!("package store found: {}", package_store_path.display());

        send(tx, message.into()).await?;

        create_dir_all(&package_path).await?;

        if let Err(err) = unpack_zstd(&package_path, &package_store_path).await {
            send_error(tx, format!("failed to unpack package tar: {:?}", err)).await?
        }
    }

    // Create build environment

    let mut bin_paths = vec![];
    let mut env_var = HashMap::new();
    let mut store_paths = vec![];

    for (key, value) in request.build_environment.clone() {
        env_var.insert(key, value);
    }

    for build_package in request.build_packages {
        let build_package_path = get_package_path(&build_package.name, &build_package.hash);

        if !build_package_path.exists() {
            let message = format!("package not found: {}", build_package_path.display());

            send_error(tx, message.into()).await?
        }

        let build_package_bin_path = build_package_path.join("bin");

        if build_package_bin_path.exists() {
            bin_paths.push(build_package_bin_path.display().to_string());
        }

        env_var.insert(
            build_package.name.to_lowercase().replace('-', "_"),
            build_package_path.display().to_string(),
        );

        store_paths.push(build_package_path.display().to_string());
    }

    env_var.insert("output".to_string(), package_path.display().to_string());

    let message = format!("build environment: {:?}", env_var);

    send(tx, message.into()).await?;

    // Create build script

    let build_script = request
        .build_script
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");

    let build_script_commands = [
        "#!/bin/sh",
        "set -ex",
        "echo \"PATH: $PATH\"",
        "echo \"Starting build script\"",
        &build_script,
        "echo \"Finished build script\"",
    ];

    let build_script = build_script_commands.join("\n");

    let message = format!("build script: {}", build_script);

    send(tx, message.into()).await?;

    let sandbox_build_script_path = create_file("sh").await?;

    write(&sandbox_build_script_path, build_script).await?;

    set_permissions(&sandbox_build_script_path, Permissions::from_mode(0o755)).await?;

    if !sandbox_build_script_path.exists() {
        remove_file(&sandbox_build_script_path).await?;
        send_error(tx, "build script not found".to_string()).await?
    }

    // Create source directory

    let source_path = get_source_store_path(&request.source_name, &request.source_hash);

    if !source_path.exists() {
        remove_file(&sandbox_build_script_path).await?;
        send_error(tx, "source store not found".to_string()).await?
    }

    let sandbox_source_path = create_dir().await?;

    unpack_zstd(&sandbox_source_path, &source_path).await?;

    // TODO: handle custom build images

    // Create container bundle

    let mut bundle_command =
        Process::new("/nix/store/f7xznd3x3q6f2vxnw2j91838wmvps9ha-umoci-0.4.7/bin/umoci");

    // TODO: see if this works with remote images

    let bundle_image_path = get_image_dir_path().join("sandbox");

    if !bundle_image_path.exists() {
        remove_file(&sandbox_build_script_path).await?;
        remove_dir_all(&sandbox_source_path).await?;
        send_error(tx, "bundle image path not found".to_string()).await?
    }

    let bundle_image_tag = "ubuntu-24.04";

    let bundle_image = format!("{}:{}", bundle_image_path.display(), bundle_image_tag);

    let sandbox_bundle_path = create_dir().await?;

    let bundle_args = vec![
        "unpack",
        "--image",
        bundle_image.as_str(),
        "--rootless",
        sandbox_bundle_path.to_str().unwrap(),
    ];

    bundle_command.args(bundle_args);

    let mut stream = bundle_command.spawn_and_stream()?;

    while let Some(output) = stream.next().await {
        send(tx, output.to_string()).await?;

        if let Some(success) = output.is_success() {
            if !success {
                remove_file(&sandbox_build_script_path).await?;
                remove_dir_all(&sandbox_source_path).await?;
                remove_dir_all(&sandbox_bundle_path).await?;
                send_error(tx, "bundle command failed".to_string()).await?
            }
        }
    }

    // Create container spec

    let container_id = Uuid::now_v7().to_string();
    let sandbox_package_path = create_dir().await?;

    set_bundle_config(
        tx,
        &container_id,
        &env_var,
        &package_path,
        &sandbox_build_script_path,
        &sandbox_bundle_path,
        &sandbox_package_path,
        &sandbox_source_path,
        &store_paths,
    )
    .await?;

    let mut runc = Process::new("/nix/store/488pild8br6aaaqv1069qgcw4l62ib3g-runc-1.1.13/bin/runc");

    let container_root_path = get_container_dir_path();

    let runc_args = vec![
        "--root",
        container_root_path.to_str().unwrap(),
        "run",
        "--bundle",
        sandbox_bundle_path.to_str().unwrap(),
        container_id.as_str(),
    ];

    runc.args(runc_args.clone());

    let mut stream = runc.spawn_and_stream()?;

    while let Some(output) = stream.next().await {
        send(tx, output.to_string()).await?;

        if let Some(success) = output.is_success() {
            if !success {
                remove_file(&sandbox_build_script_path).await?;
                remove_dir_all(&sandbox_source_path).await?;
                remove_dir_all(&sandbox_bundle_path).await?;
                remove_dir_all(&sandbox_package_path).await?;
                send_error(tx, "runc command failed".to_string()).await?
            }
        }
    }

    remove_file(&sandbox_build_script_path).await?;
    remove_dir_all(&sandbox_source_path).await?;
    remove_dir_all(&sandbox_bundle_path).await?;

    let build_path_files = get_file_paths(&sandbox_package_path, &Vec::<&str>::new())?;

    if build_path_files.is_empty() {
        send_error(tx, "no build output files found".to_string()).await?
    }

    let message = format!("build output files: {}", build_path_files.len());

    send(tx, message.into()).await?;

    // Create package tar from build output files

    if let Err(err) = compress_zstd(
        &sandbox_package_path,
        &build_path_files,
        &package_store_path,
    )
    .await
    {
        send_error(tx, format!("failed to compress package tar: {:?}", err)).await?
    }

    remove_dir_all(&sandbox_package_path).await?;

    let message = format!(
        "package store created: {}",
        package_store_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message.into()).await?;

    // Unpack package tar to package path

    create_dir_all(&package_path).await?;

    if let Err(err) = unpack_zstd(&package_path, &package_store_path).await {
        send_error(tx, format!("failed to unpack package archive: {:?}", err)).await?
    }

    let message = format!(
        "package created: {}",
        package_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message.into()).await?;

    Ok(())
}
