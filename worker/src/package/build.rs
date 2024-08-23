use bollard::{
    container::{Config, CreateContainerOptions, LogsOptions, StartContainerOptions},
    models::{HostConfig, Mount, MountTypeEnum},
    Docker,
};
use rsa::pss::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tokio::fs::{create_dir_all, remove_dir_all, remove_file, set_permissions, write};
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tonic::{Request, Status, Streaming};
use tracing::debug;
use uuid::Uuid;
use vorpal_notary::get_public_key;
use vorpal_schema::{
    api::package::{
        BuildRequest, BuildResponse, PackageSystem,
        PackageSystem::{Aarch64Linux, Aarch64Macos, Unknown},
    },
    get_package_system,
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_file_paths, get_package_archive_path, get_package_path,
        get_public_key_path, get_source_archive_path, get_source_path,
    },
    temps::{create_temp_dir, create_temp_file},
};

async fn send(
    tx: &Sender<Result<BuildResponse, Status>>,
    package_log: String,
) -> Result<(), anyhow::Error> {
    debug!("{}", package_log);

    tx.send(Ok(BuildResponse { package_log })).await?;

    Ok(())
}

async fn send_error(
    tx: &Sender<Result<BuildResponse, Status>>,
    message: String,
) -> Result<(), anyhow::Error> {
    debug!("{}", message);

    tx.send(Err(Status::internal(message.clone()))).await?;

    anyhow::bail!(message);
}

