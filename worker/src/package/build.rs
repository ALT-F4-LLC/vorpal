use crate::package::{darwin, linux, native};
use anyhow::{anyhow, bail, Result};
use rsa::{
    pss::{Signature, VerifyingKey},
    sha2::Sha256,
    signature::Verifier,
};
use std::{
    collections::HashMap,
    env::consts::{ARCH, OS},
    fs::Permissions,
    iter::Iterator,
    os::unix::fs::PermissionsExt,
    process::Stdio,
};
use tokio::{
    fs::{create_dir_all, set_permissions, write},
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc::Sender,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};
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

async fn send(tx: &Sender<Result<BuildResponse, Status>>, output: String) -> Result<()> {
    debug!("{}", output);

    tx.send(Ok(BuildResponse { output }))
        .await
        .map_err(|err| anyhow!("failed to send response: {:?}", err))?;

    Ok(())
}

// async fn cleanup(
//     sandbox_package_dir: &Path,
//     sandbox_source_dir: &Path,
//     sandbox_script_file: &PathBuf,
// ) -> Result<()> {
//     remove_dir_all(sandbox_package_dir)
//         .await
//         .map_err(|err| anyhow!("failed to remove package directory: {:?}", err))?;
//
//     remove_dir_all(sandbox_source_dir)
//         .await
//         .map_err(|err| anyhow!("failed to remove source directory: {:?}", err))?;
//
//     remove_file(sandbox_script_file)
//         .await
//         .map_err(|err| anyhow!("failed to remove script file: {:?}", err))?;
//
//     Ok(())
// }

