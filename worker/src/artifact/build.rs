use anyhow::{anyhow, bail, Result};
use rsa::{
    pss::{Signature, VerifyingKey},
    sha2::Sha256,
    signature::Verifier,
};
use std::{
    env::consts::{ARCH, OS},
    fs::Permissions,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Stdio,
};
use tokio::process::Command;
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
        artifact::v0::ArtifactSystem::UnknownSystem,
        artifact::v0::{
            ArtifactBuildRequest, ArtifactBuildResponse, ArtifactEnvironment, ArtifactId,
        },
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

#[allow(clippy::too_many_arguments)]
pub async fn run_step(
    arguments: Vec<String>,
    artifact_path: &Path,
    artifacts: Vec<ArtifactId>,
    entrypoint: String,
    environments: Vec<ArtifactEnvironment>,
    name: String,
    script: Option<String>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
    workspace_path: &Path,
) -> Result<()> {
    let mut envs = vec![];

    // Add all artifact environment variables

    let mut paths = vec![];

    for a in artifacts {
        let path = get_artifact_path(&a.hash, &a.name);

        if !path.exists() {
            bail!(format!("artifact missing: {}", path.display()))
        }

        envs.push(ArtifactEnvironment {
            key: format!(
                "VORPAL_ARTIFACT_{}",
                a.name.to_lowercase().replace('-', "_")
            ),
            value: path.display().to_string(),
        });

        paths.push(path.display().to_string());
    }

    // Add default environment variables

    let name_envkey = name.to_lowercase().replace('-', "_");

    envs.push(ArtifactEnvironment {
        key: format!("VORPAL_ARTIFACT_{}", name_envkey.clone()),
        value: artifact_path.display().to_string(),
    });

    envs.push(ArtifactEnvironment {
        key: "VORPAL_ARTIFACTS".to_string(),
        value: paths.join(" ").to_string(),
    });

    envs.push(ArtifactEnvironment {
        key: "VORPAL_OUTPUT".to_string(),
        value: artifact_path.display().to_string(),
    });

    envs.push(ArtifactEnvironment {
        key: "VORPAL_WORKSPACE".to_string(),
        value: workspace_path.display().to_string(),
    });

    // Add all custom environment variables

    for e in environments.clone() {
        envs.push(e);
    }

    // Sort environment variables by key length

    let mut envs_sorted = envs.to_vec();

    envs_sorted.sort_by(|a, b| b.key.len().cmp(&a.key.len()));

    // Setup command

    let mut command = Command::new(&entrypoint);

    // Setup working directory

    command.current_dir(workspace_path);

    // Setup environment variables

    for env in envs_sorted.clone() {
        command.env(env.key, env.value);
    }

    // Setup arguments

    for arg in arguments.into_iter() {
        let mut arg = arg.clone();

        for env in envs_sorted.clone() {
            arg = arg.replace(&format!("${}", env.key), &env.value);
        }

        command.arg(arg);
    }

    // Setup script

    let mut script_path = None;

    if let Some(script) = script {
        let mut script = script.clone();

        for env in envs_sorted.clone() {
            script = script.replace(&format!("${}", env.key), &env.value);
        }

        let path = workspace_path.join("script.sh");

        write(&path, script.clone())
            .await
            .map_err(|err| anyhow!("failed to write script: {:?}", err))?;

        set_permissions(&path, Permissions::from_mode(0o755))
            .await
            .map_err(|err| anyhow!("failed to set permissions: {:?}", err))?;

        script_path = Some(path);
    }

    if let Some(script_path) = script_path {
        command.arg(script_path);
    }

    // Run command

    let mut child = command
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

    Ok(())
}

pub async fn run(
    workspace_path: &Path,
    request: Request<Streaming<ArtifactBuildRequest>>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
) -> Result<()> {
    let mut artifact_artifacts = vec![];
    let mut artifact_hash = String::new();
    let mut artifact_name = String::new();
    let mut artifact_source_data: Vec<u8> = vec![];
    let mut artifact_source_data_chunk = 0;
    let mut artifact_source_data_signature = None;
    let mut artifact_steps = vec![];
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
        artifact_hash = result.hash;
        artifact_name = result.name;
        artifact_source_data_signature = result.source_data_signature;
        artifact_steps = result.steps;
        artifact_target = ArtifactSystem::try_from(result.target)
            .map_err(|err| anyhow!("failed to parse target: {:?}", err))?;
    }

    // Check if required fields are present

    if artifact_name.is_empty() {
        bail!("'name' missing in configuration")
    }

    if artifact_hash.is_empty() {
        bail!("'source_hash' is missing in configuration")
    }

    if artifact_steps.is_empty() {
        bail!("'steps' missing in configuration")
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

    let artifact_lock_path = get_artifact_lock_path(&artifact_hash, &artifact_name);

    if artifact_lock_path.exists() {
        bail!("artifact is locked") // TODO: figure out better way to handle this (e.g. prompt, ui, etc)
    }

    // If artifact exists, return

    let artifact_path = get_artifact_path(&artifact_hash, &artifact_name);

    if artifact_path.exists() {
        send(tx, artifact_path.display().to_string()).await?;

        return Ok(());
    }

    // If artifact archive exists, unpack it to artifact path

    let artifact_archive_path = get_artifact_archive_path(&artifact_hash, &artifact_name);

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

    let source_archive_path = get_source_archive_path(&artifact_hash, &artifact_name);

    let source_path = get_source_path(&artifact_hash, &artifact_name);

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

        let source_archive_path = get_source_archive_path(&artifact_hash, &artifact_name);

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

    // Setup sandbox path source

    let sandbox_source_path = workspace_path.join("source");

    create_dir_all(&sandbox_source_path)
        .await
        .map_err(|err| anyhow!("failed to create source directory: {:?}", err))?;

    if source_path.exists() {
        let source_files = get_file_paths(&source_path, vec![], vec![])?;

        copy_files(&source_path, source_files, &sandbox_source_path).await?;
    }

    // Run artifact steps

    for step in artifact_steps.into_iter() {
        run_step(
            step.arguments.clone(),
            &artifact_path,
            artifact_artifacts.clone(),
            step.entrypoint,
            step.environments,
            artifact_name.clone(),
            step.script,
            tx,
            workspace_path,
        )
        .await?;
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
