//! Container filesystem commands: `podnext copy`, `sync`, `reset`, `rename` and the hole-aware
//! copy machinery they share.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::sync::*`.

use anyhow::Result;
use serde::Serialize;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::native::{install_image_native, verify_image_available};
use crate::{output_json, output_yaml, OutputFormat};

/// Rename a container: the directory name *is* the identity, so the move plus a
/// rewritten `metadata.json` is the whole operation.
pub async fn cmd_rename(oldname: String, newname: String) -> Result<()> {
    if oldname == newname {
        anyhow::bail!("Source and destination names are identical");
    }
    proot_distro_core::NameValidator::new().validate_container_name(&newname)?;

    let mgr = proot_distro_core::ContainerManager::new()?;
    let container = mgr.get(&oldname).await?;

    let src_dir = container
        .rootfs_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("unexpected rootfs layout"))?
        .to_path_buf();
    let containers_dir = src_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("unexpected container layout"))?
        .to_path_buf();
    let dst_dir = containers_dir.join(&newname);

    if dst_dir.exists() {
        anyhow::bail!("Container '{}' already exists", newname);
    }

    std::fs::rename(&src_dir, &dst_dir)?;

    // metadata.json embeds both the name and the rootfs path; a stale copy makes
    // `list` disagree with the directory on disk.
    let meta_path = dst_dir.join("metadata.json");
    if let Ok(Some(raw)) = proot_distro_core::container::read_metadata_text(&dst_dir) {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("name".into(), serde_json::json!(newname));
                obj.insert(
                    "rootfs".into(),
                    serde_json::json!(dst_dir.join("rootfs").to_string_lossy()),
                );
            }
            std::fs::write(&meta_path, serde_json::to_string_pretty(&value)?)?;
        }
    }

    // Rewrite any link2symlink intermediate targets pointing to old rootfs
    let old_rootfs = src_dir.join("rootfs");
    let new_rootfs = dst_dir.join("rootfs");
    if new_rootfs.exists() {
        if let Ok(rewritten) =
            proot_distro_core::l2s::rewrite_l2s_tree(&new_rootfs, &old_rootfs, &new_rootfs)
        {
            if rewritten > 0 {
                pn_trace!(
                    "Rewrote {} link2symlink target(s) for renamed container",
                    rewritten
                );
            }
        }
    }

    println!("Renamed container '{}' to '{}'", oldname, newname);
    Ok(())
}

/// Split `[CONTAINER:]PATH`. A prefix is only a container when it has no path
/// separator, so an absolute host path is never mistaken for one.
pub(crate) fn split_spec(spec: &str) -> (Option<&str>, &str) {
    match spec.split_once(':') {
        Some((name, path)) if !name.is_empty() && !name.contains('/') => (Some(name), path),
        _ => (None, spec),
    }
}

/// Translate a `[CONTAINER:]PATH` spec to a host path.
///
/// The rootfs is a plain host directory, so a guest path is just the rootfs
/// joined with the guest path — no protocol needed. But it is a **trust
/// boundary**: the guest path comes from the user, and a naive `join` lets
/// `c1:../../../../etc/shadow` walk straight out of the container and read a
/// host file. Every component is therefore checked, and the result is confirmed
/// to still be inside the rootfs after symlinks are resolved.
async fn resolve_endpoint(spec: &str) -> Result<std::path::PathBuf> {
    match split_spec(spec) {
        (None, path) => Ok(std::path::PathBuf::from(path)),
        (Some(name), path) => {
            let mgr = proot_distro_core::ContainerManager::new()?;
            let container = mgr.get(name).await?;
            guest_to_host(&container.rootfs_path, path)
        }
    }
}

/// Join a guest path onto a rootfs, refusing anything that leaves it.
pub(crate) fn guest_to_host(
    rootfs: &std::path::Path,
    guest_path: &str,
) -> Result<std::path::PathBuf> {
    use std::path::Component;

    let mut resolved = rootfs.to_path_buf();
    for component in std::path::Path::new(guest_path).components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => resolved.push(part),
            // `..` is refused outright rather than normalised: inside a container
            // it can only ever mean "leave", and silently clamping it would make
            // the resulting path differ from what the user asked for.
            Component::ParentDir => {
                anyhow::bail!(
                    "path '{}' escapes the container rootfs (contains '..')",
                    guest_path
                )
            }
            Component::Prefix(_) => {
                anyhow::bail!("unsupported path component in '{}'", guest_path)
            }
        }
    }

    // Lexical checks cannot see a symlink inside the rootfs that points outside
    // it, so confirm against the real path when the target exists.
    if let (Ok(root), Ok(real)) = (
        std::fs::canonicalize(rootfs),
        std::fs::canonicalize(&resolved),
    ) {
        if !real.starts_with(&root) {
            anyhow::bail!(
                "path '{}' resolves outside the container rootfs ({})",
                guest_path,
                real.display()
            );
        }
    }

    Ok(resolved)
}

