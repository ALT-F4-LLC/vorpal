use crate::command::store::{
    archives::unpack_zstd,
    paths::{
        get_artifact_alias_path, get_artifact_archive_path, get_artifact_output_path,
        get_file_paths, get_key_ca_path, set_timestamps,
    },
};
use anyhow::{anyhow, bail, Context, Result};
use http::uri::Uri;
use std::{
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};
use tokio::fs::{create_dir_all, read, read_to_string, write};
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig},
    Code, Request,
};
use tracing::{debug, info};
use vorpal_sdk::{
    api::{
        archive::{archive_service_client::ArchiveServiceClient, ArchivePullRequest},
        artifact::{
            artifact_service_client::ArtifactServiceClient, ArtifactSystem, GetArtifactAliasRequest,
        },
    },
    artifact::system::get_system_default,
    context::{client_auth_header, parse_artifact_alias},
};

async fn get_alias_from_registry(
    registry: &str,
    name: &str,
    namespace: &str,
    system: ArtifactSystem,
    tag: &str,
) -> Result<String> {
    let client_ca_pem_path = get_key_ca_path();
    let client_ca_pem = read(&client_ca_pem_path).await.with_context(|| {
        format!(
            "failed to read CA certificate: {}",
            client_ca_pem_path.display()
        )
    })?;
    let client_ca = Certificate::from_pem(client_ca_pem);

    let client_tls = ClientTlsConfig::new()
        .ca_certificate(client_ca)
        .domain_name("localhost");

    let client_uri = registry
        .parse::<Uri>()
        .map_err(|e| anyhow!("invalid registry address: {}", e))?;

    let client_channel = Channel::builder(client_uri)
        .tls_config(client_tls)?
        .connect()
        .await
        .with_context(|| format!("failed to connect to registry: {}", registry))?;

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
    // Setup TLS with CA certificate

    let client_ca_pem_path = get_key_ca_path();
    let client_ca_pem = read(&client_ca_pem_path).await.with_context(|| {
        format!(
            "failed to read CA certificate: {}",
            client_ca_pem_path.display()
        )
    })?;
    let client_ca = Certificate::from_pem(client_ca_pem);

    let client_tls = ClientTlsConfig::new()
        .ca_certificate(client_ca)
        .domain_name("localhost");

    // Connect to registry archive service

    let client_uri = registry
        .parse::<Uri>()
        .map_err(|e| anyhow!("invalid registry address: {}", e))?;

    let client_channel = Channel::builder(client_uri)
        .tls_config(client_tls)?
        .connect()
        .await
        .with_context(|| format!("failed to connect to registry: {}", registry))?;

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

    info!("run: executing {}", binary_path.display());

    // Step 3: Execute the binary, replacing this process

    let err = Command::new(&binary_path).args(args).exec();

    // exec() only returns on error
    bail!("failed to execute {}: {}", binary_path.display(), err,);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// Helper: create the store directory layout for a single artifact.
    ///
    /// Layout under `store_root`:
    /// ```text
    /// store/artifact/alias/{namespace}/{system}/{name}/{tag}   (contains `digest`)
    /// store/artifact/output/{namespace}/{digest}/bin/{binaries...}
    /// ```
    struct StoreFixture {
        _tmp: TempDir,
        pub alias_path: PathBuf,
        pub output_path: PathBuf,
    }

    impl StoreFixture {
        /// Build a fixture with the given parameters. When `digest` is `Some`,
        /// the alias file is created with that content. When `create_output`
        /// is true, the output directory (and `bin/` subdirectory) is created.
        /// `binaries` lists names of dummy executables to place in `output/bin/`.
        fn new(
            name: &str,
            namespace: &str,
            system: &str,
            tag: &str,
            digest: Option<&str>,
            create_output: bool,
            binaries: &[&str],
        ) -> Self {
            let tmp = TempDir::new().expect("failed to create temp dir");
            let root = tmp.path();

            // Build alias path: store/artifact/alias/{namespace}/{system}/{name}/{tag}
            let alias_dir = root
                .join("store")
                .join("artifact")
                .join("alias")
                .join(namespace)
                .join(system)
                .join(name);
            fs::create_dir_all(&alias_dir).expect("failed to create alias dir");
            let alias_path = alias_dir.join(tag);

            if let Some(d) = digest {
                fs::write(&alias_path, d).expect("failed to write alias file");
            }

            // Build output path: store/artifact/output/{namespace}/{digest}/
            let digest_str = digest.unwrap_or("placeholder");
            let output_path = root
                .join("store")
                .join("artifact")
                .join("output")
                .join(namespace)
                .join(digest_str);

            if create_output {
                let bin_dir = output_path.join("bin");
                fs::create_dir_all(&bin_dir).expect("failed to create bin dir");

                for bin_name in binaries {
                    let bin_path = bin_dir.join(bin_name);
                    fs::write(&bin_path, "#!/bin/sh\n").expect("failed to write binary");
                    fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755))
                        .expect("failed to set permissions");
                }
            }

            Self {
                _tmp: tmp,
                alias_path,
                output_path,
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tests for read_alias_digest
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_read_alias_digest_happy_path() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("abc123def456"),
            false,
            &[],
        );

        let digest = read_alias_digest(&fixture.alias_path, "rsync")
            .await
            .expect("should succeed");

        assert_eq!(digest, "abc123def456");
    }

    #[tokio::test]
    async fn test_read_alias_digest_trims_whitespace() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("  abc123  \n"),
            false,
            &[],
        );

        let digest = read_alias_digest(&fixture.alias_path, "rsync")
            .await
            .expect("should succeed");

        assert_eq!(digest, "abc123");
    }

    #[tokio::test]
    async fn test_read_alias_digest_missing_file() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let alias_path = tmp.path().join("nonexistent");

        let result = read_alias_digest(&alias_path, "rsync").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("failed to read alias file"),
            "expected 'failed to read alias file' in: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_read_alias_digest_empty_file() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some(""),
            false,
            &[],
        );

        let result = read_alias_digest(&fixture.alias_path, "rsync").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("alias file is empty"),
            "expected 'alias file is empty' in: {err_msg}"
        );
        assert!(
            err_msg.contains("vorpal build rsync"),
            "expected rebuild hint in: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_read_alias_digest_whitespace_only() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("   \n\t  "),
            false,
            &[],
        );

        let result = read_alias_digest(&fixture.alias_path, "rsync").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("alias file is empty"),
            "expected 'alias file is empty' in: {err_msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Tests for validate_binary_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_binary_name_valid() {
        assert!(validate_binary_name("rsync").is_ok());
        assert!(validate_binary_name("my-tool").is_ok());
        assert!(validate_binary_name("tool_v2").is_ok());
    }

    #[test]
    fn test_validate_binary_name_empty() {
        let result = validate_binary_name("");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("binary name cannot be empty"),
            "expected 'binary name cannot be empty' in: {err_msg}"
        );
    }

    #[test]
    fn test_validate_binary_name_forward_slash() {
        let result = validate_binary_name("../../etc/passwd");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("must be a plain filename without path separators"),
            "expected path separator error in: {err_msg}"
        );
        // Should suggest just the filename portion
        assert!(
            err_msg.contains("--bin passwd"),
            "expected suggested fix in: {err_msg}"
        );
    }

    #[test]
    fn test_validate_binary_name_backslash() {
        let result = validate_binary_name("..\\..\\etc\\passwd");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("must be a plain filename without path separators"),
            "expected path separator error in: {err_msg}"
        );
    }

    #[test]
    fn test_validate_binary_name_absolute_path() {
        let result = validate_binary_name("/etc/passwd");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("must be a plain filename without path separators"),
            "expected path separator error in: {err_msg}"
        );
    }

    #[test]
    fn test_validate_binary_name_dotfile() {
        let result = validate_binary_name(".hidden");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("must not start with '.'"),
            "expected dotfile error in: {err_msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Tests for resolve_binary
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_binary_happy_path() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("abc123"),
            true,
            &["rsync"],
        );

        let binary_path = resolve_binary(&fixture.output_path, "rsync")
            .await
            .expect("should resolve binary");

        assert_eq!(binary_path, fixture.output_path.join("bin").join("rsync"));
        assert!(binary_path.exists());
    }

    #[tokio::test]
    async fn test_resolve_binary_missing_binary_lists_available() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("abc123"),
            true,
            &["rsync3", "rsync-ssl"],
        );

        let result = resolve_binary(&fixture.output_path, "rsync").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("binary 'rsync' not found"),
            "expected 'binary not found' in: {err_msg}"
        );
        assert!(
            err_msg.contains("rsync3"),
            "expected available binary 'rsync3' in: {err_msg}"
        );
        assert!(
            err_msg.contains("rsync-ssl"),
            "expected available binary 'rsync-ssl' in: {err_msg}"
        );
        // Verify the --bin hint (vorpal-njf)
        assert!(
            err_msg.contains("Use --bin <name> to select a different binary"),
            "expected --bin hint in: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_resolve_binary_missing_binary_no_bin_dir() {
        // Create output dir but no bin/ subdirectory
        let tmp = TempDir::new().expect("failed to create temp dir");
        let output_path = tmp.path().join("output");
        fs::create_dir_all(&output_path).expect("failed to create output dir");

        let result = resolve_binary(&output_path, "rsync").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("binary 'rsync' not found"),
            "expected 'binary not found' in: {err_msg}"
        );
        assert!(
            err_msg.contains("no bin/ directory"),
            "expected 'no bin/ directory' in: {err_msg}"
        );
        // Should NOT contain --bin hint when there are no available binaries
        assert!(
            !err_msg.contains("Use --bin"),
            "should not contain --bin hint when no bin/ dir: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_resolve_binary_empty_bin_dir() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("abc123"),
            true,
            &[], // no binaries
        );

        let result = resolve_binary(&fixture.output_path, "rsync").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("bin/ directory exists but contains no files"),
            "expected 'bin/ directory exists but contains no files' in: {err_msg}"
        );
        // Should NOT contain --bin hint when bin/ is empty
        assert!(
            !err_msg.contains("Use --bin"),
            "should not contain --bin hint when bin/ is empty: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_resolve_binary_not_executable() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let output_path = tmp.path().join("output");
        let bin_dir = output_path.join("bin");
        fs::create_dir_all(&bin_dir).expect("failed to create bin dir");

        let bin_path = bin_dir.join("rsync");
        fs::write(&bin_path, "#!/bin/sh\n").expect("failed to write binary");
        // Set permissions to read-only (no execute)
        fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o644))
            .expect("failed to set permissions");

        let result = resolve_binary(&output_path, "rsync").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("binary is not executable"),
            "expected 'binary is not executable' in: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_resolve_binary_rejects_path_traversal() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("abc123"),
            true,
            &["rsync"],
        );

        let result = resolve_binary(&fixture.output_path, "../../etc/passwd").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("must be a plain filename without path separators"),
            "expected path separator error in: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_resolve_binary_rejects_empty_name() {
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some("abc123"),
            true,
            &["rsync"],
        );

        let result = resolve_binary(&fixture.output_path, "").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("binary name cannot be empty"),
            "expected 'binary name cannot be empty' in: {err_msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Tests for tag resolution (different tags resolve to different digests)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_tag_resolution_latest_vs_versioned() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let root = tmp.path();

        let alias_dir = root
            .join("store")
            .join("artifact")
            .join("alias")
            .join("library")
            .join("AARCH64_DARWIN")
            .join("rsync");
        fs::create_dir_all(&alias_dir).expect("failed to create alias dir");

        // Create two different tag files pointing to different digests
        let latest_path = alias_dir.join("latest");
        fs::write(&latest_path, "digest_latest_abc").expect("failed to write latest");

        let versioned_path = alias_dir.join("3.4.1");
        fs::write(&versioned_path, "digest_341_xyz").expect("failed to write 3.4.1");

        // Verify they resolve to different digests
        let latest_digest = read_alias_digest(&latest_path, "rsync")
            .await
            .expect("should resolve latest");
        let versioned_digest = read_alias_digest(&versioned_path, "rsync")
            .await
            .expect("should resolve 3.4.1");

        assert_eq!(latest_digest, "digest_latest_abc");
        assert_eq!(versioned_digest, "digest_341_xyz");
        assert_ne!(latest_digest, versioned_digest);
    }

    // -----------------------------------------------------------------------
    // End-to-end resolution test (alias -> digest -> binary)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_full_resolution_alias_to_binary() {
        let digest = "sha256_abc123def456";
        let fixture = StoreFixture::new(
            "rsync",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some(digest),
            true,
            &["rsync"],
        );

        // Step 1: Read alias
        let resolved_digest = read_alias_digest(&fixture.alias_path, "rsync")
            .await
            .expect("should read alias");
        assert_eq!(resolved_digest, digest);

        // Step 2: Resolve binary
        let binary_path = resolve_binary(&fixture.output_path, "rsync")
            .await
            .expect("should resolve binary");

        assert!(binary_path.exists());
        assert!(binary_path.ends_with("bin/rsync"));

        // Verify the binary is executable
        let meta = fs::metadata(&binary_path).expect("should read metadata");
        assert!(
            meta.permissions().mode() & 0o111 != 0,
            "binary should be executable"
        );
    }

    #[tokio::test]
    async fn test_full_resolution_with_custom_bin() {
        let digest = "sha256_abc123";
        let fixture = StoreFixture::new(
            "my-tool",
            "library",
            "AARCH64_DARWIN",
            "latest",
            Some(digest),
            true,
            &["my-tool", "my-tool-helper"],
        );

        // Resolve alias
        let resolved_digest = read_alias_digest(&fixture.alias_path, "my-tool")
            .await
            .expect("should read alias");
        assert_eq!(resolved_digest, digest);

        // Resolve the helper binary via --bin equivalent
        let binary_path = resolve_binary(&fixture.output_path, "my-tool-helper")
            .await
            .expect("should resolve helper binary");

        assert!(binary_path.exists());
        assert!(binary_path.ends_with("bin/my-tool-helper"));
    }
}
