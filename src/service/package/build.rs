use crate::api::{PackageBuildRequest, PackageBuildResponse, PackageBuildSystem};
use crate::service::get_build_system;
use crate::store::archives::{compress_zstd, unpack_zstd};
use crate::store::paths::{
    copy_files, get_file_paths, get_package_archive_path, get_package_path,
    get_source_archive_path, get_source_path,
};
use crate::store::temps::{create_dir, create_file};
use bollard::{
    container::{Config, CreateContainerOptions, LogsOptions, StartContainerOptions},
    models::{HostConfig, Mount, MountTypeEnum},
    Docker,
};
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

        send_error(tx, message).await?
    }

    let package_path = get_package_path(&request.source_name, &request.source_hash);

    // If package exists, return

    if package_path.exists() {
        let message = format!("package: {}", package_path.display());

        send(tx, message).await?;

        return Ok(());
    }

    let package_archive_path = get_package_archive_path(&request.source_name, &request.source_hash);

    // If package tar exists, unpack it to package path

    if package_archive_path.exists() {
        let message = format!("package archive found: {}", package_archive_path.display());

        send(tx, message).await?;

        create_dir_all(&package_path).await?;

        if let Err(err) = unpack_zstd(&package_path, &package_archive_path).await {
            send_error(tx, format!("failed to unpack package archive: {:?}", err)).await?
        }

        return Ok(());
    }

    // If package tar exists, unpack it to package path

    let package_source_path = get_source_path(&request.source_name, &request.source_hash);

    let package_source_archive_path =
        get_source_archive_path(&request.source_name, &request.source_hash);

    if !package_source_path.exists() && package_source_archive_path.exists() {
        let message = format!(
            "package source archive found: {}",
            package_source_archive_path.display()
        );

        send(tx, message).await?;

        create_dir_all(&package_source_path).await?;

        if let Err(err) = unpack_zstd(&package_source_path, &package_source_archive_path).await {
            send_error(tx, format!("failed to unpack package archive: {:?}", err)).await?
        }
    }

    // Create build environment

    let mut bin_paths = vec![];
    let mut env_var = HashMap::new();
    let mut store_paths = vec![];

    for (key, value) in request.build_environment.clone() {
        env_var.insert(key, value);
    }

    for build_package in request.build_packages.iter() {
        let build_package_path = get_package_path(&build_package.name, &build_package.hash);

        if !build_package_path.exists() {
            let message = format!("package not found: {}", build_package_path.display());

            send_error(tx, message).await?
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

    // expand any environment variables that have package references

    for (key, value) in env_var.clone().into_iter() {
        for package in request.build_packages.iter() {
            let package_name = package.name.to_lowercase();

            if value.starts_with(&format!("${}", package_name)) {
                let package_path = get_package_path(&package.name, &package.hash);

                let value = value.replace(
                    &format!("${}", package_name),
                    &package_path.display().to_string(),
                );

                env_var.insert(key.clone(), value);
            }
        }
    }

    let message = format!("build environment: {:?}", env_var);

    send(tx, message).await?;

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

    let sandbox_build_script_path = create_file("sh").await?;

    write(&sandbox_build_script_path, build_script).await?;

    set_permissions(&sandbox_build_script_path, Permissions::from_mode(0o755)).await?;

    if !sandbox_build_script_path.exists() {
        remove_file(&sandbox_build_script_path).await?;
        send_error(tx, "build script not found".to_string()).await?
    }

    // Create source directory

    if !package_source_path.exists() {
        remove_file(&sandbox_build_script_path).await?;
        send_error(tx, "source not found".to_string()).await?
    }

    let sandbox_source_path = create_dir().await?;

    copy_files(&package_source_path, &sandbox_source_path).await?;

    let sandbox_package_path = create_dir().await?;

    #[cfg(unix)]
    let docker = Docker::connect_with_socket_defaults()?;

    let container_name = Uuid::now_v7().to_string();

    let container_options = Some(CreateContainerOptions {
        name: container_name.clone(),
        platform: None,
    });

    let mut container_env = env_var
        .iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<String>>();

    if !bin_paths.is_empty() {
        let path_default = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
        let path = format!("PATH={}:{}", bin_paths.join(":"), path_default);
        container_env.push(path);
    }

    let mut mounts = vec![
        Mount {
            read_only: Some(true),
            source: Some(sandbox_build_script_path.to_str().unwrap().to_string()),
            target: Some("/sandbox/build.sh".to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
        Mount {
            read_only: Some(false),
            source: Some(sandbox_source_path.to_str().unwrap().to_string()),
            target: Some("/sandbox/source".to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
        Mount {
            read_only: Some(false),
            source: Some(sandbox_package_path.to_str().unwrap().to_string()),
            target: Some(package_path.to_str().unwrap().to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
    ];

    for store_path in store_paths {
        let path = PathBuf::from(store_path);

        if !path.exists() {
            let message = format!("store path not found: {}", path.display());

            send_error(tx, message).await?
        }

        mounts.push(Mount {
            read_only: Some(true),
            source: Some(path.to_str().unwrap().to_string()),
            target: Some(path.to_str().unwrap().to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        });
    }

    let container_host_config = HostConfig {
        mounts: Some(mounts),
        ..Default::default()
    };

    let container_image_name = "docker.io/altf4llc/vorpal-sandbox";
    let container_image = format!("{}:{}", container_image_name, "dev");

    let container_config = Config::<String> {
        entrypoint: Some(vec!["/bin/bash".to_string()]),
        cmd: Some(vec!["/sandbox/build.sh".to_string()]),
        env: Some(container_env),
        host_config: Some(container_host_config),
        hostname: Some(container_name),
        image: Some(container_image),
        network_disabled: Some(false),
        working_dir: Some("/sandbox/source".to_string()),
        ..Default::default()
    };

    let container = docker
        .create_container(container_options, container_config)
        .await?;

    docker
        .start_container(&container.id, None::<StartContainerOptions<String>>)
        .await?;

    let options = Some(LogsOptions::<String> {
        follow: true,
        stderr: true,
        stdout: true,
        ..Default::default()
    });

    let mut stream = docker.logs(&container.id, options);

    while let Some(output) = stream.next().await {
        match output {
            Ok(output) => {
                send(tx, output.to_string().trim().to_string()).await?;
            }
            Err(err) => {
                remove_file(&sandbox_build_script_path).await?;
                remove_dir_all(&sandbox_source_path).await?;
                remove_dir_all(&sandbox_package_path).await?;
                send_error(tx, format!("docker logs error: {:?}", err)).await?
            }
        }
    }

    docker
        .remove_container(
            &container.id,
            None::<bollard::container::RemoveContainerOptions>,
        )
        .await?;

    remove_file(&sandbox_build_script_path).await?;
    remove_dir_all(&sandbox_source_path).await?;

    let sandbox_package_files = get_file_paths(&sandbox_package_path, &Vec::<&str>::new())?;

    if sandbox_package_files.is_empty() || sandbox_package_files.len() == 1 {
        send_error(tx, "no build output files found".to_string()).await?
    }

    let message = format!("build output files: {}", sandbox_package_files.len());

    send(tx, message).await?;

    // Create package tar from build output files

    if let Err(err) = compress_zstd(
        &sandbox_package_path,
        &sandbox_package_files,
        &package_archive_path,
    )
    .await
    {
        send_error(tx, format!("failed to compress package tar: {:?}", err)).await?
    }

    remove_dir_all(&sandbox_package_path).await?;

    let message = format!(
        "package store created: {}",
        package_archive_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    // Unpack package tar to package path

    create_dir_all(&package_path).await?;

    if let Err(err) = unpack_zstd(&package_path, &package_archive_path).await {
        send_error(tx, format!("failed to unpack package archive: {:?}", err)).await?
    }

    let message = format!(
        "package created: {}",
        package_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    Ok(())
}