pub async fn run(
    request: Request<Streaming<BuildRequest>>,
    tx: &Sender<Result<BuildResponse, Status>>,
) -> Result<(), anyhow::Error> {
    // let mut package_sandbox = false;
    // let mut package_systems = vec![];
    let mut package_environment = HashMap::new();
    let mut package_image = String::new();
    let mut package_name = String::new();
    let mut package_packages = vec![];
    let mut package_script = String::new();
    let mut package_source_data: Vec<u8> = Vec::new();
    let mut package_source_data_chunks = 0;
    let mut package_source_data_signature = String::new();
    let mut package_source_hash = String::new();
    let mut package_target = Unknown;
    let mut stream = request.into_inner();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;

        if let Some(data) = chunk.package_source_data {
            package_source_data_chunks += 1;
            package_source_data.extend_from_slice(&data);
        }

        if let Some(signature) = chunk.package_source_data_signature {
            package_source_data_signature = signature;
        }

        if let Some(hash) = chunk.package_source_hash {
            package_source_hash = hash;
        }

        if let Some(image) = chunk.package_image {
            package_image = image;
        }

        // package_sandbox = chunk.package_sandbox;
        // package_systems = chunk.package_systems;
        package_environment = chunk.package_environment;
        package_name = chunk.package_name;
        package_packages = chunk.package_packages;
        package_script = chunk.package_script;
        package_target = PackageSystem::try_from(chunk.package_target)?;
    }

    if package_name.is_empty() {
        send_error(tx, "source name is empty".to_string()).await?
    }

    if package_target == Unknown {
        send_error(tx, "unsupported build target".to_string()).await?
    }

    let mut worker_system = get_package_system(format!("{}-{}", ARCH, OS).as_str());

    if worker_system == Aarch64Macos {
        worker_system = Aarch64Linux; // docker uses linux on macos
    }

    if package_target != worker_system {
        let message = format!(
            "build target mismatch: {} != {}",
            package_target.as_str_name(),
            worker_system.as_str_name()
        );

        send_error(tx, message).await?
    }

    // If package exists, return

    let package_path = get_package_path(&package_source_hash, &package_name);

    if package_path.exists() {
        let message = format!("package: {}", package_path.display());

        send(tx, message).await?;

        return Ok(());
    }

    // If package archive exists, unpack it to package path

    let package_archive_path = get_package_archive_path(&package_source_hash, &package_name);

    if package_archive_path.exists() {
        let message = format!("package archive found: {}", package_archive_path.display());

        send(tx, message).await?;

        create_dir_all(&package_path).await?;

        if let Err(err) = unpack_zstd(&package_path, &package_archive_path).await {
            send_error(tx, format!("failed to unpack package archive: {:?}", err)).await?
        }

        return Ok(());
    }

    // at this point we should be ready to prepare the source

    let source_path = get_source_path(&package_source_hash, &package_name);

    if !source_path.exists() && !package_source_data.is_empty() {
        send(
            tx,
            format!("source chunks received: {}", package_source_data_chunks),
        )
        .await?;

        if package_source_data_signature.is_empty() {
            send_error(tx, "source signature is empty".to_string()).await?
        }

        if package_source_hash.is_empty() {
            send_error(tx, "source hash is empty".to_string()).await?
        }

        let public_key_path = get_public_key_path();

        let public_key = get_public_key(public_key_path).await?;

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        let signature_decode = match hex::decode(package_source_data_signature.clone()) {
            Ok(signature) => signature,
            Err(e) => return send_error(tx, format!("failed to decode signature: {:?}", e)).await,
        };

        let signature = Signature::try_from(signature_decode.as_slice())?;

        verifying_key.verify(&package_source_data, &signature)?;

        let source_archive_path = get_source_archive_path(&package_source_hash, &package_name);

        if !source_archive_path.exists() {
            write(&source_archive_path, &package_source_data).await?;

            let message = format!("source archive: {}", source_archive_path.to_string_lossy());

            send(tx, message).await?;
        }

        if !source_path.exists() {
            let message = format!(
                "source unpacking: {} => {}",
                source_archive_path.to_string_lossy(),
                source_path.to_string_lossy()
            );

            send(tx, message).await?;

            create_dir_all(&source_path).await?;

            unpack_zstd(&source_path, &source_archive_path).await?;

            let message = format!("package source: {}", source_path.to_string_lossy());

            send(tx, message).await?;
        }
    }

    // Handle remote source paths

    // Create build environment

    let mut bin_paths = vec![];
    let mut env_var = HashMap::new();
    let mut store_paths = vec![];

    for (key, value) in package_environment.clone() {
        env_var.insert(key, value);
    }

    for build_package in package_packages.iter() {
        let build_package_path = get_package_path(&build_package.hash, &build_package.name);

        if !build_package_path.exists() {
            let message = format!("Package not found: {}", build_package_path.display());

            println!("Package not found: {}", build_package_path.display());

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
        for package in package_packages.iter() {
            let package_name = package.name.to_lowercase();

            if value.starts_with(&format!("${}", package_name)) {
                let package_path = get_package_path(&package_name, &package.hash);

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

    if package_script.is_empty() {
        send_error(tx, "build script is empty".to_string()).await?
    }

    let sandbox_script = package_script
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");
    let sandbox_script_commands = [
        "#!/bin/sh",
        "set -euxo pipefail",
        "echo \"PATH: $PATH\"",
        "echo \"Starting build script\"",
        &sandbox_script,
        "echo \"Finished build script\"",
    ];
    let sandbox_script_data = sandbox_script_commands.join("\n");
    let sandbox_script_file_path = create_temp_file("sh").await?;

    write(&sandbox_script_file_path, sandbox_script_data).await?;

    set_permissions(&sandbox_script_file_path, Permissions::from_mode(0o755)).await?;

    // Create source directory

    let sandbox_source_dir_path = create_temp_dir().await?;

    if source_path.exists() {
        let source_store_path_files =
            get_file_paths(&source_path, Vec::<String>::new(), Vec::<String>::new())?;

        copy_files(
            &source_path,
            source_store_path_files,
            &sandbox_source_dir_path,
        )
        .await?;
    }

    let sandbox_package_dir_path = create_temp_dir().await?;

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
            source: Some(sandbox_script_file_path.to_str().unwrap().to_string()),
            target: Some("/sandbox/build.sh".to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
        Mount {
            read_only: Some(false),
            source: Some(sandbox_source_dir_path.to_str().unwrap().to_string()),
            target: Some("/sandbox/source".to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
        Mount {
            read_only: Some(false),
            source: Some(sandbox_package_dir_path.to_str().unwrap().to_string()),
            target: Some(package_path.to_str().unwrap().to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
    ];

    for store_path in store_paths {
        let path = PathBuf::from(store_path);

        if !path.exists() {
            remove_dir_all(&sandbox_package_dir_path).await?;
            remove_dir_all(&sandbox_source_dir_path).await?;
            remove_file(&sandbox_script_file_path).await?;

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

    let package_image_default = "ghcr.io/alt-f4-llc/vorpal-sandbox:edge".to_string();

    if package_image.is_empty() {
        package_image = package_image_default;
    }

    let container_config = Config::<String> {
        entrypoint: Some(vec!["/bin/bash".to_string()]),
        cmd: Some(vec!["/sandbox/build.sh".to_string()]),
        env: Some(container_env),
        host_config: Some(container_host_config),
        hostname: Some(container_name),
        image: Some(package_image),
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
            Ok(output) => send(tx, output.to_string().trim().to_string()).await?,
            Err(err) => {
                remove_dir_all(&sandbox_package_dir_path).await?;
                remove_dir_all(&sandbox_source_dir_path).await?;
                remove_file(&sandbox_script_file_path).await?;
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

    let sandbox_package_files = get_file_paths(
        &sandbox_package_dir_path,
        Vec::<String>::new(),
        Vec::<String>::new(),
    )?;

    if sandbox_package_files.is_empty() || sandbox_package_files.len() == 1 {
        remove_dir_all(&sandbox_package_dir_path).await?;
        remove_dir_all(&sandbox_source_dir_path).await?;
        remove_file(&sandbox_script_file_path).await?;
        send_error(tx, "no build output files found".to_string()).await?
    }

    let message = format!("build output files: {}", sandbox_package_files.len());

    send(tx, message).await?;

    // Create package tar from build output files

    if let Err(err) = compress_zstd(
        &sandbox_package_dir_path,
        &sandbox_package_files,
        &package_archive_path,
    )
    .await
    {
        remove_dir_all(&sandbox_package_dir_path).await?;
        remove_dir_all(&sandbox_source_dir_path).await?;
        remove_file(&sandbox_script_file_path).await?;
        send_error(tx, format!("failed to compress package tar: {:?}", err)).await?
    }

    let message = format!(
        "package archive created: {}",
        package_archive_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    // Unpack package tar to package path

    create_dir_all(&package_path).await?;

    if let Err(err) = unpack_zstd(&package_path, &package_archive_path).await {
        remove_dir_all(&sandbox_package_dir_path).await?;
        remove_dir_all(&sandbox_source_dir_path).await?;
        remove_file(&sandbox_script_file_path).await?;
        send_error(tx, format!("failed to unpack package archive: {:?}", err)).await?
    }

    let message = format!(
        "package created: {}",
        package_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    remove_dir_all(&sandbox_package_dir_path).await?;
    remove_dir_all(&sandbox_source_dir_path).await?;
    remove_file(&sandbox_script_file_path).await?;

    Ok(())
}
