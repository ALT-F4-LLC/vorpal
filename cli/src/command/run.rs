use crate::command::store::{
    archives::unpack_zstd,
    paths::{
        get_artifact_alias_path, get_artifact_archive_path, get_artifact_output_path,
        get_file_paths, set_timestamps,
    },
};
use anyhow::{anyhow, bail, Context, Result};
use std::{
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};
use tokio::fs::{create_dir_all, read_to_string, write};
use tonic::{Code, Request};
use tracing::{debug, info};
use vorpal_sdk::{
    api::{
        archive::{archive_service_client::ArchiveServiceClient, ArchivePullRequest},
        artifact::{
            artifact_service_client::ArtifactServiceClient, ArtifactSystem, GetArtifactAliasRequest,
        },
    },
    artifact::system::get_system_default,
    context::{build_channel, client_auth_header, parse_artifact_alias},
};

async fn get_alias_from_registry(
    registry: &str,
    name: &str,
    namespace: &str,
    system: ArtifactSystem,
    tag: &str,
) -> Result<String> {
    let client_channel = build_channel(registry).await?;
    let mut client = ArtifactServiceClient::new(client_channel);

    let request = GetArtifactAliasRequest {
        system: system.into(),
        name: name.to_string(),
        namespace: namespace.to_string(),
        tag: tag.to_string(),
    };

    let mut request = Request::new(request);
    let request_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

    if let Some(header) = request_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    let response = client.get_artifact_alias(request).await.map_err(|status| {
        if status.code() == Code::NotFound {
            anyhow!("alias not found in registry")
        } else {
            anyhow!("registry error: {:?}", status)
        }
    })?;

    let digest = response.into_inner().digest;

    if digest.is_empty() {
        bail!("registry returned empty digest for alias");
    }

    Ok(digest)
}

async fn read_alias_digest(alias_path: &Path, artifact_name: &str) -> Result<String> {
    let artifact_digest = read_to_string(alias_path)
        .await
        .with_context(|| format!("failed to read alias file: {}", alias_path.display()))?;

    let artifact_digest = artifact_digest.trim().to_string();

    if artifact_digest.is_empty() {
        bail!(
            "alias file is empty: {}\n\
             \n\
             The alias file exists but contains no digest. Try rebuilding:\n\
             \n\
             \tvorpal build {}",
            alias_path.display(),
            artifact_name,
        );
    }

    Ok(artifact_digest)
}

async fn pull_artifact_from_registry(
    registry: &str,
    digest: &str,
    namespace: &str,
    output_path: &std::path::Path,
) -> Result<()> {
    let client_channel = build_channel(registry).await?;
    let mut client_archive = ArchiveServiceClient::new(client_channel);

    // Pull archive from registry

    let request = ArchivePullRequest {
        digest: digest.to_string(),
        namespace: namespace.to_string(),
    };

    let mut request = Request::new(request);
    let request_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

    if let Some(header) = request_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    let response = match client_archive.pull(request).await {
        Ok(response) => response,

        Err(status) => {
            if status.code() == Code::NotFound {
                bail!("artifact not found in registry");
            }

            bail!("registry pull error: {:?}", status);
        }
    };

    let mut stream = response.into_inner();
    let mut stream_data = Vec::new();

    loop {
        match stream.message().await {
            Ok(Some(chunk)) => {
                if !chunk.data.is_empty() {
                    stream_data.extend_from_slice(&chunk.data);
                }
            }

            Ok(None) => break,

            Err(status) => {
                if status.code() == Code::NotFound {
                    bail!("artifact not found in registry");
                }

                bail!("registry stream error: {:?}", status);
            }
        }
    }

    if stream_data.is_empty() {
        bail!("registry returned empty archive for digest: {digest}");
    }

    // Write archive to local store

    let archive_path = get_artifact_archive_path(digest, namespace);

    let archive_path_parent = archive_path
        .parent()
        .ok_or_else(|| anyhow!("failed to get archive parent path"))?;

    create_dir_all(archive_path_parent).await?;

    write(&archive_path, &stream_data)
        .await
        .with_context(|| format!("failed to write archive: {}", archive_path.display()))?;

    set_timestamps(&archive_path).await?;

    // Unpack archive to output path

    info!("unpacking artifact: {digest}");

    create_dir_all(output_path).await.with_context(|| {
        format!(
            "failed to create output directory: {}",
            output_path.display()
        )
    })?;

    unpack_zstd(&output_path.to_path_buf(), &archive_path).await?;

    let artifact_files = get_file_paths(&output_path.to_path_buf(), vec![], vec![])?;

    for file in &artifact_files {
        set_timestamps(file).await?;
    }

    Ok(())
}

fn validate_binary_name(binary_name: &str) -> Result<()> {
    if binary_name.is_empty() {
        bail!(
            "binary name cannot be empty\n\
             \n\
             Provide a non-empty value for --bin, or omit it to use the artifact name."
        );
    }

    if binary_name.contains('/') || binary_name.contains('\\') {
        bail!(
            "invalid binary name '{}': must be a plain filename without path separators\n\
             \n\
             Use just the binary name, for example: --bin {}",
            binary_name,
            binary_name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(binary_name),
        );
    }

    if binary_name.starts_with('.') {
        bail!(
            "invalid binary name '{}': must not start with '.'",
            binary_name,
        );
    }

    Ok(())
}

