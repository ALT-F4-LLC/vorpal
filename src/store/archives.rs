use anyhow::Result;
use async_compression::tokio::{
    bufread::{GzipDecoder, ZstdDecoder},
    write::GzipEncoder,
    write::ZstdEncoder,
};
use async_zip::tokio::read::seek::ZipFileReader;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::{
    fs::{copy, create_dir_all, File, OpenOptions},
    io::BufReader,
};
use tokio_tar::{Archive, ArchiveBuilder, Builder};
use tokio_util::compat::TokioAsyncWriteCompatExt;

pub async fn compress_zstd(
    source_path: &PathBuf,
    source_files: &[PathBuf],
    output_path: &PathBuf,
) -> Result<File, anyhow::Error> {
    let source_archive_path = source_path.join("source").with_extension("tar.zst");

    let file = File::create(source_archive_path.clone()).await?;
    let encoder = ZstdEncoder::new(file);
    let mut builder = Builder::new(encoder);

    for path in source_files {
        let relative_path = path.strip_prefix(source_path)?;

        if relative_path.display().to_string() == "" {
            continue;
        }

        builder.append_path_with_name(path, relative_path).await?;
    }

    builder.finish().await?;

    let mut output = builder.into_inner().await?;

    output.shutdown().await?;

    let mut file = output.into_inner();

    file.flush().await?;

    copy(&source_archive_path, output_path).await?;

    Ok(file)
}

pub async fn unpack_zstd(target_dir: &PathBuf, source_zstd: &Path) -> Result<(), anyhow::Error> {
    let zstd = File::open(source_zstd).await?;
    let buf_reader = BufReader::new(zstd);
    let zstd_decoder = ZstdDecoder::new(buf_reader);
    let mut archive = Archive::new(zstd_decoder);

    Ok(archive.unpack(target_dir).await?)
}

pub async fn compress_gzip(
    source_path: &PathBuf,
    source_files: &[PathBuf],
    output_tar_path: &PathBuf,
) -> Result<File, anyhow::Error> {
    let tar = File::create(output_tar_path).await?;
    let tar_encoder = GzipEncoder::new(tar);
    let mut tar_builder = Builder::new(tar_encoder);

    for path in source_files {
        if path == source_path {
            continue;
        }

        let relative_path = path.strip_prefix(source_path)?;

        if path.is_file() {
            tar_builder
                .append_path_with_name(path, relative_path)
                .await?;
        } else if path.is_dir() {
            tar_builder.append_dir_all(relative_path, path).await?;
        }
    }

    let mut output = tar_builder.into_inner().await?;

    output.shutdown().await?;

    Ok(output.into_inner())
}

pub async fn unpack_gzip(target_dir: &PathBuf, source_tar: &Path) -> Result<(), anyhow::Error> {
    let tar_gz = File::open(source_tar).await?;
    let buf_reader = BufReader::new(tar_gz);
    let gz_decoder = GzipDecoder::new(buf_reader);
    let archive_builder = ArchiveBuilder::new(gz_decoder);
    let mut archive = archive_builder.build();

    Ok(archive.unpack(target_dir).await?)
}

pub async fn unpack_zip(source_path: &PathBuf, out_dir: &Path) -> Result<(), anyhow::Error> {
    let archive_file = File::open(source_path)
        .await
        .expect("Failed to open zip file");

    let mut archive = BufReader::new(archive_file);

    let mut reader = ZipFileReader::with_tokio(&mut archive)
        .await
        .expect("Failed to read zip file");

    for index in 0..reader.file().entries().len() {
        let entry = reader.file().entries().get(index).unwrap();
        let path = out_dir.join(entry.filename().as_str().unwrap());
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
            if !path.exists() {
                create_dir_all(&path)
                    .await
                    .expect("Failed to create extracted directory");
            }
        } else {
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