pub async fn cmd_copy(
    source: String,
    destination: String,
    recursive: bool,
    move_: bool,
) -> Result<()> {
    let dst = resolve_endpoint(&destination).await?;
    let raw_src = resolve_endpoint(&source).await?;

    // A plain copy follows the final symlink, exactly as `cp` does; `--move`
    // acts on the link itself. Without this, copying a guest path that happens
    // to be a link produces a dangling link on the host: Debian's
    // /etc/os-release is `../usr/lib/os-release`, which stops meaning anything
    // once it is outside the rootfs.
    let src_is_link = std::fs::symlink_metadata(&raw_src)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let src = if !move_ && src_is_link {
        let link = std::fs::read_link(&raw_src)?;
        if link.is_absolute() {
            link
        } else {
            raw_src
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(link)
        }
    } else {
        raw_src.clone()
    };

    let meta = std::fs::symlink_metadata(&src)
        .map_err(|_| anyhow::anyhow!("Source '{}' does not exist", source))?;
    let is_dir = meta.is_dir();
    if !is_dir && !meta.file_type().is_file() && !meta.file_type().is_symlink() {
        anyhow::bail!("Source '{}' is not a regular file or directory", source);
    }
    if is_dir && !recursive {
        anyhow::bail!("Source '{}' is a directory; pass --recursive", source);
    }

    // Copying onto an existing directory places the source inside it, as cp does.
    let dest = if dst.is_dir() {
        let name = src
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("source has no file name"))?;
        dst.join(name)
    } else {
        dst
    };

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if is_dir {
        copy_dir_recursive(&src, &dest)?;
    } else if meta.file_type().is_symlink() {
        // Only reachable with --move: the leaf link is moved as a link.
        let link = std::fs::read_link(&src)?;
        let _ = std::fs::remove_file(&dest);
        std::os::unix::fs::symlink(&link, &dest)?;
    } else {
        copy_preserving_holes(&src, &dest)?;
    }

    if move_ {
        if is_dir {
            std::fs::remove_dir_all(&src)?;
        } else {
            std::fs::remove_file(&src)?;
        }
        println!("Moved {} -> {}", source, destination);
    } else {
        println!("Copied {} -> {}", source, destination);
    }
    Ok(())
}

/// Reset a container: remove it and reinstall the image it came from.
///
/// This is destructive by definition — the same contract as proot-distro's
/// `reset` — so it says so before doing it, and it refuses to guess an image it
/// cannot prove the container came from.
pub async fn cmd_reset(name: String) -> Result<()> {
    let mgr = proot_distro_core::ContainerManager::new()?;
    let container = mgr.get(&name).await?;

    let container_dir = container
        .rootfs_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("unexpected rootfs layout"))?
        .to_path_buf();

    let Some(raw) = proot_distro_core::container::read_metadata_text(&container_dir)? else {
        anyhow::bail!(
            "Container '{}' has no metadata.json, so there is no recorded source image. \
             Reinstall it manually.",
            name
        );
    };
    let meta: serde_json::Value = serde_json::from_str(&raw)?;
    let image = meta["image"].as_str().unwrap_or_default().to_string();
    if image.is_empty() {
        anyhow::bail!(
            "Container '{}' has no recorded source image, so there is nothing to reinstall. \
             Reinstall it manually.",
            name
        );
    }
    // A restored container records the *container* it came from as its image;
    // that is not a registry reference and must not be sent to a registry.
    let tag = meta["tag"].as_str().unwrap_or_default();
    let image_ref = if tag.is_empty() || tag == "restored" {
        image.clone()
    } else {
        format!("{}:{}", image, tag)
    };

    println!("Resetting '{}' from '{}'.", name, image_ref);
    println!("This deletes the container and everything inside it.");

    // Fail before the irreversible step. A `backup restore` records the *source
    // container* as the image, which is not a registry reference; the old order
    // deleted the container first and only then discovered the image could not be
    // fetched, losing the container with nothing to replace it.
    verify_image_available(&image_ref).await?;

    std::fs::remove_dir_all(&container_dir)?;
    install_image_native(&image_ref, &name, false, None).await?;

    println!("\nContainer '{}' reset to a clean {}", name, image_ref);
    Ok(())
}

