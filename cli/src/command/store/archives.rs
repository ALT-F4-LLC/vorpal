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

/// Unpacks a zstd-compressed tar archive into `target_dir`.
///
/// Two preconditions this function relies on rather than checks itself:
/// `target_dir` is trusted — never built from unvalidated request input; see
/// `store::paths::parse_store_path_component`, applied upstream at every
/// caller — and `source_zstd` is expected to be an archive `compress_zstd`
/// produced, whose entries are always relative and never carry a leading
/// `.`/`/` (`compress_zstd` above strips the source prefix and skips the
/// empty relative path). An archive that deviates from that corpus — an
/// absolute or `..`-carrying entry — is treated as hostile and refused
/// rather than accommodated.
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

        if path.as_os_str().is_empty() {
            // A directory entry that lexically resolves to the target
            // itself (an archive's own "./" root entry) is ordinary and
            // benign: the directory already exists, so there is nothing to
            // do. Any other entry type resolving to the target itself (a
            // symlink named ".", say) has no legitimate reading and is
            // refused — left unrejected, `dest == target` and the
            // remove_file/chmod calls below would operate on the target
            // root instead of a real entry.
            if entry_type.is_dir() {
                continue;
            }
            return Err(anyhow!(
                "archive entry path resolves to the target root, rejecting: {:?}",
                raw_path
            ));
        }

        // Resolve the destination one component at a time, refusing to
        // continue through any *existing* intermediate component that is
        // itself a symlink. tokio-tar's own containment check
        // (`validate_inside_dst`) does this too, but only after the
        // remove_file/chmod calls below have already run against an
        // unresolved path — this moves that check ahead of them, closing
        // the case where an earlier archive entry planted a symlink and a
        // later, lexically-clean entry walks through it to a location
        // outside `target`.
        let dest = resolve_inside_target(&target, &path).await?;

        if (entry_type.is_symlink() || entry_type.is_hard_link())
            && tokio::fs::symlink_metadata(&dest).await.is_ok()
        {
            tokio::fs::remove_file(&dest)
                .await
                .map_err(|e| anyhow!("failed to remove existing symlink: {}", e))?;
        }

        // Ensure all existing ancestor directories are writable.
        // Tar archives (e.g. Rust toolchain) may contain directory entries
        // with read-only permissions (0555) that appear before their child
        // entries, preventing extraction into them. Walk up from the entry
        // path to the target root, making each read-only directory writable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut ancestor = dest.parent();
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
        //
        // Checked with symlink_metadata rather than a following stat:
        // Path::is_dir() follows symlinks, so a symlink an earlier entry
        // planted at `dest` that happens to point at a directory would be
        // treated as "already extracted" and the real entry silently
        // dropped, with the unpack still reporting success.
        if !entry_type.is_dir() {
            if let Ok(meta) = tokio::fs::symlink_metadata(&dest).await {
                if meta.is_dir() {
                    continue;
                }
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
                "archive entry escaped target directory: {:?}",
                raw_path
            ));
        }
    }

    Ok(())
}

/// Resolves `relative` under `target` one component at a time, refusing to
/// continue through any existing intermediate component that is a symlink.
/// A missing intermediate component is not an error: tokio-tar creates
/// parent directories itself, later, inside `entry.unpack_in`, so a
/// component that does not exist yet is nothing to check.
async fn resolve_inside_target(target: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let mut current = target.to_path_buf();
    let mut components = relative.components().peekable();

    while let Some(component) = components.next() {
        current.push(component);
        if components.peek().is_none() {
            // The final component is the entry's own destination — the
            // caller operates on it directly and it need not already exist.
            break;
        }
        if let Ok(meta) = tokio::fs::symlink_metadata(&current).await {
            if meta.file_type().is_symlink() {
                return Err(anyhow!(
                    "archive entry path traverses through a symlink at {:?}: {:?}",
                    current,
                    relative
                ));
            }
        }
    }

    Ok(current)
}

