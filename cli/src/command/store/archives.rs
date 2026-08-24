use crate::command::store::temps::create_sandbox_file;
use anyhow::{anyhow, Error, Result};
use async_compression::tokio::{bufread::ZstdDecoder, write::ZstdEncoder};
use async_zip::tokio::read::seek::ZipFileReader;
use std::path::{Component, Path, PathBuf};
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
        .map_err(|e| anyhow!("failed to create temp file: {e}"))?;

    let file = File::create(&temp_file)
        .await
        .map_err(|e| anyhow!("failed to create temp file: {e}"))?;

    let encoder = ZstdEncoder::new(file);
    let mut builder = Builder::new(encoder);

    builder.follow_symlinks(false);

    for path in source_files {
        let relative_path = path
            .strip_prefix(source_path)
            .map_err(|e| anyhow!("failed to strip prefix: {e}"))?;

        if relative_path.display().to_string() == "" {
            continue;
        }

        builder
            .append_path_with_name(path, relative_path)
            .await
            .map_err(|e| anyhow!("failed to append path: {e}"))?;
    }

    builder
        .finish()
        .await
        .map_err(|e| anyhow!("failed to finish tar builder: {e}"))?;

    let mut encoder = builder
        .into_inner()
        .await
        .map_err(|e| anyhow!("failed to get tar inner writer: {e}"))?;
    encoder
        .shutdown()
        .await
        .map_err(|e| anyhow!("failed to shutdown zstd encoder: {e}"))?;

    copy(&temp_file, output_path)
        .await
        .map_err(|e| anyhow!("failed to copy archive: {e}"))?;
    remove_file(&temp_file)
        .await
        .map_err(|e| anyhow!("failed to remove temp file: {e}"))?;

    Ok(())
}

pub async fn unpack_zstd(target_dir: &Path, source_zstd: &Path) -> Result<(), Error> {
    let file = File::open(source_zstd)
        .await
        .map_err(|e| anyhow!("failed to open file: {e}"))?;
    let buf_reader = BufReader::new(file);
    let zstd_decoder = ZstdDecoder::new(buf_reader);
    let mut archive = Archive::new(zstd_decoder);

    // Iterate entries manually instead of using archive.unpack() because
    // tokio-tar does not handle overwriting existing symlinks — it fails
    // with "File exists" (os error 17). For symlink/hardlink entries, we
    // remove the destination path before unpacking.
    let mut entries = archive
        .entries()
        .map_err(|e| anyhow!("failed to read archive entries: {e}"))?;

    let target = target_dir.to_path_buf();

    while let Some(entry) = entries.next().await {
        let mut entry = entry.map_err(|e| anyhow!("failed to read archive entry: {e}"))?;
        let entry_type = entry.header().entry_type();

        let raw_path = entry
            .path()
            .map_err(|e| anyhow!("failed to read archive entry path: {}", e))?
            .into_owned();

        // Reject any entry whose path would escape the target directory
        // before touching the filesystem. tokio-tar's own traversal guard
        // (Entry::unpack_in) runs after the removal and chmod steps below,
        // so an unsanitized path here is a delete/chmod primitive outside
        // the target for a hostile archive.
        let path = sanitize_entry_path(&raw_path)?;

        if entry_type.is_symlink() || entry_type.is_hard_link() {
            let dest = target.join(&path);
            if dest.symlink_metadata().is_ok() {
                tokio::fs::remove_file(&dest)
                    .await
                    .map_err(|e| anyhow!("failed to remove existing symlink: {}", e))?;
            }
        }

        // Ensure all existing ancestor directories are writable.
        // Tar archives (e.g. Rust toolchain) may contain directory entries
        // with read-only permissions (0555) that appear before their child
        // entries, preventing extraction into them. Walk up from the entry
        // path to the target root, making each read-only directory writable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let full_path = target.join(&path);
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

        // Skip non-directory entries whose destination already exists as a
        // directory. Some tar archives contain entries typed as regular
        // files for paths that were already implicitly created as
        // directories by earlier child entries. tokio-tar tries to
        // remove_file() the existing path, which returns EPERM on macOS
        // when the path is a directory.
        if !entry_type.is_dir() {
            let dest = target.join(&path);
            if dest.is_dir() {
                continue;
            }
        }

        let unpacked = entry.unpack_in(&target).await.map_err(|e| {
            // Walk the error source chain because TarError::Display
            // only shows the description and drops the underlying IO
            // error (e.g. "Permission denied", "File exists").
            use std::fmt::Write;

            let mut msg = format!("failed to unpack entry: {e}");
            let mut source: Option<&dyn std::error::Error> = std::error::Error::source(&e);
            while let Some(s) = source {
                let _ = write!(msg, ": {s}");
                source = s.source();
            }
            anyhow!(msg)
        })?;

        if !unpacked {
            return Err(anyhow!(
                "archive entry escaped target directory: {}",
                raw_path.display()
            ));
        }
    }

    Ok(())
}