/// Counters for one sync run, so the command reports what it actually did.
#[derive(Default)]
pub(crate) struct SyncReport {
    pub(crate) copied: usize,
    pub(crate) unchanged: usize,
    pub(crate) removed: usize,
    pub(crate) skipped: usize,
}

/// Mirror `source` onto `destination`.
///
/// Comparison is by content (size first, then sha256), not by mtime: PDN has no
/// dependency that can set a destination's mtime back, so an mtime comparison
/// would report every file as changed on every run. Hashing costs more and is
/// the only honest option here.
pub async fn cmd_sync(source: String, destination: String, delete: bool) -> Result<()> {
    let src = resolve_endpoint(&source).await?;
    let dst = resolve_endpoint(&destination).await?;

    let src_meta = std::fs::symlink_metadata(&src)
        .map_err(|_| anyhow::anyhow!("Source '{}' does not exist", source))?;

    // Without this, `--delete` against a file source silently does nothing,
    // which is the bug the reference implementation documents avoiding.
    if delete && !src_meta.is_dir() {
        anyhow::bail!("--delete requires a directory as the source");
    }

    let mut report = SyncReport::default();

    if src_meta.is_dir() {
        mirror_dir(&src, &dst, &mut report)?;
        if delete {
            prune_extras(&src, &dst, &mut report)?;
        }
    } else {
        sync_one(&src, &dst, &mut report)?;
    }

    println!(
        "Synced {} -> {}: {} copied, {} unchanged, {} removed, {} skipped",
        source, destination, report.copied, report.unchanged, report.removed, report.skipped
    );
    Ok(())
}

pub(crate) fn file_digest(path: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Copy `src` onto `dst`, keeping holes as holes.
///
/// `std::fs::copy` densifies a sparse source: on Android/aarch64 its
/// `copy_file_range` fast path is unavailable, so it falls back to a loop that
/// reads and writes every logical byte — a 1 GiB file holding 16 KiB of data
/// becomes a 1 GiB allocation (issue #18). Walking the source with
/// `SEEK_DATA`/`SEEK_HOLE` copies only the live regions; the gaps stay holes and
/// the final length is fixed with `set_len`, so non-sparse content is still
/// byte-exact. A filesystem without `SEEK_DATA` support takes the old dense path
/// rather than failing.
pub(crate) fn copy_preserving_holes(src: &std::path::Path, dst: &std::path::Path) -> Result<u64> {
    let src_len = std::fs::metadata(src)?.len();
    let mut input = std::fs::File::open(src)?;
    let mut output = std::fs::File::create(dst)?;

    if let Err(e) = copy_data_regions(&mut input, &mut output) {
        // A half-written destination is worse than none; `fs::copy` also removes
        // the target when it fails.
        let _ = std::fs::remove_file(dst);
        return Err(e.into());
    }

    // Extend a trailing hole (or shrink a longer destination) to the source size.
    output.set_len(src_len)?;
    if let Ok(meta) = std::fs::metadata(src) {
        let _ = std::fs::set_permissions(dst, meta.permissions());
    }
    Ok(src_len)
}

/// Copy only the allocated regions, leaving the source's holes as holes.
pub(crate) fn copy_data_regions(
    input: &mut std::fs::File,
    output: &mut std::fs::File,
) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::io::AsRawFd;

    let fd = input.as_raw_fd();
    let mut buf = vec![0u8; 128 * 1024];
    let mut offset: u64 = 0;

    loop {
        let data = match lseek(fd, offset, libc::SEEK_DATA) {
            Ok(p) => p,
            // No data at or after `offset`: the rest of the file is a hole.
            Err(e) if e.raw_os_error() == Some(libc::ENXIO) => break,
            // The filesystem has no SEEK_DATA/SEEK_HOLE: copy it dense instead.
            Err(e)
                if offset == 0
                    && matches!(e.raw_os_error(), Some(libc::EINVAL) | Some(libc::ENOTSUP)) =>
            {
                return std::io::copy(input, output).map(|_| ());
            }
            Err(e) => return Err(e),
        };
        let hole = lseek(fd, data, libc::SEEK_HOLE)?;
        input.seek(SeekFrom::Start(data))?;
        output.seek(SeekFrom::Start(data))?;
        let mut left = hole - data;
        while left > 0 {
            let want = left.min(buf.len() as u64) as usize;
            let n = input.read(&mut buf[..want])?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n])?;
            left -= n as u64;
        }
        offset = hole;
    }
    Ok(())
}

