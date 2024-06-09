use async_compression::tokio::bufread::BzDecoder;
use git2::{build::RepoBuilder, Cred, RemoteCallbacks};
use tokio_tar::Archive;
use tonic::async_trait;
use tracing::{error, info};
use url::Url;

use crate::{
    api::{PackageSource, PackageSourceKind},
    store,
};
use std::{
    any::Any,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct SourceContext {
    pub workdir_path: PathBuf,
    pub source: PackageSource,
}

/// A trait that must be implemented by all supported sources.
#[async_trait]
pub trait Source: Send {
    /// Returns the [`PackageSourceKind`] of the source.
    fn kind(&self) -> PackageSourceKind;

    async fn fetch(self: Box<Self>, ctx: SourceContext) -> Result<(), anyhow::Error>;
}

#[async_trait]
impl Source for Box<dyn Source> {
    fn kind(&self) -> PackageSourceKind {
        self.as_ref().kind()
    }

    async fn fetch(self: Box<Self>, ctx: SourceContext) -> Result<(), anyhow::Error> {
        self.fetch(ctx).await
    }
}

/// A trait object that can be downcast to a concrete type.
///
/// WARNING: this trait should never be implemented manually, because it is already implemented for
/// all types that satisfies the trait bound `Source + 'static`.
pub trait AnySource {
    /// Returns a reference to the boxed [`Source`] trait object.
    fn as_any(&self) -> &dyn Any;
    /// Returns a mutable reference to the boxed [`Source`] trait object.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Returns the [`PackageSourceKind`] of the source.
    fn any_kind(&self) -> PackageSourceKind;
}

impl std::fmt::Debug for dyn AnySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "kind: {:?}", self.any_kind())
    }
}

impl<T: Source + 'static> AnySource for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn any_kind(&self) -> PackageSourceKind {
        self.kind()
    }
}

/// Resolves a [`PackageSourceKind`] to a concrete source implementation.
///
/// # Arguments
/// - `source` - The [`PackageSourceKind`] to resolve.
pub fn resolve_source(
    source: &PackageSourceKind,
) -> Result<Box<dyn Source + 'static>, anyhow::Error> {
    let resolved: Box<dyn Source> = match source {
        PackageSourceKind::Local => Box::new(LocalPackageSource {}),
        PackageSourceKind::Http => Box::new(HttpPackageSource {}),
        PackageSourceKind::Git => Box::new(GitPackageSource {}),
        PackageSourceKind::Unknown => unreachable!(),
    };

    Ok(resolved)
}

pub struct GitPackageSource {}

#[async_trait]
impl Source for GitPackageSource {
    fn kind(&self) -> PackageSourceKind {
        PackageSourceKind::Git
    }

    async fn fetch(self: Box<Self>, ctx: SourceContext) -> Result<(), anyhow::Error> {
        let mut builder = RepoBuilder::new();

        if ctx.source.uri.starts_with("git://") {
            let mut callbacks = RemoteCallbacks::new();

            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::ssh_key(
                    username_from_url.unwrap(),
                    None,
                    Path::new(&format!(
                        "{}/.ssh/id_rsa",
                        dirs::home_dir().unwrap().display()
                    )),
                    None,
                )
            });

            let mut fetch_options = git2::FetchOptions::new();

            fetch_options.remote_callbacks(callbacks);

            builder.fetch_options(fetch_options);
        }

        let _ = builder.clone(&ctx.source.uri, &ctx.workdir_path)?;

        Ok(())
    }
}

pub struct HttpPackageSource {}

#[async_trait]
impl Source for HttpPackageSource {
    fn kind(&self) -> PackageSourceKind {
        PackageSourceKind::Http
    }

    async fn fetch(self: Box<Self>, ctx: SourceContext) -> Result<(), anyhow::Error> {
        info!("Downloading source: {:?}", &ctx.source.uri);

        let url = Url::parse(&ctx.source.uri)?;

        if url.scheme() != "http" && url.scheme() != "https" {
            error!("Invalid HTTP source URL");
            return Err(anyhow::anyhow!("Invalid HTTP source URL"));
        }

        let response = reqwest::get(url.as_str()).await?.bytes().await?;
        let response_bytes = response.as_ref();

        if let Some(source_kind) = infer::get(response_bytes) {
            info!("Preparing source kind: {:?}", source_kind);

            if let "application/gzip" = source_kind.mime_type() {
                let temp_file = store::create_temp_file("tar.gz").await?;
                tokio::fs::write(&temp_file, response_bytes).await?;
                store::unpack_tar_gz(&ctx.workdir_path, &temp_file).await?;
                tokio::fs::remove_file(&temp_file).await?;
                info!("Prepared gzip source: {:?}", ctx.workdir_path);
            } else if let "application/x-bzip2" = source_kind.mime_type() {
                let bz_decoder = BzDecoder::new(response_bytes);
                let mut archive = Archive::new(bz_decoder);
                archive.unpack(&ctx.workdir_path).await?;
                info!("Prepared bzip2 source: {:?}", ctx.workdir_path);
            } else {
                let source_file_name = url.path_segments().unwrap().last();
                let source_file = source_file_name.unwrap();
                tokio::fs::write(&source_file, response_bytes).await?;
                info!("Prepared source file: {:?}", source_file);
            }
        }

        Ok(())
    }
}

pub struct LocalPackageSource {}

#[async_trait]
impl Source for LocalPackageSource {
    fn kind(&self) -> PackageSourceKind {
        PackageSourceKind::Local
    }

    async fn fetch(self: Box<Self>, ctx: SourceContext) -> Result<(), anyhow::Error> {
        let source_path = Path::new(&ctx.source.uri).canonicalize()?;

        info!("Preparing source path: {:?}", source_path);

        if let Ok(Some(source_kind)) = infer::get_from_path(&source_path) {
            info!("Preparing source kind: {:?}", source_kind);

            if source_kind.mime_type() == "application/gzip" {
                info!("Preparing packed source: {:?}", ctx.workdir_path);
                store::unpack_tar_gz(&ctx.workdir_path, &source_path).await?;
            }
        }

        if source_path.is_file() {
            let dest = ctx.workdir_path.join(source_path.file_name().unwrap());
            tokio::fs::copy(&source_path, &dest).await?;
            info!(
                "Preparing source file: {:?} -> {:?}",
                source_path.display(),
                dest.display()
            );
        }

        if source_path.is_dir() {
            let files = store::get_file_paths(&source_path, &ctx.source.ignore_paths)?;

            if files.is_empty() {
                return Err(anyhow::anyhow!("No source files found"));
            }

            for src in &files {
                if src.is_dir() {
                    let dest = ctx.workdir_path.join(src.strip_prefix(&source_path)?);
                    tokio::fs::create_dir_all(dest).await?;
                    continue;
                }

                let dest = ctx.workdir_path.join(src.file_name().unwrap());
                tokio::fs::copy(src, &dest).await?;
                info!(
                    "Preparing source file: {:?} -> {:?}",
                    src.display(),
                    dest.display()
                );
            }
        }
        Ok(())
    }
}
