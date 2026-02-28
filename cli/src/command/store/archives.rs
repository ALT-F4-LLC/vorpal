use crate::command::store::temps::create_sandbox_file;
use anyhow::{anyhow, Error, Result};
use async_compression::tokio::{bufread::ZstdDecoder, write::ZstdEncoder};
use async_zip::tokio::read::seek::ZipFileReader;
use std::path::{Path, PathBuf};
use tokio::{
    fs::{copy, create_dir_all, remove_file, File, OpenOptions},
    io::{AsyncWriteExt, BufReader},
};
use tokio_stream::StreamExt;
use tokio_tar::{Archive, Builder};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

pub async fn compress_zstd(
    source_path: &PathBuf,
    source_files: &[PathBuf],
    output_path: &PathBuf,
) -> Result<(), Error> {
    let temp_file = create_sandbox_file(Some("tar.zst"))
        .await
        .map_err(|e| anyhow!("failed to create temp file: {}", e))?;

    let file = File::create(&temp_file)
        .await
        .map_err(|e| anyhow!("failed to create temp file: {}", e))?;

    let encoder = ZstdEncoder::new(file);
    let mut builder = Builder::new(encoder);

    builder.follow_symlinks(false);

    for path in source_files {
        let relative_path = path
            .strip_prefix(source_path)
            .map_err(|e| anyhow!("failed to strip prefix: {}", e))?;

        if relative_path.display().to_string() == "" {
            continue;
        }

        builder
            .append_path_with_name(path, relative_path)
            .await
            .map_err(|e| anyhow!("failed to append path: {}", e))?;
    }

    builder
        .finish()
        .await
        .map_err(|e| anyhow!("failed to finish tar builder: {}", e))?;

    let mut encoder = builder
        .into_inner()
        .await
        .map_err(|e| anyhow!("failed to get tar inner writer: {}", e))?;
    encoder
        .shutdown()
        .await
        .map_err(|e| anyhow!("failed to shutdown zstd encoder: {}", e))?;

    copy(&temp_file, output_path)
        .await
        .map_err(|e| anyhow!("failed to copy archive: {}", e))?;
    remove_file(&temp_file)
        .await
        .map_err(|e| anyhow!("failed to remove temp file: {}", e))?;

    Ok(())
}

