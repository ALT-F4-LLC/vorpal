use anyhow::Result;
use async_compression::tokio::{bufread::GzipDecoder, write::GzipEncoder};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio_tar::ArchiveBuilder;
use tokio_tar::Builder;

pub async fn compress_tar_gz(
    source: &PathBuf,
    source_tar_path: &PathBuf,
    source_files: &[PathBuf],
) -> Result<File, anyhow::Error> {
    let tar = File::create(source_tar_path).await?;
    let tar_encoder = GzipEncoder::new(tar);
    let mut tar_builder = Builder::new(tar_encoder);

    for path in source_files {
        if path == source {
            continue;
        }

        let relative_path = path.strip_prefix(source)?;

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

pub async fn unpack_tar_gz(target_dir: &PathBuf, source_tar: &Path) -> Result<(), anyhow::Error> {
    let tar_gz = File::open(source_tar).await?;
    let buf_reader = BufReader::new(tar_gz);
    let gz_decoder = GzipDecoder::new(buf_reader);
    let archive_builder = ArchiveBuilder::new(gz_decoder)
        .set_preserve_permissions(true)
        .set_ignore_zeros(true);
    let mut archive = archive_builder.build();

    Ok(archive.unpack(target_dir).await?)
}
