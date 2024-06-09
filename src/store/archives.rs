use anyhow::Result;
use async_compression::tokio::{bufread::GzipDecoder, write::GzipEncoder};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio_tar::Archive;
use tokio_tar::Builder;
use tracing::info;

pub async fn compress_tar_gz<'a, P1, P2, P3, I>(
    source: P1,
    source_output: P2,
    source_files: I,
) -> Result<File, anyhow::Error>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    P3: AsRef<Path> + 'a,
    I: IntoIterator<Item = &'a P3>,
{
    let tar = File::create(source_output).await?;
    let tar_encoder = GzipEncoder::new(tar);
    let mut tar_builder = Builder::new(tar_encoder);

    let source = source.as_ref();
    info!("Compressing: {}", source.display());

    for path in source_files {
        let path = path.as_ref();
        if path == source {
            continue;
        }

        let relative_path = path.strip_prefix(source)?;
        info!("Adding: {}", relative_path.display());

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

pub async fn unpack_tar_gz<P1, P2>(target_dir: P1, source_tar: P2) -> Result<(), anyhow::Error>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    let tar_gz = File::open(source_tar).await?;
    let buf_reader = BufReader::new(tar_gz);
    let gz_decoder = GzipDecoder::new(buf_reader);
    let mut archive = Archive::new(gz_decoder);

    Ok(archive.unpack(target_dir).await?)
}
