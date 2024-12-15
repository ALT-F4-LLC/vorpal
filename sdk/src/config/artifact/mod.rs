use crate::config::{
    artifact::{
        steps::{bash, bwrap},
        toolchain::linux::{debian, vorpal},
    },
    ConfigContext,
};
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use sha256::digest;
use std::path::Path;
use tokio::fs::{create_dir_all, remove_file, write};
use tokio_tar::Archive;
use tracing::info;
use url::Url;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId, ArtifactSource, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};
use vorpal_store::{archives::unpack_zip, paths::get_file_paths, temps::create_sandbox_file};

pub mod language;
pub mod shell;
pub mod steps;
pub mod toolchain;

pub fn get_artifact_envkey(artifact: &ArtifactId) -> String {
    format!(
        "$VORPAL_ARTIFACT_{}",
        artifact.name.to_lowercase().replace("-", "_")
    )
    .to_string()
}

#[derive(Debug, PartialEq)]
pub enum ArtifactSourceKind {
    UnknownSourceKind,
    Git,
    Http,
    Local,
}

pub async fn add_artifact_source(
    context: &mut ConfigContext,
    source: ArtifactSource,
) -> Result<ArtifactSource> {
    let mut source_path = None;

    let source_path_kind = match &source.path {
        s if Path::new(s).exists() => ArtifactSourceKind::Local,
        s if s.starts_with("git") => ArtifactSourceKind::Git,
        s if s.starts_with("http") => ArtifactSourceKind::Http,
        _ => ArtifactSourceKind::UnknownSourceKind,
    };

    if source_path_kind == ArtifactSourceKind::UnknownSourceKind {
        bail!(
            "`source.{}.path` unknown source kind: {:?}",
            source.name,
            source.path
        );
    }

    if source_path_kind == ArtifactSourceKind::Git {
        bail!(
            "`source.{}.path` git source kind not supported",
            source.name
        );
    }

    if source_path_kind == ArtifactSourceKind::Http {
        if source.hash.is_none() {
            bail!(
                "`source.{}.hash` required for HTTP sources: {:?}",
                source.name,
                source.path
            );
        }

        if source.hash.is_some() && source.hash.clone().unwrap() == "" {
            bail!(
                "`source.{}.hash` empty for HTTP sources: {:?}",
                source.name,
                source.path
            );
        }

        // TODO: support sources being stored in registry

        let source_cache_hash = digest(source.path.as_bytes());
        let source_cache_path = format!("/tmp/vorpal-source-{}", source_cache_hash);
        let source_cache_path = Path::new(&source_cache_path).to_path_buf();

        if !source_cache_path.exists() {
            let source_uri = Url::parse(&source.path).map_err(|e| anyhow::anyhow!(e))?;

            if source_uri.scheme() != "http" && source_uri.scheme() != "https" {
                anyhow::bail!("source URL scheme not supported: {:?}", source_uri.scheme());
            }

            info!(
                "[{}] fetching source... ({})",
                source.name,
                source_uri.as_str()
            );

            let source_response = reqwest::get(source_uri.as_str())
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            if !source_response.status().is_success() {
                anyhow::bail!("source URL not failed: {:?}", source_response.status());
            }

            let source_response_bytes = source_response
                .bytes()
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            let source_response_bytes = source_response_bytes.as_ref();

            create_dir_all(&source_cache_path)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            if let Some(kind) = infer::get(source_response_bytes) {
                match kind.mime_type() {
                    "application/gzip" => {
                        let decoder = GzipDecoder::new(source_response_bytes);
                        let mut archive = Archive::new(decoder);

                        info!(
                            "[{}] unpacking gzip... ({})",
                            source.name,
                            source_cache_path.display(),
                        );

                        archive
                            .unpack(&source_cache_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    "application/x-bzip2" => {
                        let decoder = BzDecoder::new(source_response_bytes);
                        let mut archive = Archive::new(decoder);

                        info!(
                            "[{}] unpacking bzip2... ({})",
                            source.name,
                            source_cache_path.display(),
                        );

                        archive
                            .unpack(&source_cache_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    "application/x-xz" => {
                        let decoder = XzDecoder::new(source_response_bytes);
                        let mut archive = Archive::new(decoder);

                        info!(
                            "[{}] unpacking xz... ({})",
                            source.name,
                            source_cache_path.display(),
                        );

                        archive
                            .unpack(&source_cache_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    "application/zip" => {
                        let sandbox_file_path = create_sandbox_file(Some("zip")).await?;

                        write(&sandbox_file_path, source_response_bytes)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;

                        info!(
                            "[{}] unpacking zip... ({})",
                            source.name,
                            source_cache_path.display(),
                        );

                        unpack_zip(&sandbox_file_path, &source_cache_path).await?;

                        remove_file(&sandbox_file_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    _ => {
                        bail!(
                            "`source.{}.path` unsupported mime-type detected: {:?}",
                            source.name,
                            source.path
                        );
                    }
                }
            }
        }

        source_path = Some(source_cache_path);
    }

    if source_path_kind == ArtifactSourceKind::Local {
        source_path = Some(Path::new(&source.path).to_path_buf());
    }

    if source_path.is_none() {
        bail!(
            "`source.{}.path` failed to resolve: {:?}",
            source.name,
            source.path
        );
    }

    let source_path = source_path.unwrap();

    if !source_path.exists() {
        bail!(
            "Artifact `source.{}.path` not found: {:?}",
            source.name,
            source.path
        );
    }

    let mut source_hash_log = "untracked".to_string();

    if let Some(hash) = source.hash.clone() {
        source_hash_log = hash;
    }

    info!("[{}] checking source... ({})", source.name, source_hash_log);

    let source_files = get_file_paths(
        &source_path,
        source.excludes.clone(),
        source.includes.clone(),
    )?;

    if source_files.is_empty() {
        bail!(
            "Artifact `source.{}.path` no files found: {:?}",
            source.name,
            source.path
        );
    }

    let source_hash = match context.get_source_hash(
        source_files.clone(),
        source.name.clone(),
        source_path.clone(),
    ) {
        Some(hash) => hash.clone(),
        None => {
            context
                .add_source_hash(
                    source_files.clone(),
                    source.name.clone(),
                    source_path.clone(),
                )
                .await?
        }
    };

    if let Some(hash) = source.hash.clone() {
        if hash != source_hash {
            bail!(
                "`source.{}.hash` mismatch: {} != {}",
                source.name,
                source_hash,
                hash,
            );
        }
    }

    Ok(ArtifactSource {
        excludes: source.excludes,
        hash: Some(source_hash),
        includes: source.includes,
        name: source.name,
        path: source_path.to_string_lossy().to_string(),
    })
}

pub fn add_artifact_systems(systems: Vec<&str>) -> Result<Vec<ArtifactSystem>> {
    let mut build_systems = vec![];

    for system in systems {
        match system {
            "aarch64-linux" => build_systems.push(Aarch64Linux),
            "aarch64-macos" => build_systems.push(Aarch64Macos),
            "x86_64-linux" => build_systems.push(X8664Linux),
            "x86_64-macos" => build_systems.push(X8664Macos),
            _ => bail!("Unsupported system: {}", system),
        }
    }

    Ok(build_systems)
}

// cross-platform sandboxed artifact

pub async fn add_artifact(
    context: &mut ConfigContext,
    artifacts: Vec<ArtifactId>,
    environments: Vec<ArtifactEnvironment>,
    name: &str,
    script: String,
    sources: Vec<ArtifactSource>,
    systems: Vec<&str>,
) -> Result<ArtifactId> {
    // Setup artifacts

    let mut build_artifacts = vec![];

    for artifact in artifacts {
        build_artifacts.push(artifact);
    }

    // Setup environments

    let build_target = context.get_target();

    let mut build_environments = vec![];

    if build_target == Aarch64Linux || build_target == X8664Linux {
        let path = ArtifactEnvironment {
            key: "PATH".to_string(),
            value: "/usr/bin:/usr/sbin".to_string(),
        };

        let ssl_cert_file = ArtifactEnvironment {
            key: "SSL_CERT_FILE".to_string(),
            value: "/etc/ssl/certs/ca-certificates.crt".to_string(),
        };

        let path_prev = environments
            .clone()
            .into_iter()
            .find(|env| env.key == "PATH");

        if let Some(prev) = path_prev {
            build_environments.push(ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!("{}:{}", prev.value, path.value),
            });
        } else {
            build_environments.push(path.clone());
        }

        build_environments.push(ssl_cert_file.clone());
    }

    if build_target == Aarch64Macos || build_target == X8664Macos {
        let path = ArtifactEnvironment {
            key: "PATH".to_string(),
            value: "/usr/local/bin:/usr/bin:/usr/sbin:/bin".to_string(),
        };

        let path_prev = environments
            .clone()
            .into_iter()
            .find(|env| env.key == "PATH");

        if let Some(prev) = path_prev {
            build_environments.push(ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!("{}:{}", prev.value, path.value),
            });
        } else {
            build_environments.push(path.clone());
        }
    }

    for env in environments.clone().into_iter() {
        if env.key == "PATH" {
            continue;
        }

        build_environments.push(env);
    }

    // Setup sources

    let mut build_sources = vec![];

    for source in sources.clone().into_iter() {
        let source = add_artifact_source(context, source).await?;

        build_sources.push(source);
    }

    // Setup steps

    let mut build_steps = vec![];

    if build_target == Aarch64Linux || build_target == X8664Linux {
        let linux_debian = debian::artifact(context).await?;
        let linux_vorpal = vorpal::artifact(context, &linux_debian)?;

        build_artifacts.push(linux_vorpal.clone());

        build_steps.push(bwrap(
            vec![],
            build_artifacts.clone(),
            build_environments.clone(),
            Some(get_artifact_envkey(&linux_vorpal)),
            script.clone(),
        ));
    }

    if build_target == Aarch64Macos || build_target == X8664Macos {
        build_steps.push(bash(build_environments.clone(), script));
    }

    // Setup systems

    let systems = add_artifact_systems(systems)?;
    let systems = systems.iter().map(|s| (*s).into()).collect::<Vec<i32>>();

    // Add artifact to context

    context.add_artifact(Artifact {
        artifacts: build_artifacts.clone(),
        name: name.to_string(),
        sources: build_sources,
        steps: build_steps,
        systems,
    })
}