/// Sanitizes a raw archive entry path against traversal outside the target
/// directory. `Prefix`, `RootDir`, and `CurDir` components are dropped (so the
/// computed destination never disagrees with the relative path tokio-tar
/// itself would re-root an absolute name under); a `ParentDir` (`..`)
/// component is rejected outright, and an entry whose raw path was absolute
/// is rejected too rather than silently re-rooted, since an absolute name in
/// an archive vorpal itself produced is an attack signature, not a
/// compatibility case. An entry that reduces to the target itself (`.`,
/// `./`) is rejected too: left unrejected, it would make the ancestor-chmod
/// walk below start one level *above* the target instead of inside it,
/// reopening the same walk-to-root exposure this function exists to close.
fn sanitize_entry_path(path: &Path) -> Result<PathBuf, Error> {
    if path.is_absolute() {
        return Err(anyhow!(
            "archive entry path is absolute, rejecting: {}",
            path.display()
        ));
    }

    let mut sanitized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                return Err(anyhow!(
                    "archive entry path traverses outside target: {}",
                    path.display()
                ));
            }
        }
    }

    if sanitized.as_os_str().is_empty() {
        return Err(anyhow!(
            "archive entry path resolves to the target root, rejecting: {}",
            path.display()
        ));
    }

    Ok(sanitized)
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
    let archive_file = File::open(source_path)
        .await
        .map_err(|e| anyhow!("failed to open file: {e}"))?;

    let archive = BufReader::new(archive_file).compat();

    let mut reader = ZipFileReader::new(archive)
        .await
        .map_err(|e| anyhow!("failed to read zip file: {e}"))?;

    for index in 0..reader.file().entries().len() {
        let entry = reader
            .file()
            .entries()
            .get(index)
            .ok_or_else(|| anyhow!("zip entry {index} disappeared during iteration"))?;

        let entry_filename = entry
            .filename()
            .as_str()
            .map_err(|e| anyhow!("failed to read zip entry filename: {e}"))?;

        let path = target_dir.join(sanitize_file_path(entry_filename));

        // If the filename of the entry ends with '/', it is treated as a directory.
        // This is implemented by previous versions of this crate and the Python Standard Library.
        // https://docs.rs/async_zip/0.0.8/src/async_zip/read/mod.rs.html#63-65
        // https://github.com/python/cpython/blob/820ef62833bd2d84a141adedd9a05998595d6b6d/Lib/zipfile.py#L528
        let entry_is_dir = entry
            .dir()
            .map_err(|e| anyhow!("failed to determine zip entry type: {e}"))?;

        let mut entry_reader = reader
            .reader_without_entry(index)
            .await
            .map_err(|e| anyhow!("failed to read ZipEntry: {e}"))?;

        if entry_is_dir {
            // The directory may have been created if iteration is out of order.
            if !path.exists() {
                create_dir_all(&path)
                    .await
                    .map_err(|e| anyhow!("failed to create extracted directory: {e}"))?;
            }
        } else {
            // Creates parent directories. They may not exist if iteration is out of order
            // or the archive does not contain directory entries.
            let parent = path
                .parent()
                .ok_or_else(|| anyhow!("a file entry should have parent directories"))?;

            if !parent.is_dir() {
                create_dir_all(parent)
                    .await
                    .map_err(|e| anyhow!("failed to create parent directories: {e}"))?;
            }

            let writer = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
                .map_err(|e| anyhow!("failed to create extracted file: {e}"))?;

            futures_lite::io::copy(&mut entry_reader, &mut writer.compat_write())
                .await
                .map_err(|e| anyhow!("failed to copy to extracted file: {e}"))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::unpack_zstd;
    use async_compression::tokio::write::ZstdEncoder;
    use std::path::PathBuf;
    use tokio::{
        fs::{self, File},
        io::AsyncWriteExt,
    };
    use tokio_tar::{Builder, EntryType, Header};

    /// Writes a tar archive containing `entries` (path, link name, contents)
    /// and zstd-compresses it, returning the path to the compressed archive.
    /// A symlink entry has non-empty `link_name` and empty `contents`; a
    /// regular file entry has empty `link_name` and its `contents`.
    async fn build_archive(dir: &std::path::Path, entries: &[(&str, &str, &[u8])]) -> PathBuf {
        let tar_path = dir.join("archive.tar");
        let zst_path = dir.join("archive.tar.zst");

        let tar_file = File::create(&tar_path).await.unwrap();
        let mut builder = Builder::new(tar_file);

        for (path, link_name, contents) in entries {
            let mut header = Header::new_gnu();

            if link_name.is_empty() {
                header.set_entry_type(EntryType::Regular);
                header.set_size(contents.len() as u64);
                header.set_mode(0o644);
                builder
                    .append_data(&mut header, path, *contents)
                    .await
                    .unwrap();
            } else {
                header.set_entry_type(EntryType::Symlink);
                // Header::set_path refuses `..` and absolute paths itself —
                // that is tokio-tar's own writer being a well-behaved
                // client, not a guarantee about bytes a hostile archive can
                // carry. Write the raw ustar name field directly so the
                // fixture matches what an attacker actually controls.
                let name_bytes = path.as_bytes();
                assert!(name_bytes.len() < 100, "test fixture name too long");
                header.as_mut_bytes()[0..name_bytes.len()].copy_from_slice(name_bytes);
                header.set_link_name(link_name).unwrap();
                header.set_size(0);
                header.set_mode(0o777);
                header.set_cksum();
                builder.append(&header, &b""[..]).await.unwrap();
            }
        }

        builder.finish().await.unwrap();
        let tar_file = builder.into_inner().await.unwrap();
        drop(tar_file);

        let raw = fs::read(&tar_path).await.unwrap();
        let zst_file = File::create(&zst_path).await.unwrap();
        let mut encoder = ZstdEncoder::new(zst_file);
        encoder.write_all(&raw).await.unwrap();
        encoder.shutdown().await.unwrap();

        zst_path
    }

    #[tokio::test]
    async fn rejects_relative_traversal_symlink_and_preserves_canary() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        let canary = base.path().join("victim");
        fs::write(&canary, b"untouched").await.unwrap();

        let archive = build_archive(base.path(), &[("../victim", "irrelevant", b"")]).await;

        let result = unpack_zstd(&target, &archive).await;

        assert!(result.is_err(), "traversing entry must refuse the unpack");
        assert_eq!(
            fs::read(&canary).await.unwrap(),
            b"untouched",
            "canary outside target must survive a refused unpack"
        );
    }

    #[tokio::test]
    async fn rejects_an_entry_that_resolves_to_the_target_root() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        let archive = build_archive(base.path(), &[(".", "irrelevant", b"")]).await;

        let result = unpack_zstd(&target, &archive).await;

        assert!(
            result.is_err(),
            "an entry that sanitizes to the target root itself must refuse the unpack"
        );
        assert!(
            target.is_dir(),
            "the target directory itself must survive a refused unpack"
        );
    }

    #[tokio::test]
    async fn rejects_absolute_path_entry_and_preserves_canary() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        let canary = base.path().join("victim");
        fs::write(&canary, b"untouched").await.unwrap();

        let absolute_name = canary.to_str().unwrap().to_string();
        let archive = build_archive(base.path(), &[(&absolute_name, "irrelevant", b"")]).await;

        let result = unpack_zstd(&target, &archive).await;

        assert!(
            result.is_err(),
            "absolute-path entry must refuse the unpack"
        );
        assert_eq!(
            fs::read(&canary).await.unwrap(),
            b"untouched",
            "canary must survive a refused unpack even though it is named literally"
        );
    }

    #[tokio::test]
    async fn ordinary_archive_still_overwrites_symlinks_and_unlocks_readonly_ancestors() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        // Existing symlink the archive's own entry must be able to overwrite.
        let existing_link = target.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink("old-target", &existing_link).unwrap();

        // Existing read-only ancestor directory the walk must unlock so the
        // child entry inside it can be written.
        let readonly_dir = target.join("readonly");
        fs::create_dir_all(&readonly_dir).await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o555))
                .await
                .unwrap();
        }

        let archive = build_archive(
            base.path(),
            &[("link", "new-target", b""), ("readonly/child", "", b"hi")],
        )
        .await;

        let result = unpack_zstd(&target, &archive).await;
        assert!(result.is_ok(), "ordinary archive must unpack: {result:?}");

        #[cfg(unix)]
        {
            let link_target = fs::read_link(&existing_link).await.unwrap();
            assert_eq!(link_target, std::path::PathBuf::from("new-target"));

            let child_contents = fs::read(readonly_dir.join("child")).await.unwrap();
            assert_eq!(child_contents, b"hi");

            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&readonly_dir)
                .await
                .unwrap()
                .permissions()
                .mode();
            assert_ne!(mode & 0o200, 0, "read-only ancestor must become writable");
        }
    }
}