pub async fn run(
    request: Request<Streaming<BuildRequest>>,
    tx: &Sender<Result<BuildResponse, Status>>,
) -> Result<()> {
    let mut package_environment = HashMap::new();
    let mut package_name = String::new();
    let mut package_packages = vec![];
    let mut package_sandbox = None;
    let mut package_script = String::new();
    let mut package_source_data: Vec<u8> = vec![];
    let mut package_source_data_chunk = 0;
    let mut package_source_data_signature = None;
    let mut package_source_hash = String::new();
    let mut package_target = Unknown;
    let mut request_stream = request.into_inner();

    while let Some(result) = request_stream.next().await {
        let result = result.map_err(|err| anyhow!("failed to parse request: {:?}", err))?;

        if let Some(data) = result.source_data {
            package_source_data_chunk += 1;
            package_source_data.extend_from_slice(&data);
        }

        package_environment = result.environment;
        package_name = result.name;
        package_packages = result.packages;
        package_sandbox = result.sandbox;
        package_script = result.script;
        package_source_data_signature = result.source_data_signature;
        package_source_hash = result.source_hash;
        package_target = PackageSystem::try_from(result.target)
            .map_err(|err| anyhow!("failed to parse target: {:?}", err))?;
    }

    if package_name.is_empty() {
        bail!("'name' missing in configuration")
    }

    if package_script.is_empty() {
        bail!("'script' missing in configuration")
    }

    if package_source_hash.is_empty() {
        bail!("'source_hash' is missing in configuration")
    }

    if package_target == Unknown {
        bail!("'target' missing in configuration")
    }

    let worker_system = format!("{}-{}", ARCH, OS);

    let worker_target = get_package_system::<PackageSystem>(worker_system.as_str());

    if package_target != worker_target {
        bail!("'target' mismatch")
    }

    // If package exists, return

    let package_path = get_package_path(&package_source_hash, &package_name);

    if package_path.exists() {
        send(tx, package_path.display().to_string()).await?;

        return Ok(());
    }

    // If package archive exists, unpack it to package path

    let package_archive_path = get_package_archive_path(&package_source_hash, &package_name);

    if package_archive_path.exists() {
        send(tx, package_archive_path.display().to_string()).await?;

        create_dir_all(&package_path)
            .await
            .map_err(|err| anyhow!("failed to create package directory: {:?}", err))?;

        if let Err(err) = unpack_zstd(&package_path, &package_archive_path).await {
            bail!("failed to unpack package archive: {:?}", err)
        }

        send(tx, package_path.display().to_string()).await?;

        return Ok(());
    }

    // at this point we should be ready to prepare the source

    let package_source_path = get_source_path(&package_source_hash, &package_name);

    if !package_source_path.exists() && !package_source_data.is_empty() {
        send(tx, format!("Source chunks: {}", package_source_data_chunk)).await?;

        match package_source_data_signature {
            None => bail!("'source_signature' missing in configuration"),
            Some(signature) => {
                if signature.is_empty() {
                    bail!("'source_signature' missing in configuration")
                }

                let public_key_path = get_public_key_path();

                let public_key = get_public_key(public_key_path).await?;

                let signature = Signature::try_from(signature.as_slice())
                    .map_err(|err| anyhow!("failed to parse signature: {:?}", err))?;

                let verifying_key = VerifyingKey::<Sha256>::new(public_key);

                if let Err(msg) = verifying_key.verify(&package_source_data, &signature) {
                    bail!("failed to verify signature: {:?}", msg)
                }
            }
        }

        let source_archive_path = get_source_archive_path(&package_source_hash, &package_name);

        if !source_archive_path.exists() {
            write(&source_archive_path, &package_source_data)
                .await
                .map_err(|err| anyhow!("failed to write source archive: {:?}", err))?;

            send(tx, source_archive_path.display().to_string()).await?;
        }

        if !package_source_path.exists() {
            let message = format!(
                "Source unpack: {} => {}",
                source_archive_path.file_name().unwrap().to_str().unwrap(),
                package_source_path.file_name().unwrap().to_str().unwrap()
            );

            send(tx, message).await?;

            create_dir_all(&package_source_path)
                .await
                .map_err(|err| anyhow!("failed to create source directory: {:?}", err))?;

            unpack_zstd(&package_source_path, &source_archive_path).await?;

            send(tx, package_source_path.display().to_string()).await?;
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

    for p in package_packages.iter() {
        let path = get_package_path(&p.hash, &p.name);

        if !path.exists() {
            let message = format!("package missing: {}", path.display());

            bail!(message)
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
        for package in package_packages.iter() {
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

    // Create build scripts

    let mut sandbox_script_file_path = None;
    let mut sandbox_stdenv_path = None;
    let mut sandbox_stdenv_bash_path = None;

    if let Some(s) = package_sandbox.clone() {
        let stdenv_dir_path = get_package_path(&s.hash, &s.name);

        if !stdenv_dir_path.exists() {
            bail!("sandbox stdenv missing: {}", stdenv_dir_path.display())
        }

        sandbox_stdenv_path = Some(stdenv_dir_path.to_path_buf());

        let sandbox_stdenv_script_path = stdenv_dir_path.join("sandbox");

        if sandbox_stdenv_script_path.exists() {
            sandbox_script_file_path = Some(sandbox_stdenv_script_path.to_path_buf());
        }

        let sandbox_stdenv_bash_script_path = stdenv_dir_path.join("bin/bash");

        if sandbox_stdenv_bash_script_path.exists() {
            sandbox_stdenv_bash_path = Some(sandbox_stdenv_bash_script_path.display().to_string());
        }
    }

    if sandbox_script_file_path.is_none() {
        let stdenv_bash_path = "/bin/sh".to_string();

        let sandbox_script_commands = [
            format!("#!{}", stdenv_bash_path),
            "set -eu".to_string(),
            r"${@}".to_string(),
        ];

        let sandbox_script_data = sandbox_script_commands.join("\n");
        let sandbox_script_path = create_temp_file(None).await?;

        write(&sandbox_script_path, sandbox_script_data)
            .await
            .map_err(|err| anyhow!("failed to write script: {:?}", err))?;

        set_permissions(&sandbox_script_path, Permissions::from_mode(0o755))
            .await
            .map_err(|err| anyhow!("failed to set permissions: {:?}", err))?;

        sandbox_script_file_path = Some(sandbox_script_path.to_path_buf());
        sandbox_stdenv_bash_path = Some(stdenv_bash_path.to_string());
    }

    let sandbox_stdenv_bash_path =
        sandbox_stdenv_bash_path.expect("failed to get stdenv bash path");

    let sandbox_script_file_path = sandbox_script_file_path.expect("failed to get script path");

    let sandbox_script_package_file_path = create_temp_file(None).await?;

    let sandbox_script_package = package_script
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");

    let sandbox_script_package_commands = [
        format!("#!{}", sandbox_stdenv_bash_path),
        "set -eu".to_string(),
        sandbox_script_package,
    ];

    let sandbox_script_package_data = sandbox_script_package_commands.join("\n");

    write(
        &sandbox_script_package_file_path,
        sandbox_script_package_data,
    )
    .await
    .map_err(|err| anyhow!("failed to write package script: {:?}", err))?;

    set_permissions(
        &sandbox_script_package_file_path,
        Permissions::from_mode(0o755),
    )
    .await
    .map_err(|err| anyhow!("failed to set permissions: {:?}", err))?;
    // Create source directory

    let sandbox_source_dir_path = create_temp_dir().await?;

    if package_source_path.exists() {
        let source_store_path_files = get_file_paths(
            &package_source_path,
            Vec::<String>::new(),
            Vec::<String>::new(),
        )?;

        copy_files(
            &package_source_path,
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

    env_var.insert("packages".to_string(), store_paths.join(" ").to_string());

    let sandbox_home_dir_path = create_temp_dir().await?;

    let sandbox_command = match package_sandbox {
        None => Some(
            native::build(
                bin_paths,
                env_var,
                &sandbox_script_package_file_path,
                &sandbox_script_file_path,
                &sandbox_source_dir_path,
            )
            .await?,
        ),
        Some(_) => match worker_target {
            Aarch64Macos | X8664Macos => Some(
                darwin::build(
                    bin_paths.clone(),
                    env_var.clone(),
                    &sandbox_script_package_file_path,
                    &sandbox_script_file_path,
                    &sandbox_source_dir_path,
                    sandbox_stdenv_path,
                )
                .await?,
            ),
            Aarch64Linux | X8664Linux => Some(
                linux::build(
                    bin_paths.clone(),
                    env_var.clone(),
                    &sandbox_home_dir_path,
                    &sandbox_package_dir_path,
                    &store_paths,
                    &sandbox_script_package_file_path,
                    &sandbox_script_file_path,
                    &sandbox_source_dir_path,
                    sandbox_stdenv_path,
                )
                .await?,
            ),
            _ => None,
        },
    };

    let mut child = sandbox_command
        .expect("failed to create sandbox command")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| anyhow!("failed to spawn sandbox command: {:?}", err))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stdout = LinesStream::new(BufReader::new(stdout).lines());
    let stderr = LinesStream::new(BufReader::new(stderr).lines());

    let mut merged = StreamExt::merge(stdout, stderr);

    while let Some(line) = merged.next().await {
        let line = line.map_err(|err| anyhow!("failed to read line: {:?}", err))?;
        send(tx, line.trim().to_string()).await?;
    }

    let status = child
        .wait()
        .await
        .map_err(|err| anyhow!("failed to wait for sandbox: {:?}", err))?;

    if !status.success() {
        bail!("failed to build package")
    }

    // Check for output files

    let sandbox_package_files = get_file_paths(
        &sandbox_package_dir_path,
        Vec::<String>::new(),
        Vec::<String>::new(),
    )?;

    if sandbox_package_files.is_empty() || sandbox_package_files.len() == 1 {
        bail!("no build output files found")
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
        bail!("failed to compress package tar: {:?}", err)
    }

    let message = format!(
        "package archive created: {}",
        package_archive_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    // Unpack package tar to package path

    create_dir_all(&package_path)
        .await
        .map_err(|err| anyhow!("failed to create package directory: {:?}", err))?;

    if let Err(err) = unpack_zstd(&package_path, &package_archive_path).await {
        bail!("failed to unpack package archive: {:?}", err)
    }

    let message = format!(
        "package created: {}",
        package_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    Ok(())
}
