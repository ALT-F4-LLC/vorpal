use crate::worker::{darwin, darwin::profile, linux};
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
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Stdio,
};
use tera::Tera;
use tokio::{
    fs::{create_dir_all, remove_file, set_permissions, write},
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc::Sender,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::{Request, Status, Streaming};
use tracing::debug;
use vorpal_notary::get_public_key;
use vorpal_schema::{
    get_package_system,
    vorpal::{
        package::v0::PackageSystem::{
            Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos,
        },
        package::v0::{PackageSandboxPath, PackageSystem},
        worker::v0::{BuildRequest, BuildResponse},
    },
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_file_paths, get_package_archive_path, get_package_lock_path,
        get_package_path, get_public_key_path, get_source_archive_path, get_source_path,
    },
};

async fn send(tx: &Sender<Result<BuildResponse, Status>>, output: String) -> Result<()> {
    debug!("{}", output);

    tx.send(Ok(BuildResponse { output }))
        .await
        .map_err(|err| anyhow!("failed to send response: {:?}", err))?;

    Ok(())
}

pub async fn run(
    sandbox_path: &Path,
    request: Request<Streaming<BuildRequest>>,
    tx: &Sender<Result<BuildResponse, Status>>,
) -> Result<()> {
    let mut package_environment = vec![];
    let mut package_name = String::new();
    let mut package_packages = vec![];
    let mut package_sandbox = None;
    let mut package_script = String::new();
    let mut package_source_data: Vec<u8> = vec![];
    let mut package_source_data_chunk = 0;
    let mut package_source_data_signature = None;
    let mut package_source_hash = String::new();
    let mut package_target = UnknownSystem;
    let mut request_stream = request.into_inner();

    // Parse request stream

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

    // Check if required fields are present

    if package_name.is_empty() {
        bail!("'name' missing in configuration")
    }

    if package_script.is_empty() {
        bail!("'script' missing in configuration")
    }

    if package_source_hash.is_empty() {
        bail!("'source_hash' is missing in configuration")
    }

    if package_target == UnknownSystem {
        bail!("'target' missing in configuration")
    }

    // Check if worker target matches package target

    let worker_system = format!("{}-{}", ARCH, OS);

    let worker_target = get_package_system::<PackageSystem>(worker_system.as_str());

    if package_target != worker_target {
        bail!("'target' mismatch")
    }

    // Check if package is locked

    let package_lock_path = get_package_lock_path(&package_source_hash, &package_name);

    if package_lock_path.exists() {
        bail!("package is locked") // TODO: figure out better way to handle this (e.g. prompt, ui, etc)
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

    // create package directory and lock file to prevent concurrent builds

    create_dir_all(&package_path)
        .await
        .map_err(|err| anyhow!("failed to create package directory: {:?}", err))?;

    // TODO: add metadata to the lockfile to know how to clean up

    write(&package_lock_path, "")
        .await
        .map_err(|err| anyhow!("failed to write package lock: {:?}", err))?;

    // Check if source archive is present

    let source_archive_path = get_source_archive_path(&package_source_hash, &package_name);

    let source_path = get_source_path(&package_source_hash, &package_name);

    if source_archive_path.exists() {
        create_dir_all(&source_path)
            .await
            .map_err(|err| anyhow!("failed to create source directory: {:?}", err))?;

        unpack_zstd(&source_path, &source_archive_path).await?;

        send(tx, source_path.display().to_string()).await?;
    }

    // Check if source data is present and verify signature

    if !source_path.exists() && !package_source_data.is_empty() {
        send(tx, format!("Source chunks: {}", package_source_data_chunk)).await?;

        // Verify source data signature

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

        if source_archive_path.exists() {
            bail!("source archive already exists")
        }

        write(&source_archive_path, &package_source_data)
            .await
            .map_err(|err| anyhow!("failed to write source archive: {:?}", err))?;

        send(tx, source_archive_path.display().to_string()).await?;

        if source_path.exists() {
            bail!("source path already exists")
        }

        let message = format!(
            "Source unpack: {} => {}",
            source_archive_path.file_name().unwrap().to_str().unwrap(),
            source_path.file_name().unwrap().to_str().unwrap()
        );

        send(tx, message).await?;

        create_dir_all(&source_path)
            .await
            .map_err(|err| anyhow!("failed to create source directory: {:?}", err))?;

        unpack_zstd(&source_path, &source_archive_path).await?;

        send(tx, source_path.display().to_string()).await?;
    }

    // Create sandbox environment

    let mut packages_paths = vec![];
    let mut package_env = HashMap::new();

    // Add package environment variables

    for env in package_environment.clone() {
        package_env.insert(env.key, env.value);
    }

    // Add package environment variables and paths

    for p in package_packages.iter() {
        let path = get_package_path(&p.hash, &p.name);

        if !path.exists() {
            let message = format!("package missing: {}", path.display());

            bail!(message)
        }

        packages_paths.push(path.display().to_string());

        package_env.insert(
            p.name.to_lowercase().replace('-', "_"),
            path.display().to_string(),
        );
    }

    // Setup sandbox path source

    let sandbox_source_path = sandbox_path.join("source");

    create_dir_all(&sandbox_source_path)
        .await
        .map_err(|err| anyhow!("failed to create source directory: {:?}", err))?;

    if source_path.exists() {
        let source_files = get_file_paths(&source_path, vec![], vec![])?;

        copy_files(&source_path, source_files, &sandbox_source_path).await?;
    }

    // Add package(s) environment variables

    let package_env_name = package_name.to_lowercase().replace('-', "_");

    package_env.insert(package_env_name.clone(), package_path.display().to_string());

    package_env.insert("output".to_string(), package_path.display().to_string());

    package_env.insert("packages".to_string(), packages_paths.join(" ").to_string());

    // Expand package references in environment variables

    for (key, _) in package_env.clone().into_iter() {
        for p in package_packages.iter() {
            let p_key = p.name.to_lowercase().replace('-', "_");
            let p_path = get_package_path(&p.hash, &p.name);
            let p_envvar = format!("${}", p_key);

            let value = package_env.get(&key).unwrap().clone();
            let p_value = value.replace(&p_envvar, &p_path.display().to_string());

            if p_value == value {
                continue;
            }

            package_env.insert(key.clone(), p_value);
        }

        let value = package_env.get(&key).unwrap().clone();

        let value = value.replace(
            &format!("${}", package_name.to_lowercase().replace('-', "_")),
            &package_path.display().to_string(),
        );

        package_env.insert(key.clone(), value.clone());
    }

    // Expand self '$<package>' references in script

    package_script = package_script.replace(
        format!(r"${}", package_env_name).as_str(),
        &package_path.display().to_string(),
    );

    // Expand self '$output' references in script

    package_script = package_script.replace(r"$output", &package_path.display().to_string());

    // Expand other '$packages' references in script

    package_script = package_script.replace(r"$packages", &packages_paths.join(" "));

    // Expand other '$<package>' references in script

    for package in package_packages.iter() {
        let placeholder = format!(r"${}", package.name.replace('-', "_").to_lowercase());
        let path = get_package_path(&package.hash, &package.name);

        package_script = package_script.replace(&placeholder, &path.display().to_string());
    }

    // Write package script

    let sandbox_script_path = sandbox_path.join("package.sh");

    write(&sandbox_script_path, package_script.clone())
        .await
        .map_err(|err| anyhow!("failed to write package script: {:?}", err))?;

    set_permissions(&sandbox_script_path, Permissions::from_mode(0o755))
        .await
        .map_err(|err| anyhow!("failed to set permissions: {:?}", err))?;

    // Create sandbox command

    let mut package_sandbox_paths = vec![];

    if let Some(sandbox) = package_sandbox {
        for sandbox_path in sandbox.paths {
            let mut source = sandbox_path.source.clone();
            let mut target = sandbox_path.target.clone();

            for package in package_packages.iter() {
                let placeholder = format!(r"${}", package.name.replace('-', "_").to_lowercase());
                let path = get_package_path(&package.hash, &package.name);

                source = source.replace(&placeholder, &path.display().to_string());
                target = target.replace(&placeholder, &path.display().to_string());
            }

            package_sandbox_paths.push(PackageSandboxPath { source, target });
        }
    }

    let mut sandbox_command = match worker_target {
        Aarch64Macos | X8664Macos => {
            let profile_path = sandbox_path.join("package.sb");

            let mut tera = Tera::default();

            tera.add_raw_template("build_default", profile::STDENV_DEFAULT)
                .unwrap();

            let profile_context = tera::Context::new();

            let profile_data = tera.render("build_default", &profile_context).unwrap();

            write(&profile_path, profile_data)
                .await
                .expect("failed to write sandbox profile");

            darwin::build(
                package_env,
                profile_path.as_path(),
                sandbox_script_path.as_path(),
                sandbox_source_path.as_path(),
            )
            .await?
        }

        Aarch64Linux | X8664Linux => {
            let home_path = sandbox_path.join("home");

            create_dir_all(&home_path)
                .await
                .map_err(|err| anyhow!("failed to create home directory: {:?}", err))?;

            linux::build(
                package_env,
                home_path.as_path(),
                package_path.as_path(),
                packages_paths.clone(),
                package_sandbox_paths,
                sandbox_script_path.as_path(),
                sandbox_source_path.as_path(),
            )
            .await?
        }

        _ => bail!("unknown target"),
    };

    // Run sandbox command

    let mut child = sandbox_command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| anyhow!("failed to spawn sandbox command: {:?}", err))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stdout = LinesStream::new(BufReader::new(stdout).lines());
    let stderr = LinesStream::new(BufReader::new(stderr).lines());

    let mut stdio_merged = StreamExt::merge(stdout, stderr);

    while let Some(line) = stdio_merged.next().await {
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

    let package_path_files = get_file_paths(&package_path, vec![], vec![])?;

    if package_path_files.is_empty() || package_path_files.len() == 1 {
        bail!("no build output files found")
    }

    let message = format!("output files: {}", package_path_files.len());

    send(tx, message).await?;

    // Create package tar from build output files

    if let Err(err) = compress_zstd(&package_path, &package_path_files, &package_archive_path).await
    {
        bail!("failed to compress package tar: {:?}", err)
    }

    let message = format!(
        "package archive created: {}",
        package_archive_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    // Remove lock file

    remove_file(&package_lock_path)
        .await
        .map_err(|err| anyhow!("failed to remove package lock: {:?}", err))?;

    Ok(())
}
