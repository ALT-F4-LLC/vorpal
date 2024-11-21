use crate::artifact::{darwin, darwin::profile, linux, native};
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
    get_artifact_system,
    vorpal::{
        artifact::v0::ArtifactSystem,
        artifact::v0::ArtifactSystem::{
            Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos,
        },
        artifact::v0::{ArtifactBuildRequest, ArtifactBuildResponse},
    },
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_artifact_archive_path, get_artifact_lock_path, get_artifact_path,
        get_file_paths, get_public_key_path, get_source_archive_path, get_source_path,
    },
};

async fn send(tx: &Sender<Result<ArtifactBuildResponse, Status>>, output: String) -> Result<()> {
    debug!("{}", output);

    tx.send(Ok(ArtifactBuildResponse { output }))
        .await
        .map_err(|err| anyhow!("failed to send response: {:?}", err))?;

    Ok(())
}

pub async fn run(
    sandbox_path: &Path,
    request: Request<Streaming<ArtifactBuildRequest>>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
) -> Result<()> {
    let mut artifact_environments = vec![];
    let mut artifact_name = String::new();
    let mut artifact_artifacts = vec![];
    let mut artifact_sandbox = None;
    let mut artifact_script = String::new();
    let mut artifact_source_data: Vec<u8> = vec![];
    let mut artifact_source_data_chunk = 0;
    let mut artifact_source_data_signature = None;
    let mut artifact_source_hash = String::new();
    let mut artifact_target = UnknownSystem;
    let mut request_stream = request.into_inner();

    // Parse request stream

    while let Some(result) = request_stream.next().await {
        let result = result.map_err(|err| anyhow!("failed to parse request: {:?}", err))?;

        if let Some(data) = result.source_data {
            artifact_source_data_chunk += 1;
            artifact_source_data.extend_from_slice(&data);
        }

        artifact_artifacts = result.artifacts;
        artifact_environments = result.environments;
        artifact_name = result.name;
        artifact_sandbox = result.sandbox;
        artifact_script = result.script;
        artifact_source_data_signature = result.source_data_signature;
        artifact_source_hash = result.source_hash;
        artifact_target = ArtifactSystem::try_from(result.target)
            .map_err(|err| anyhow!("failed to parse target: {:?}", err))?;
    }

    // Check if required fields are present

    if artifact_name.is_empty() {
        bail!("'name' missing in configuration")
    }

    if artifact_script.is_empty() {
        bail!("'script' missing in configuration")
    }

    if artifact_source_hash.is_empty() {
        bail!("'source_hash' is missing in configuration")
    }

    if artifact_target == UnknownSystem {
        bail!("'target' missing in configuration")
    }

    // Check if worker target matches artifact target

    let worker_system = format!("{}-{}", ARCH, OS);

    let worker_target = get_artifact_system::<ArtifactSystem>(worker_system.as_str());

    if artifact_target != worker_target {
        bail!("'target' mismatch")
    }

    // Check if artifact is locked

    let artifact_lock_path = get_artifact_lock_path(&artifact_source_hash, &artifact_name);

    if artifact_lock_path.exists() {
        bail!("artifact is locked") // TODO: figure out better way to handle this (e.g. prompt, ui, etc)
    }

    // If artifact exists, return

    let artifact_path = get_artifact_path(&artifact_source_hash, &artifact_name);

    if artifact_path.exists() {
        send(tx, artifact_path.display().to_string()).await?;

        return Ok(());
    }

    // If artifact archive exists, unpack it to artifact path

    let artifact_archive_path = get_artifact_archive_path(&artifact_source_hash, &artifact_name);

    if artifact_archive_path.exists() {
        send(tx, artifact_archive_path.display().to_string()).await?;

        create_dir_all(&artifact_path)
            .await
            .map_err(|err| anyhow!("failed to create artifact directory: {:?}", err))?;

        if let Err(err) = unpack_zstd(&artifact_path, &artifact_archive_path).await {
            bail!("failed to unpack artifact archive: {:?}", err)
        }

        send(tx, artifact_path.display().to_string()).await?;

        return Ok(());
    }

    // create artifact directory and lock file to prevent concurrent builds

    create_dir_all(&artifact_path)
        .await
        .map_err(|err| anyhow!("failed to create artifact directory: {:?}", err))?;

    // TODO: add metadata to the lockfile to know how to clean up

    write(&artifact_lock_path, "")
        .await
        .map_err(|err| anyhow!("failed to write artifact lock: {:?}", err))?;

    // Check if source archive is present

    let source_archive_path = get_source_archive_path(&artifact_source_hash, &artifact_name);

    let source_path = get_source_path(&artifact_source_hash, &artifact_name);

    if source_archive_path.exists() {
        create_dir_all(&source_path)
            .await
            .map_err(|err| anyhow!("failed to create source directory: {:?}", err))?;

        unpack_zstd(&source_path, &source_archive_path).await?;

        send(tx, source_path.display().to_string()).await?;
    }

    // Check if source data is present and verify signature

    if !source_path.exists() && !artifact_source_data.is_empty() {
        send(tx, format!("Source chunks: {}", artifact_source_data_chunk)).await?;

        // Verify source data signature

        match artifact_source_data_signature {
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

                if let Err(msg) = verifying_key.verify(&artifact_source_data, &signature) {
                    bail!("failed to verify signature: {:?}", msg)
                }
            }
        }

        let source_archive_path = get_source_archive_path(&artifact_source_hash, &artifact_name);

        if source_archive_path.exists() {
            bail!("source archive already exists")
        }

        write(&source_archive_path, &artifact_source_data)
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

    let mut artifacts_paths = vec![];
    let mut artifact_env = HashMap::new();

    // Add artifact environment variables

    for env in artifact_environments.clone() {
        artifact_env.insert(env.key, env.value);
    }

    // Add artifact environment variables and paths

    for p in artifact_artifacts.iter() {
        let path = get_artifact_path(&p.hash, &p.name);

        if !path.exists() {
            let message = format!("artifact missing: {}", path.display());

            bail!(message)
        }

        artifacts_paths.push(path.display().to_string());

        artifact_env.insert(
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

    // Add artifact(s) environment variables

    let artifact_env_name = artifact_name.to_lowercase().replace('-', "_");

    artifact_env.insert(
        artifact_env_name.clone(),
        artifact_path.display().to_string(),
    );

    artifact_env.insert("output".to_string(), artifact_path.display().to_string());

    artifact_env.insert(
        "artifacts".to_string(),
        artifacts_paths.join(" ").to_string(),
    );

    // Write artifact script

    let sandbox_script_path = sandbox_path.join("artifact.sh");

    write(&sandbox_script_path, artifact_script.clone())
        .await
        .map_err(|err| anyhow!("failed to write artifact script: {:?}", err))?;

    set_permissions(&sandbox_script_path, Permissions::from_mode(0o755))
        .await
        .map_err(|err| anyhow!("failed to set permissions: {:?}", err))?;

    // Create sandbox command

    let mut sandbox_command = match artifact_sandbox {
        None => {
            native::build(
                artifact_env,
                sandbox_script_path.as_path(),
                sandbox_source_path.as_path(),
            )
            .await?
        }

        Some(sandbox_artifact) => match worker_target {
            Aarch64Macos | X8664Macos => {
                let profile_path = sandbox_path.join("artifact.sb");

                let mut tera = Tera::default();

                tera.add_raw_template("build_default", profile::STDENV_DEFAULT)
                    .unwrap();

                let profile_context = tera::Context::new();

                let profile_data = tera.render("build_default", &profile_context).unwrap();

                write(&profile_path, profile_data)
                    .await
                    .expect("failed to write sandbox profile");

                darwin::build(
                    artifact_env,
                    profile_path.as_path(),
                    sandbox_script_path.as_path(),
                    sandbox_source_path.as_path(),
                )
                .await?
            }

            Aarch64Linux | X8664Linux => {
                let sandbox_artifact_path =
                    get_artifact_path(&sandbox_artifact.hash, &sandbox_artifact.name);

                let home_path = sandbox_path.join("home");

                create_dir_all(&home_path)
                    .await
                    .map_err(|err| anyhow!("failed to create home directory: {:?}", err))?;

                linux::build(
                    artifact_env,
                    home_path.as_path(),
                    artifact_path.as_path(),
                    artifacts_paths.clone(),
                    sandbox_artifact_path.as_path(),
                    sandbox_script_path.as_path(),
                    sandbox_source_path.as_path(),
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
        bail!("failed to build artifact")
    }

    // Check for output files

    let artifact_path_files = get_file_paths(&artifact_path, vec![], vec![])?;

    if artifact_path_files.is_empty() || artifact_path_files.len() == 1 {
        bail!("no build output files found")
    }

    let message = format!("output files: {}", artifact_path_files.len());

    send(tx, message).await?;

    // Create artifact tar from build output files

    if let Err(err) =
        compress_zstd(&artifact_path, &artifact_path_files, &artifact_archive_path).await
    {
        bail!("failed to compress artifact tar: {:?}", err)
    }

    let message = format!(
        "artifact archive created: {}",
        artifact_archive_path.file_name().unwrap().to_str().unwrap()
    );

    send(tx, message).await?;

    // Remove lock file

    remove_file(&artifact_lock_path)
        .await
        .map_err(|err| anyhow!("failed to remove artifact lock: {:?}", err))?;

    Ok(())
}