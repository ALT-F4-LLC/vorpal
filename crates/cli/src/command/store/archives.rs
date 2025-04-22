use crate::command::store::temps::create_sandbox_file;
use anyhow::{Error, Result};
use async_compression::tokio::{bufread::ZstdDecoder, write::ZstdEncoder};
use async_zip::tokio::read::seek::ZipFileReader;
use std::path::{Path, PathBuf};
use tokio::{
    fs::{copy, create_dir_all, remove_file, File, OpenOptions},
    io::{AsyncWriteExt, BufReader},
};
use tokio_tar::{Archive, Builder};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

pub async fn compress_zstd(
    source_path: &PathBuf,
    source_files: &[PathBuf],
    output_path: &PathBuf,
) -> Result<File, Error> {
    let temp_file = create_sandbox_file(Some("tar.zst"))
        .await
        .expect("Failed to create temp file");

    let file = File::create(temp_file.clone())
        .await
        .expect("Failed to create file");

    let encoder = ZstdEncoder::new(file);

    let mut builder = Builder::new(encoder);

    builder.follow_symlinks(false);

    for path in source_files {
        let relative_path = path
            .strip_prefix(source_path)
            .expect("Failed to strip prefix");

        if relative_path.display().to_string() == "" {
            continue;
        }

        builder
            .append_path_with_name(path, relative_path)
            .await
            .expect("Failed to append path");
    }

    builder.finish().await.expect("Failed to finish builder");

    let mut output = builder.into_inner().await.expect("Failed to get inner");

    output.shutdown().await.expect("Failed to shutdown");

    let mut file = output.into_inner();

    file.flush().await.expect("Failed to flush");

    copy(&temp_file, output_path).await.expect("Failed to copy");

    remove_file(temp_file).await.expect("Failed to remove file");

    Ok(file)
}

pub async fn unpack_zstd(target_dir: &PathBuf, source_zstd: &Path) -> Result<(), Error> {
    let zstd = File::open(source_zstd).await.expect("Failed to open file");

    let buf_reader = BufReader::new(zstd);

    let zstd_decoder = ZstdDecoder::new(buf_reader);

    let mut archive = Archive::new(zstd_decoder);

    archive.unpack(target_dir).await.expect("Failed to unpack");

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

            // Closes the file and manipulates its metadata here if you wish to preserve its metadata from the archive.
        }
    }

    Ok(())
}
