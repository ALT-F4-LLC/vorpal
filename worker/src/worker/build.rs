use crate::worker::{darwin, linux, native};
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
    get_package_system,
    vorpal::{
        package::v0::PackageSystem,
        package::v0::PackageSystem::{
            Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos,
        },
        worker::v0::{BuildRequest, BuildResponse},
    },
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_file_paths, get_package_archive_path, get_package_path,
        get_public_key_path, get_source_archive_path, get_source_path,
    },
    temps::create_temp_dir,
};

async fn send(tx: &Sender<Result<BuildResponse, Status>>, output: String) -> Result<()> {
    debug!("{}", output);

    tx.send(Ok(BuildResponse { output }))
        .await
        .map_err(|err| anyhow!("failed to send response: {:?}", err))?;

    Ok(())
}

pub async fn run(
    request: Request<Streaming<BuildRequest>>,
    tx: &Sender<Result<BuildResponse, Status>>,
) -> Result<()> {
    let mut package_environment = HashMap::new();
    let mut package_name = String::new();
    let mut package_packages = vec![];
    let mut package_sandbox = true;
    let mut package_script = HashMap::new();
    let mut package_source_data: Vec<u8> = vec![];
    let mut package_source_data_chunk = 0;
    let mut package_source_data_signature = None;
    let mut package_source_hash = String::new();
    let mut package_target = UnknownSystem;
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

    if package_target == UnknownSystem {
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

    // TODO: Handle remote source paths

    // Create build environment

    let mut build_bin_paths = vec![];
    let mut build_env = HashMap::new();
    let mut build_packages = vec![];

    for (key, value) in package_environment.clone() {
        build_env.insert(key, value);
    }

    for p in package_packages.iter() {
        let path = get_package_path(&p.hash, &p.name);

        if !path.exists() {
            let message = format!("package missing: {}", path.display());

            bail!(message)
        }

        let bin_path = path.join("bin");

        if bin_path.exists() {
            build_bin_paths.push(bin_path.display().to_string());
        }

        build_env.insert(
            p.name.to_lowercase().replace('-', "_"),
            path.display().to_string(),
        );

        build_packages.push(path.display().to_string());
    }

    // expand environment variables that have package references

    for (key, value) in build_env.clone().into_iter() {
        for package in package_packages.iter() {
            let package_name = package.name.to_lowercase();

            if value.starts_with(&format!("${}", package_name)) {
                let path = get_package_path(&package_name, &package.hash);

                let value =
                    value.replace(&format!("${}", package_name), &path.display().to_string());

                build_env.insert(key.clone(), value);
            }
        }
    }

    send(tx, format!("Build environment: {:?}", build_env)).await?;

    // Setup build path

    let build_path = create_temp_dir().await?;
    let build_path_source = build_path.join("source");

    create_dir_all(&build_path_source)
        .await
        .map_err(|err| anyhow!("failed to create source directory: {:?}", err))?;

    if package_source_path.exists() {
        let build_source_files = get_file_paths(&package_source_path, vec![], vec![])?;

        copy_files(&package_source_path, build_source_files, &build_path_source).await?;
    }

    // Create output directory

    let build_output_path = build_path.join("output");

    create_dir_all(&build_output_path)
        .await
        .map_err(|err| anyhow!("failed to create output directory: {:?}", err))?;

    build_env.insert(
        package_name.to_lowercase().replace('-', "_"),
        package_path.display().to_string(),
    );

    build_env.insert(
        "output".to_string(),
        build_output_path.display().to_string(),
    );

    build_env.insert("packages".to_string(), build_packages.join(" ").to_string());

    let mut build_script_data = String::new();

    for (_, script) in package_script.iter() {
        build_script_data.push_str(script);
    }

    for package in package_packages.iter() {
        let placeholder = format!("${}", package.name);
        let path = get_package_path(&package.hash, &package.name);

        while build_script_data.contains(&placeholder) {
            build_script_data =
                build_script_data.replace(&placeholder, &path.display().to_string());
        }
    }

    build_script_data = build_script_data.replace("$packages", &build_packages.join(" "));

    let build_script_path = build_path.join("package.sh");

    write(&build_script_path, build_script_data)
        .await
        .map_err(|err| anyhow!("failed to write package script: {:?}", err))?;

    set_permissions(&build_script_path, Permissions::from_mode(0o755))
        .await
        .map_err(|err| anyhow!("failed to set permissions: {:?}", err))?;

    let mut sandbox_command = match package_sandbox {
        false => native::build(build_bin_paths, build_env, &build_path).await?,
        true => match worker_target {
            Aarch64Macos | X8664Macos => {
                darwin::build(build_bin_paths, build_env, &build_path).await?
            }
            Aarch64Linux | X8664Linux => {
                linux::build(
                    build_bin_paths.clone(),
                    build_env.clone(),
                    &build_path,
                    build_packages,
                )
                .await?
            }
            _ => bail!("unknown target"),
        },
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

    let build_output_files = get_file_paths(&build_output_path, vec![], vec![])?;

    if build_output_files.is_empty() || build_output_files.len() == 1 {
        bail!("no build output files found")
    }

    let message = format!("output files: {}", build_output_files.len());

    send(tx, message).await?;

    // Create package tar from build output files

    if let Err(err) = compress_zstd(
        &build_output_path,
        &build_output_files,
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
