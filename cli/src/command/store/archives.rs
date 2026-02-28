#[cfg(panic = "abort")]
compile_error!("compress_zstd relies on catch_unwind -- panic=abort is not supported");

use crate::command::store::temps::create_sandbox_file;
use anyhow::{anyhow, Error, Result};
use async_zip::tokio::read::seek::ZipFileReader;
use std::path::{Path, PathBuf};
use tokio::{
    fs::{copy, create_dir_all, remove_file, File, OpenOptions},
    io::{AsyncWriteExt, BufReader},
};
use tokio_stream::StreamExt;
use tokio_tar::{Archive, Builder};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tokio_util::io::SyncIoBridge;

use super::DUPLEX_BUF_SIZE;

/// Compresses files into a tar.zst archive.
///
/// Uses `ruzstd` (pure-Rust zstd) via `spawn_blocking` + `SyncIoBridge` to bridge
/// the synchronous compression API with the async tar builder. The `ruzstd 0.8`
/// `FrameCompressor::compress()` panics on errors instead of returning a `Result`,
/// so `catch_unwind` is used as a workaround. The `#[cfg(panic = "abort")]` guard
/// at the top of this file ensures this remains sound.
pub async fn compress_zstd(
    source_path: &PathBuf,
    source_files: &[PathBuf],
    output_path: &PathBuf,
) -> Result<(), Error> {
    let temp_file = create_sandbox_file(Some("tar.zst"))
        .await
        .map_err(|e| anyhow!("failed to create temp file: {}", e))?;

    let (async_reader, async_writer) = tokio::io::duplex(DUPLEX_BUF_SIZE);

    let compress_temp = temp_file.clone();
    let compress_fut = tokio::task::spawn_blocking(move || {
        let sync_reader = SyncIoBridge::new(async_reader);
        let output_file =
            std::fs::File::create(&compress_temp).map_err(|e| anyhow!("create file: {}", e))?;
        let mut buffered = std::io::BufWriter::new(output_file);
        // Fastest is the only implemented compression level in ruzstd 0.8;
        // Default/Better/Best are not yet available. Fastest ≈ zstd level 1.
        // TODO: Adopt higher compression levels when ruzstd supports them,
        // or evaluate statically-linked zstd-sys for better ratios.
        let mut compressor =
            ruzstd::encoding::FrameCompressor::new(ruzstd::encoding::CompressionLevel::Fastest);
        compressor.set_source(sync_reader);
        compressor.set_drain(&mut buffered);
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compressor.compress();
        })) {
            Ok(()) => {
                std::io::Write::flush(&mut buffered).map_err(|e| anyhow!("flush failed: {}", e))?;
            }
            Err(panic_info) => {
                // Discard the internal buffer so BufWriter::drop does not
                // attempt to flush potentially corrupt data to disk.
                let _ = buffered.into_parts();
                let msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");
                return Err(anyhow!("zstd compression panicked: {}", msg));
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    let tar_fut = async {
        let mut builder = Builder::new(async_writer);

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

        let mut output = builder
            .into_inner()
            .await
            .map_err(|e| anyhow!("failed to get tar inner writer: {}", e))?;
        output
            .shutdown()
            .await
            .map_err(|e| anyhow!("failed to shutdown tar writer: {}", e))?;

        Ok::<(), anyhow::Error>(())
    };

    // Use join! (not try_join!) to always await the blocking task, preventing
    // a resource leak if the tar side fails before compression finishes.
    let (compress_result, tar_result) = tokio::join!(
        async {
            compress_fut
                .await
                .map_err(|e| anyhow!("zstd task join error: {}", e))?
        },
        tar_fut,
    );

    if compress_result.is_err() || tar_result.is_err() {
        let _ = remove_file(&temp_file).await;
        // Check tar first — when both fail, tar is typically the root cause
        // (compression fails secondarily from broken pipe / EOF).
        tar_result?;
        compress_result?;
    }

    copy(&temp_file, output_path)
        .await
        .map_err(|e| anyhow!("failed to copy archive: {}", e))?;
    remove_file(&temp_file)
        .await
        .map_err(|e| anyhow!("failed to remove temp file: {}", e))?;

    Ok(())
}

pub async fn unpack_zstd(target_dir: &Path, source_zstd: &Path) -> Result<(), Error> {
    let (async_reader, async_writer) = tokio::io::duplex(DUPLEX_BUF_SIZE);

    let source_path = source_zstd.to_path_buf();
    let decompress_fut = tokio::task::spawn_blocking(move || {
        let file =
            std::fs::File::open(&source_path).map_err(|e| anyhow!("Failed to open file: {}", e))?;
        let buf_reader = std::io::BufReader::new(file);
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(buf_reader)
            .map_err(|e| anyhow!("zstd decoder init failed: {}", e))?;
        let mut sync_writer = SyncIoBridge::new(async_writer);
        std::io::copy(&mut decoder, &mut sync_writer)
            .map_err(|e| anyhow!("zstd decompression failed: {}", e))?;
        Ok::<(), anyhow::Error>(())
    });

    let target = target_dir.to_path_buf();
    let unpack_fut = async move {
        let mut archive = Archive::new(async_reader);

        // Iterate entries manually instead of using archive.unpack() because
        // tokio-tar does not handle overwriting existing symlinks — it fails
        // with "File exists" (os error 17). For symlink/hardlink entries, we
        // remove the destination path before unpacking.
        let mut entries = archive
            .entries()
            .map_err(|e| anyhow!("failed to read archive entries: {}", e))?;

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

            entry
                .unpack_in(&target)
                .await
                .map_err(|e| anyhow!("failed to unpack entry: {}", e))?;
        }

        Ok::<(), anyhow::Error>(())
    };

    // Use join! (not try_join!) to always await the blocking task, preventing
    // a resource leak if the unpack side fails before decompression finishes.
    let (decompress_result, unpack_result) = tokio::join!(
        async {
            decompress_fut
                .await
                .map_err(|e| anyhow!("zstd task join error: {}", e))?
        },
        unpack_fut,
    );

    // Check unpack first — when both fail, unpack is typically the root cause.
    unpack_result?;
    decompress_result?;

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