/// Sanitizes a raw archive entry path against the two purely lexical hazards
/// in a tar header path. `Prefix`, `RootDir`, and `CurDir` components are
/// dropped (so the computed destination never disagrees with the relative
/// path tokio-tar itself would re-root an absolute name under); a
/// `ParentDir` (`..`) component is rejected outright, and a raw path that
/// was absolute is rejected too rather than silently re-rooted, since an
/// absolute name in an archive vorpal itself produced is an attack
/// signature, not a compatibility case. The result may be the empty path
/// (an entry named `.`, `./`, or `/`) — the caller decides what that means,
/// since the right answer depends on the entry's type.
///
/// Ceiling: this function is lexical only. It has no way to know what an
/// *earlier* archive entry already planted on disk — a symlink an earlier
/// entry created can make a later, lexically-clean path resolve outside
/// `target` at unpack time. That is a filesystem-state hazard, not a
/// path-syntax one, and this function's `Ok` does not mean "safe to
/// operate on": it is closed separately by `resolve_inside_target`, which
/// runs against the real filesystem after this function returns.
fn sanitize_entry_path(path: &Path) -> Result<PathBuf, Error> {
    if path.is_absolute() {
        return Err(anyhow!(
            "archive entry path is absolute, rejecting: {:?}",
            path
        ));
    }

    let mut sanitized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                return Err(anyhow!(
                    "archive entry path traverses outside target: {:?}",
                    path
                ));
            }
        }
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

    /// The kind of archive entry a test fixture wants to write. Kept
    /// distinct from tokio-tar's own `EntryType` so a test reads as intent
    /// ("this is a benign directory entry") rather than as a raw tar
    /// typeflag.
    enum EntryKind<'a> {
        Regular(&'a [u8]),
        Symlink(&'a str),
        Directory,
    }

    /// Writes a tar archive containing `entries` (path, kind) and
    /// zstd-compresses it, returning the path to the compressed archive.
    async fn build_archive(dir: &std::path::Path, entries: &[(&str, EntryKind<'_>)]) -> PathBuf {
        let tar_path = dir.join("archive.tar");
        let zst_path = dir.join("archive.tar.zst");

        let tar_file = File::create(&tar_path).await.unwrap();
        let mut builder = Builder::new(tar_file);

        for (path, kind) in entries {
            let mut header = Header::new_gnu();

            match kind {
                EntryKind::Regular(contents) => {
                    header.set_entry_type(EntryType::Regular);
                    header.set_size(contents.len() as u64);
                    header.set_mode(0o644);
                    builder
                        .append_data(&mut header, path, *contents)
                        .await
                        .unwrap();
                }
                EntryKind::Directory => {
                    header.set_entry_type(EntryType::Directory);
                    header.set_size(0);
                    header.set_mode(0o755);
                    builder
                        .append_data(&mut header, path, &b""[..])
                        .await
                        .unwrap();
                }
                EntryKind::Symlink(link_name) => {
                    header.set_entry_type(EntryType::Symlink);
                    // Header::set_path refuses `..` and absolute paths
                    // itself — that is tokio-tar's own writer being a
                    // well-behaved client, not a guarantee about bytes a
                    // hostile archive can carry. Write the raw ustar name
                    // field directly so the fixture matches what an
                    // attacker actually controls.
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

        let archive = build_archive(
            base.path(),
            &[("../victim", EntryKind::Symlink("irrelevant"))],
        )
        .await;

        let result = unpack_zstd(&target, &archive).await;

        let err = result.expect_err("traversing entry must refuse the unpack");
        assert!(
            format!("{err}").contains("traverses outside target"),
            "error must name the traversal, not some incidental failure: {err}"
        );
        assert!(canary.exists(), "canary outside target must not be deleted");
        assert_eq!(
            fs::read(&canary).await.unwrap(),
            b"untouched",
            "canary outside target must survive a refused unpack"
        );
    }

    #[tokio::test]
    async fn root_directory_entry_is_skipped_not_rejected() {
        // A benign archive whose entries include the root directory itself
        // (an ordinary "./" entry many tar writers emit) must not fail the
        // whole unpack — it lexically resolves to the target, which already
        // exists, and there is nothing to do.
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        let archive = build_archive(
            base.path(),
            &[
                (".", EntryKind::Directory),
                ("file", EntryKind::Regular(b"hi")),
            ],
        )
        .await;

        let result = unpack_zstd(&target, &archive).await;

        assert!(
            result.is_ok(),
            "a root directory entry must be skipped, not fail the unpack: {result:?}"
        );
        assert_eq!(fs::read(target.join("file")).await.unwrap(), b"hi");
    }

    #[tokio::test]
    async fn rejects_an_entry_that_resolves_to_the_target_root() {
        // A Regular (not Symlink/hardlink) entry: without the explicit
        // empty-path rejection, this entry would hit the "destination
        // already exists as a directory" skip below (dest == target, which
        // is a directory), and the unpack would silently succeed having
        // written nothing — the same content-substitution risk the
        // Ok(false)-discarding defect this issue closes. A Symlink entry
        // named "." would also fail via remove_file(target) returning
        // EISDIR regardless of the explicit rejection, which would not
        // discriminate the behavior this test pins.
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        let archive = build_archive(base.path(), &[(".", EntryKind::Regular(b"hi"))]).await;

        let result = unpack_zstd(&target, &archive).await;

        let err = result
            .expect_err("an entry that sanitizes to the target root itself must refuse the unpack");
        assert!(
            format!("{err}").contains("resolves to the target root"),
            "error must name the specific rejection: {err}"
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

        // A fixed, short absolute name unrelated to the canary or to
        // $TMPDIR's length: the rejection fires on `is_absolute()` before
        // any filesystem call touches the named path, so the fixture need
        // not point at a real file, and must not be coupled to how long the
        // host's temp directory happens to be (a prior version used the
        // canary's own path, which can exceed the 100-byte ustar name field
        // on hosts with a long TMPDIR).
        let archive = build_archive(
            base.path(),
            &[(
                "/etc/vpl-336-unrelated-path",
                EntryKind::Symlink("irrelevant"),
            )],
        )
        .await;

        let result = unpack_zstd(&target, &archive).await;

        let err = result.expect_err("absolute-path entry must refuse the unpack");
        assert!(
            format!("{err}").contains("is absolute"),
            "error must name the absolute-path rejection: {err}"
        );
        assert!(canary.exists(), "canary must not be deleted");
        assert_eq!(
            fs::read(&canary).await.unwrap(),
            b"untouched",
            "canary must survive a refused unpack even though it is unrelated to the fixture"
        );
    }

    #[tokio::test]
    async fn rejects_symlink_mediated_escape_through_planted_intermediate() {
        // Two entries: the first plants a symlink inside the target
        // pointing at a real directory outside it; the second names a path
        // *through* that symlink and is lexically clean on its own (no
        // "..", not absolute), so the lexical sanitizer alone would pass it
        // through. Without resolving the intermediate component against
        // the real filesystem, the second entry's remove_file/chmod
        // operate outside the target.
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        let outside = base.path().join("outside");
        fs::create_dir_all(&outside).await.unwrap();
        let canary = outside.join("victim");
        fs::write(&canary, b"untouched").await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o555))
                .await
                .unwrap();
        }

        let archive = build_archive(
            base.path(),
            &[
                ("link", EntryKind::Symlink(outside.to_str().unwrap())),
                ("link/victim", EntryKind::Symlink("irrelevant")),
            ],
        )
        .await;

        let result = unpack_zstd(&target, &archive).await;

        let err = result.expect_err("escape through a planted symlink must refuse the unpack");
        assert!(
            format!("{err}").contains("traverses through a symlink"),
            "error must name the symlink traversal: {err}"
        );
        assert!(canary.exists(), "canary outside target must not be deleted");
        assert_eq!(
            fs::read(&canary).await.unwrap(),
            b"untouched",
            "canary outside target must survive a refused unpack"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&outside).await.unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o555,
                "the outside directory's permissions must not be touched"
            );
        }
    }

    #[tokio::test]
    async fn existing_directory_destination_is_skipped_without_error() {
        // Positive control for the "already extracted as a directory" skip:
        // a real, pre-existing directory at the destination must be left
        // alone and the entry silently skipped, not treated as an error.
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        let existing = target.join("existing");
        fs::create_dir_all(&existing).await.unwrap();

        let archive =
            build_archive(base.path(), &[("existing", EntryKind::Regular(b"ignored"))]).await;

        let result = unpack_zstd(&target, &archive).await;

        assert!(
            result.is_ok(),
            "an entry matching an existing real directory must be skipped, not fail: {result:?}"
        );
        assert!(existing.is_dir(), "the existing directory must survive");
    }

    #[tokio::test]
    async fn symlink_posing_as_extracted_directory_does_not_silently_swallow_the_entry() {
        // A symlink at the destination that happens to point at a real
        // directory must not be mistaken for "already extracted as a
        // directory" (which Path::is_dir() would do, since it follows
        // links): pre-fix, that mistake made this entry silently skipped —
        // Ok(()) with the symlink left untouched and the entry's content
        // never written anywhere. symlink_metadata does not follow the
        // link, so the entry now reaches tokio-tar's own write path, which
        // (per its "attackable to overwrite in place" comment) removes the
        // existing path — the symlink itself, not its target — and writes
        // a fresh regular file. The observable fix is: the destination
        // stops being a symlink and gains the archive's real content,
        // instead of silently staying exactly as it was.
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        fs::create_dir_all(&target).await.unwrap();

        let outside = base.path().join("outside");
        fs::create_dir_all(&outside).await.unwrap();
        let canary = outside.join("victim");
        fs::write(&canary, b"untouched").await.unwrap();

        let existing = target.join("existing");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &existing).unwrap();

        let archive = build_archive(
            base.path(),
            &[("existing", EntryKind::Regular(b"real-content"))],
        )
        .await;

        let result = unpack_zstd(&target, &archive).await;

        assert!(result.is_ok(), "the entry must not be dropped: {result:?}");
        #[cfg(unix)]
        assert!(
            !fs::symlink_metadata(&existing)
                .await
                .unwrap()
                .file_type()
                .is_symlink(),
            "the planted symlink must be replaced, not mistaken for the extracted entry"
        );
        assert_eq!(
            fs::read(&existing).await.unwrap(),
            b"real-content",
            "the entry's own content must be written, not silently dropped"
        );
        assert_eq!(
            fs::read(&canary).await.unwrap(),
            b"untouched",
            "content must never be written through the planted symlink into the outside dir"
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
            &[
                ("link", EntryKind::Symlink("new-target")),
                ("readonly/child", EntryKind::Regular(b"hi")),
            ],
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
