use crate::package::{darwin, linux};
use futures_util::stream::StreamExt;
use rsa::{
    pss::{Signature, VerifyingKey},
    sha2::Sha256,
    signature::Verifier,
};
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::fs::Permissions;
use std::iter::Iterator;
use std::os::unix::fs::PermissionsExt;
use tokio::fs::{create_dir_all, remove_dir_all, remove_file, set_permissions, write};
use tokio::sync::mpsc::Sender;
use tonic::{Request, Status, Streaming};
use tracing::debug;
use vorpal_notary::get_public_key;
use vorpal_schema::{
    api::package::{
        BuildRequest, BuildResponse, PackageSystem,
        PackageSystem::{Aarch64Linux, Aarch64Macos, Unknown, X8664Linux, X8664Macos},
    },
    get_package_system,
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_file_paths, get_package_archive_path, get_package_path,
        get_public_key_path, get_source_archive_path, get_source_path, replace_path_in_files,
    },
    temps::{create_temp_dir, create_temp_file},
};

async fn send(
    tx: &Sender<Result<BuildResponse, Status>>,
    output: String,
) -> Result<(), anyhow::Error> {
    debug!("{}", output);

    tx.send(Ok(BuildResponse { output })).await?;

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
    let mut environment = HashMap::new();
    let mut name = String::new();
    let mut packages = vec![];
    // let mut sandbox = false;
    let mut script = String::new();
    let mut source_data: Vec<u8> = Vec::new();
    let mut source_data_chunk = 0;
    let mut source_data_signature = String::new();
    let mut source_hash = String::new();
    let mut target = Unknown;

    let mut request_stream = request.into_inner();

    while let Some(result) = request_stream.next().await {
        let result = result?;

        if let Some(data) = result.source_data {
            source_data_chunk += 1;
            source_data.extend_from_slice(&data);
        }

        if let Some(signature) = result.source_data_signature {
            source_data_signature = signature;
        }

        if let Some(hash) = result.source_hash {
            source_hash = hash;
        }

        environment = result.environment;
        name = result.name;
        packages = result.packages;
        // sandbox = result.sandbox;
        script = result.script;
        target = PackageSystem::try_from(result.target)?;
    }

    if name.is_empty() {
        send_error(tx, "'name' missing in configuration".to_string()).await?
    }

    if target == Unknown {
        send_error(tx, "'target' unsupported".to_string()).await?
    }

    let worker_system = format!("{}-{}", ARCH, OS);

    let worker_system = get_package_system::<PackageSystem>(worker_system.as_str());

    if worker_system != target {
        send_error(tx, "'target' does not match worker system".to_string()).await?
    }

    // If package exists, return

    let package_path = get_package_path(&source_hash, &name);

    if package_path.exists() {
        send(tx, package_path.display().to_string()).await?;

        return Ok(());
    }

    // If package archive exists, unpack it to package path

    let package_archive_path = get_package_archive_path(&source_hash, &name);

    if package_archive_path.exists() {
        send(tx, package_archive_path.display().to_string()).await?;

        create_dir_all(&package_path).await?;

        if let Err(err) = unpack_zstd(&package_path, &package_archive_path).await {
            send_error(tx, format!("failed to unpack package archive: {:?}", err)).await?
        }

        send(tx, package_path.display().to_string()).await?;

        return Ok(());
    }

    // at this point we should be ready to prepare the source

    let source_path = get_source_path(&source_hash, &name);

    if !source_path.exists() && !source_data.is_empty() {
        send(tx, format!("Source chunks: {}", source_data_chunk)).await?;

        if source_data_signature.is_empty() {
            send_error(tx, "'source_signature' invalid".to_string()).await?
        }

        if source_hash.is_empty() {
            send_error(tx, "'source_hash' missing in configuration".to_string()).await?
        }

        let public_key_path = get_public_key_path();

        let public_key = get_public_key(public_key_path).await?;

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        let signature_decode = match hex::decode(source_data_signature.clone()) {
            Ok(signature) => signature,
            Err(e) => return send_error(tx, format!("failed to decode signature: {:?}", e)).await,
        };

        let signature = Signature::try_from(signature_decode.as_slice())?;

        verifying_key.verify(&source_data, &signature)?;

        let source_archive_path = get_source_archive_path(&source_hash, &name);

        if !source_archive_path.exists() {
            write(&source_archive_path, &source_data).await?;

            send(tx, source_archive_path.display().to_string()).await?;
        }

        if !source_path.exists() {
            let message = format!(
                "Source unpack: {} => {}",
                source_archive_path.file_name().unwrap().to_str().unwrap(),
                source_path.file_name().unwrap().to_str().unwrap()
            );

            send(tx, message).await?;

            create_dir_all(&source_path).await?;

            unpack_zstd(&source_path, &source_archive_path).await?;

            send(tx, source_path.display().to_string()).await?;
        }
    }

    // Handle remote source paths

    // Create build environment

    let mut bin_paths = vec![];
    let mut env_var = HashMap::new();
    let mut store_paths = vec![];

    for (key, value) in environment.clone() {
        env_var.insert(key, value);
    }

    for p in packages.iter() {
        let path = get_package_path(&p.hash, &p.name);

        if !path.exists() {
            let message = format!("package missing: {}", path.display());

            send_error(tx, message).await?
        }

        let bin_path = path.join("bin");

        if bin_path.exists() {
            bin_paths.push(bin_path.display().to_string());
        }

        env_var.insert(
            p.name.to_lowercase().replace('-', "_"),
            path.display().to_string(),
        );

        store_paths.push(path.display().to_string());
    }

    // expand any environment variables that have package references

    for (key, value) in env_var.clone().into_iter() {
        for package in packages.iter() {
            let package_name = package.name.to_lowercase();

            if value.starts_with(&format!("${}", package_name)) {
                let path = get_package_path(&package_name, &package.hash);

                let value =
                    value.replace(&format!("${}", package_name), &path.display().to_string());

                env_var.insert(key.clone(), value);
            }
        }
    }

    send(tx, format!("Sandbox environment: {:?}", env_var)).await?;

    // Create build script

    if script.is_empty() {
        send_error(tx, "build script is empty".to_string()).await?
    }

    let sandbox_script = script
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

    env_var.insert(
        "output".to_string(),
        sandbox_package_dir_path.display().to_string(),
    );

    let mut sandbox_stream = match worker_system {
        Aarch64Macos | X8664Macos => {
            darwin::build(
                bin_paths.clone(),
                env_var.clone(),
                &sandbox_script_file_path,
                &sandbox_source_dir_path,
            )
            .await?
        }
        Aarch64Linux | X8664Linux => {
            linux::build(
                bin_paths,
                env_var.clone(),
                // &sandbox_package_dir_path,
                &sandbox_script_file_path,
                &sandbox_source_dir_path,
            )
            .await?
        }
        _ => anyhow::bail!("unsupported worker system"),
    };

    while let Some(item) = sandbox_stream.next().await {
        send(tx, item.to_string().trim().to_string()).await?
    }

    // Check for output files

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

    let message = format!("output files: {}", sandbox_package_files.len());

    send(tx, message).await?;

    // Replace paths in files

    replace_path_in_files(&sandbox_package_dir_path, &package_path).await?;

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