async fn resolve_binary(output_path: &Path, binary_name: &str) -> Result<PathBuf> {
    validate_binary_name(binary_name)?;

    let bin_dir = output_path.join("bin");
    let binary_path = bin_dir.join(binary_name);

    if !binary_path.exists() {
        let mut message = format!(
            "binary '{}' not found in artifact output\n\
             \n\
             Expected binary at: {}",
            binary_name,
            binary_path.display(),
        );

        // List available binaries if the bin/ directory exists
        if bin_dir.is_dir() {
            let mut available: Vec<String> = Vec::new();

            if let Ok(mut entries) = tokio::fs::read_dir(&bin_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(ft) = entry.file_type().await {
                        if ft.is_file() {
                            available.push(entry.file_name().to_string_lossy().to_string());
                        }
                    }
                }
            }

            available.sort();

            if available.is_empty() {
                message.push_str("\n\nThe bin/ directory exists but contains no files.");
            } else {
                message.push_str("\n\nAvailable binaries:");
                for name in &available {
                    message.push_str(&format!("\n\t{name}"));
                }
                message.push_str("\n\nUse --bin <name> to select a different binary.");
            }
        } else {
            message.push_str("\n\nThe artifact output has no bin/ directory.");
        }

        bail!("{message}");
    }

    let metadata = tokio::fs::metadata(&binary_path)
        .await
        .with_context(|| format!("failed to read metadata for: {}", binary_path.display()))?;

    if metadata.permissions().mode() & 0o111 == 0 {
        bail!(
            "binary is not executable: {}\n\
             \n\
             The file exists but does not have execute permissions.",
            binary_path.display(),
        );
    }

    Ok(binary_path)
}

pub async fn run(alias: &str, args: &[String], bin: Option<&str>, registry: &str) -> Result<()> {
    let alias_parsed = parse_artifact_alias(alias)?;
    let system = get_system_default()?;

    debug!(
        "run: name={}, namespace={}, system={}, tag={}",
        alias_parsed.name,
        alias_parsed.namespace,
        system.as_str_name(),
        alias_parsed.tag,
    );

    // Step 1: Resolve artifact alias to digest

    let alias_path = get_artifact_alias_path(
        &alias_parsed.name,
        &alias_parsed.namespace,
        system,
        &alias_parsed.tag,
    )?;

    if !alias_path.exists() {
        info!("alias not found locally, checking registry: {registry}");

        match get_alias_from_registry(
            registry,
            &alias_parsed.name,
            &alias_parsed.namespace,
            system,
            &alias_parsed.tag,
        )
        .await
        {
            Ok(digest) => {
                info!("alias resolved from registry: digest={digest}");

                if let Some(parent) = alias_path.parent() {
                    create_dir_all(parent).await.with_context(|| {
                        format!("failed to create alias directory: {}", parent.display())
                    })?;
                }

                write(&alias_path, digest.as_bytes())
                    .await
                    .with_context(|| {
                        format!("failed to write alias file: {}", alias_path.display())
                    })?;
            }

            Err(err) => {
                debug!("registry alias lookup failed: {err}");

                bail!(
                    "artifact alias not found: {}\n\
                     \n\
                     The alias file does not exist at: {}\n\
                     \n\
                     The alias could not be resolved from the registry:\n\
                     \n\
                     \t{err}\n\
                     \n\
                     Have you built this artifact? Try:\n\
                     \n\
                     \tvorpal build {}",
                    alias,
                    alias_path.display(),
                    alias_parsed.name,
                );
            }
        }
    }

    let artifact_digest = read_alias_digest(&alias_path, &alias_parsed.name).await?;

    debug!("run: resolved digest={artifact_digest}");

    // Step 2: Locate the binary

    let output_path = get_artifact_output_path(&artifact_digest, &alias_parsed.namespace);

    if !output_path.exists() {
        info!("artifact output not found locally, attempting pull from registry");

        let pulled = pull_artifact_from_registry(
            registry,
            &artifact_digest,
            &alias_parsed.namespace,
            &output_path,
        )
        .await;

        match pulled {
            Ok(()) => {
                info!("artifact pulled from registry successfully");
            }

            Err(err) => {
                debug!("registry pull failed: {err}");

                bail!(
                    "artifact output not found for digest: {artifact_digest}\n\
                     \n\
                     The output directory does not exist at: {}\n\
                     \n\
                     The artifact could not be pulled from the registry:\n\
                     \n\
                     \t{err}\n\
                     \n\
                     The artifact may need to be rebuilt:\n\
                     \n\
                     \tvorpal build {}",
                    output_path.display(),
                    alias_parsed.name,
                );
            }
        }
    }

    let binary_name = bin.unwrap_or(&alias_parsed.name);
    let binary_path = resolve_binary(&output_path, binary_name).await?;

    // Step 3: Execute the binary, replacing this process

    let err = Command::new(&binary_path).args(args).exec();

    // exec() only returns on error
    bail!("failed to execute {}: {}", binary_path.display(), err,);
}