/// `lseek(2)` returning an absolute offset, so `SEEK_DATA`/`SEEK_HOLE` can drive
/// the hole-aware copy above.
pub(crate) fn lseek(fd: i32, offset: u64, whence: i32) -> std::io::Result<u64> {
    let ret = unsafe { libc::lseek(fd, offset as libc::off_t, whence) };
    if ret < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(ret as u64)
}

/// True when the destination already holds this content.
pub(crate) fn same_content(src: &std::path::Path, dst: &std::path::Path) -> Result<bool> {
    let s = std::fs::symlink_metadata(src)?;
    let Ok(d) = std::fs::symlink_metadata(dst) else {
        return Ok(false);
    };
    if !d.is_file() || !s.is_file() {
        return Ok(false);
    }
    if s.len() != d.len() {
        return Ok(false);
    }
    Ok(file_digest(src)? == file_digest(dst)?)
}

/// Sync a single file, placing it inside `dst` when `dst` is a directory.
pub(crate) fn sync_one(
    src: &std::path::Path,
    dst: &std::path::Path,
    report: &mut SyncReport,
) -> Result<()> {
    let meta = std::fs::symlink_metadata(src)?;

    let dst_is_dir = std::fs::symlink_metadata(dst)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    let target = if dst_is_dir {
        let name = src
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("source has no file name"))?;
        dst.join(name)
    } else {
        dst.to_path_buf()
    };

    if meta.file_type().is_symlink() {
        let link = std::fs::read_link(src)?;
        let already = std::fs::read_link(&target).ok().as_deref() == Some(link.as_path());
        if already {
            report.unchanged += 1;
            return Ok(());
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_file(&target);
        std::os::unix::fs::symlink(&link, &target)?;
        report.copied += 1;
        return Ok(());
    }

    // A directory standing where the source has a file is replaced, never
    // descended into: that link may lead somewhere unexpected.
    if std::fs::symlink_metadata(&target)
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        std::fs::remove_dir_all(&target)?;
    }

    if same_content(src, &target)? {
        report.unchanged += 1;
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    copy_preserving_holes(src, &target)?;
    std::fs::set_permissions(&target, meta.permissions())?;
    report.copied += 1;
    Ok(())
}

/// Walk `src` and mirror every entry into `dst`. Ownership is never changed
/// (an unprivileged Android process cannot), and modes are applied after the
/// content so a read-only source directory stays writable while its children go
/// in.
pub(crate) fn mirror_dir(
    src: &std::path::Path,
    dst: &std::path::Path,
    report: &mut SyncReport,
) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());

        if file_type.is_dir() {
            mirror_dir(&path, &target, report)?;
        } else {
            sync_one(&path, &target, report)?;
        }
    }

    if let Ok(meta) = std::fs::metadata(src) {
        let _ = std::fs::set_permissions(dst, meta.permissions());
    }
    Ok(())
}

/// Remove destination entries that have no counterpart in the source.
///
/// Only meaningful after a successful mirror pass; the caller guarantees the
/// source directory was listable, or every destination entry would look like an
/// orphan.
pub(crate) fn prune_extras(
    src: &std::path::Path,
    dst: &std::path::Path,
    report: &mut SyncReport,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dst) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let counterpart = src.join(entry.file_name());

        if std::fs::symlink_metadata(&counterpart).is_err() {
            if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
            report.removed += 1;
            continue;
        }

        if entry.file_type()?.is_dir() {
            prune_extras(&counterpart, &path, report)?;
        }
    }
    Ok(())
}

/// Recursive copy that keeps symlinks as symlinks, so a copied tree does not
/// silently balloon the way following every link would.
pub(crate) fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if file_type.is_symlink() {
            let link = std::fs::read_link(entry.path())?;
            let _ = std::fs::remove_file(&target);
            std::os::unix::fs::symlink(&link, &target)?;
        } else {
            copy_preserving_holes(&entry.path(), &target)?;
        }
    }

    Ok(())
}