pub async fn unpack_zstd(target_dir: &Path, source_zstd: &Path) -> Result<(), Error> {
    let file = File::open(source_zstd)
        .await
        .map_err(|e| anyhow!("failed to open file: {}", e))?;
    let buf_reader = BufReader::new(file);
    let zstd_decoder = ZstdDecoder::new(buf_reader);
    let mut archive = Archive::new(zstd_decoder);

    // Iterate entries manually instead of using archive.unpack() because
    // tokio-tar does not handle overwriting existing symlinks — it fails
    // with "File exists" (os error 17). For symlink/hardlink entries, we
    // remove the destination path before unpacking.
    let mut entries = archive
        .entries()
        .map_err(|e| anyhow!("failed to read archive entries: {}", e))?;

    let target = target_dir.to_path_buf();

    while let Some(entry) = entries.next().await {
        let mut entry = entry.map_err(|e| anyhow!("failed to read archive entry: {}", e))?;
        let entry_type = entry.header().entry_type();

        if entry_type.is_symlink() || entry_type.is_hard_link() {
            if let Ok(path) = entry.path() {
                let dest = target.join(path);
                if dest.symlink_metadata().is_ok() {
                    tokio::fs::remove_file(&dest)
                        .await
                        .map_err(|e| anyhow!("failed to remove existing symlink: {}", e))?;
                }
            }
        }

        // Ensure all existing ancestor directories are writable.
        // Tar archives (e.g. Rust toolchain) may contain directory entries
        // with read-only permissions (0555) that appear before their child
        // entries, preventing extraction into them. Walk up from the entry
        // path to the target root, making each read-only directory writable.
        #[cfg(unix)]
        {
            if let Ok(path) = entry.path() {
                use std::os::unix::fs::PermissionsExt;
                let full_path = target.join(&*path);
                let mut ancestor = full_path.parent();
                while let Some(dir) = ancestor {
                    if dir == target {
                        break;
                    }
                    if let Ok(meta) = tokio::fs::metadata(dir).await {
                        let mode = meta.permissions().mode();
                        if mode & 0o200 == 0 {
                            let mut perms = meta.permissions();
                            perms.set_mode(mode | 0o200);
                            let _ = tokio::fs::set_permissions(dir, perms).await;
                        }
                    }
                    ancestor = dir.parent();
                }
            }
        }

        // Skip non-directory entries whose destination already exists as a
        // directory. Some tar archives contain entries typed as regular
        // files for paths that were already implicitly created as
        // directories by earlier child entries. tokio-tar tries to
        // remove_file() the existing path, which returns EPERM on macOS
        // when the path is a directory.
        if !entry_type.is_dir() {
            if let Ok(path) = entry.path() {
                let dest = target.join(&*path);
                if dest.is_dir() {
                    continue;
                }
            }
        }

        entry.unpack_in(&target).await.map_err(|e| {
            // Walk the error source chain because TarError::Display
            // only shows the description and drops the underlying IO
            // error (e.g. "Permission denied", "File exists").
            let mut msg = format!("failed to unpack entry: {}", e);
            let mut source: Option<&dyn std::error::Error> = std::error::Error::source(&e);
            while let Some(s) = source {
                msg.push_str(&format!(": {}", s));
                source = s.source();
            }
            anyhow!(msg)
        })?;
    }

    Ok(())
}

/// Returns a relative path without reserved names, redundant separators, ".", or "..".
fn sanitize_file_path(path: &str) -> PathBuf {
    // Replaces backwards slashes
    path.replace('\\', "/")
        // Sanitizes each component
        .split('/')
        .map(sanitize_filename::sanitize)
        .collect()
}

pub async fn unpack_zip(source_path: &PathBuf, target_dir: &Path) -> Result<(), Error> {
    let archive_file = File::open(source_path).await.expect("Failed to open file");

    let archive = BufReader::new(archive_file).compat();

    let mut reader = ZipFileReader::new(archive)
        .await
        .expect("Failed to read zip file");

    for index in 0..reader.file().entries().len() {
        let entry = reader.file().entries().get(index).unwrap();

        let path = target_dir.join(sanitize_file_path(entry.filename().as_str().unwrap()));

        // If the filename of the entry ends with '/', it is treated as a directory.
        // This is implemented by previous versions of this crate and the Python Standard Library.
        // https://docs.rs/async_zip/0.0.8/src/async_zip/read/mod.rs.html#63-65
        // https://github.com/python/cpython/blob/820ef62833bd2d84a141adedd9a05998595d6b6d/Lib/zipfile.py#L528
        let entry_is_dir = entry.dir().unwrap();

        let mut entry_reader = reader
            .reader_without_entry(index)
            .await
            .expect("Failed to read ZipEntry");

        if entry_is_dir {
            // The directory may have been created if iteration is out of order.
            if !path.exists() {
                create_dir_all(&path)
                    .await
                    .expect("Failed to create extracted directory");
            }
        } else {
            // Creates parent directories. They may not exist if iteration is out of order
            // or the archive does not contain directory entries.
            let parent = path
                .parent()
                .expect("A file entry should have parent directories");

            if !parent.is_dir() {
                create_dir_all(parent)
                    .await
                    .expect("Failed to create parent directories");
            }

            let writer = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
                .expect("Failed to create extracted file");

            futures_lite::io::copy(&mut entry_reader, &mut writer.compat_write())
                .await
                .expect("Failed to copy to extracted file");
        }
    }

    Ok(())
}
