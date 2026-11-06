//! Archive extraction that survives Android.
//!
//! `tar::Archive::unpack` is the obvious call and the wrong one here. This
//! device's SELinux policy refuses `link(2)` **outright** — verified: `ln` on
//! two names in the same app-owned directory on f2fs returns
//! `Permission denied`. Any image whose layers contain hard links therefore
//! aborts the whole install:
//!
//! ```text
//! Error: failed to unpack `.../rootfs/usr/bin/perl5.40.1`
//! Caused by: Permission denied (os error 13) when hard linking
//!            .../usr/bin/perl to .../usr/bin/perl5.40.1
//! ```
//!
//! Alpine installs fine (busybox applets are symlinks), Debian does not
//! (`perl` -> `perl5.40.1` is a hard link), which is exactly the difference the
//! user hit.
//!
//! So hard links are materialised as independent copies: identical bytes, which
//! is all the guest needs, at the cost of the deduplication the link provided.
//! Special files (device nodes, FIFOs) cannot be created without root and are
//! skipped rather than failing the extraction.
//!
//! Ownership is not restored — an unprivileged Android app cannot `chown`.
//! Modes are applied, after the content is written so a read-only bit cannot
//! block the write.
//!
//! OCI whiteout markers are obeyed. `layer_diff.rs` writes them when it diffs
//! two trees, and until this side read them the loop was open: a layer that
//! deleted `/etc/old.conf` produced `.wh.old.conf`, extraction created that
//! marker as an ordinary empty file, and the deleted file was still there
//! afterwards. See `remove_whiteouted`.
//!
//! The *opaque directory* marker (`.wh..wh..opq`) is deliberately not handled:
//! nothing in this project emits it, and only a foreign layer could carry one.
//! That arrives with the layer import in v0.2.0, and it needs a test that shows
//! the lower layer's contents disappearing — which cannot be written against a
//! code path that does not exist yet.

use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tar::{Archive, EntryType};

/// `create_dir_all` calls made on this thread — thread-local so parallel tests
/// cannot see each other's work. The regression test below counts it instead of
/// timing anything, so it cannot flake on a slow device.
#[cfg(test)]
thread_local! {
    pub(crate) static DIR_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Unpack a tar stream into `dest`, creating it if needed.
pub fn extract<R: Read>(reader: R, dest: &Path) -> Result<()> {
    let mut archive = Archive::new(reader);
    extract_entries(&mut archive, dest)
}

/// Unpack an already-open archive, for callers that need to inspect it first.
pub fn extract_entries<R: Read>(archive: &mut Archive<R>, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    // Directories already known to exist this run. Without it every entry
    // re-walks its whole ancestor chain through `create_dir_all` (a `mkdir`
    // plus a `stat`, each resolving every component), so a tree `d` levels
    // deep with `n` entries below it paid `n * d` resolutions.
    let mut created: HashSet<PathBuf> = HashSet::new();
    created.insert(dest.to_path_buf());

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let kind = entry.header().entry_type();
        let target = dest.join(&path);

        // Archive paths are attacker-controlled in the general case; refuse to
        // travel outside the destination.
        if !is_inside(dest, &target) {
            continue;
        }
        // ...and refuse to travel *through* one. An earlier entry (or a lower
        // layer) may have planted a symlink, and `File::create`/`create_dir_all`
        // follow it, so this entry's bytes would land wherever the link points —
        // `is_inside` only normalises the name and cannot see the link on disk.
        if let Some(link) = first_symlink_component(dest, &target) {
            return Err(anyhow!(
                "refusing entry `{}`: `{}` is a symlink on its path, which would carry the \
                 write outside `{}`",
                path.display(),
                link.display(),
                dest.display()
            ));
        }
        if let Some(parent) = target.parent() {
            ensure_dir(parent, &mut created)?;
        }

        // An OCI whiteout is an instruction, not a file: `.wh.<name>` means `<name>` is gone in
        // this layer. Creating the marker instead is how a deleted file comes back one extraction
        // later, wearing a `.wh.` prefix.
        if let Some(hidden) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(".wh."))
        {
            remove_whiteouted(dest, &target, hidden, &mut created)?;
            continue;
        }

        match kind {
            EntryType::Directory => {
                ensure_dir(&target, &mut created)?;
            }
            EntryType::Regular | EntryType::Continuous => {
                let mut out = fs::File::create(&target)?;
                io::copy(&mut entry, &mut out)?;
            }
            EntryType::Symlink => {
                let link = entry
                    .link_name()?
                    .ok_or_else(|| anyhow!("symlink without target: {}", path.display()))?
                    .into_owned();
                let _ = fs::remove_file(&target);
                std::os::unix::fs::symlink(&link, &target)?;
            }
            EntryType::Link => {
                let link = entry
                    .link_name()?
                    .ok_or_else(|| anyhow!("hard link without target: {}", path.display()))?
                    .into_owned();
                let source = dest.join(&link);
                let _ = fs::remove_file(&target);
                // Ask for a real hard link first; on a device that permits it
                // this keeps the layer's deduplication. Everywhere else, copy.
                if fs::hard_link(&source, &target).is_err() {
                    if source.is_file() {
                        fs::copy(&source, &target)?;
                    } else {
                        // The link target was itself skipped (a special file);
                        // nothing to copy, so skip this name too.
                        continue;
                    }
                }
            }
            // Character/block devices and FIFOs need privileges we do not have.
            _ => continue,
        }

        if let Ok(mode) = entry.header().mode() {
            let bits = if kind == EntryType::Directory {
                0o755
            } else {
                mode & 0o777
            };
            let _ = fs::set_permissions(&target, fs::Permissions::from_mode(bits));
        }
    }

    Ok(())
}

/// Create `dir` unless this run already did.
///
/// `fs::create_dir_all` on a path that already exists still performs a `mkdir`
/// (fails `EEXIST`) and then a `stat`, and each of those resolves every one of
/// the path's ancestors. Archives are walked entry by entry, so a deep tree
/// with many entries below it paid that ancestor walk once per entry — the
/// super-linear cost behind a 200-level extraction that never finished. Each
/// directory is created once here; every later entry under it is a lookup.
fn ensure_dir(dir: &Path, created: &mut HashSet<PathBuf>) -> io::Result<()> {
    if created.contains(dir) {
        return Ok(());
    }
    #[cfg(test)]
    DIR_CALLS.with(|c| c.set(c.get() + 1));
    fs::create_dir_all(dir)?;
    // Only `dir` itself is recorded, not its ancestors: listing the chain would
    // cost a `PathBuf` clone per level. Archives list a directory before its
    // contents, so the ancestor is normally already known; an out-of-order
    // archive just pays the walk it paid before.
    created.insert(dir.to_path_buf());
    Ok(())
}

/// Apply one whiteout: delete what the marker names, and never create the marker.
///
/// This is the only place in this module that removes something the archive did not put there, so
/// the victim is re-checked against `dest` after the `.wh.` prefix is stripped — `is_inside` ran on
/// the marker's own path, which is not the path being deleted. Deletion does not follow symlinks:
/// `symlink_metadata` classifies the entry itself, so a whiteout on a name that is a symlink
/// removes the link and leaves its target alone.
fn remove_whiteouted(
    dest: &Path,
    marker: &Path,
    hidden: &str,
    created: &mut HashSet<PathBuf>,
) -> Result<()> {
    if hidden.is_empty() {
        return Ok(()); // ".wh." names nothing
    }
    let victim = marker.with_file_name(hidden);
    if !is_inside(dest, &victim) {
        return Ok(());
    }
    match fs::symlink_metadata(&victim) {
        Ok(meta) if meta.is_dir() => {
            fs::remove_dir_all(&victim)?;
            // A later entry under this name must `mkdir` again rather than trust the cache.
            created.remove(&victim);
        }
        Ok(_) => fs::remove_file(&victim)?,
        // Already absent: a whiteout for a name no lower layer had. Nothing to do, and not an
        // error — the layer states an absence that already holds.
        Err(_) => {}
    }
    Ok(())
}

/// True when `path` stays within `root` once `..` is resolved.
fn is_inside(root: &Path, path: &Path) -> bool {
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return false;
                }
            }
            std::path::Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.starts_with(root)
}

/// The first component of `target` below `dest` that already exists and is a
/// symlink, if any.
///
/// `symlink_metadata` classifies each component itself, so a link that points
/// out of the tree is caught without ever following it. A path whose components
/// are all real (or not yet created — the directory is made later) finds
/// nothing, which is the legitimate case.
fn first_symlink_component(dest: &Path, target: &Path) -> Option<PathBuf> {
    let relative = target.strip_prefix(dest).ok()?;
    let mut current = dest.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if let Ok(meta) = fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() {
                return Some(current);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tar::Builder;

    /// Build a tar in memory: a file, a symlink, a hard link to the file, and a
    /// traversal attempt.
    fn sample_tar() -> Vec<u8> {
        let mut builder = Builder::new(Vec::new());

        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_mode(0o644);
        header.set_entry_type(EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, "usr/bin/perl", &b"hello"[..])
            .unwrap();

        let mut link = tar::Header::new_gnu();
        link.set_size(0);
        link.set_mode(0o644);
        link.set_entry_type(EntryType::Link);
        link.set_link_name("usr/bin/perl").unwrap();
        link.set_cksum();
        builder
            .append_data(&mut link, "usr/bin/perl5.40.1", io::empty())
            .unwrap();

        let mut sym = tar::Header::new_gnu();
        sym.set_size(0);
        sym.set_mode(0o777);
        sym.set_entry_type(EntryType::Symlink);
        sym.set_link_name("/usr/bin/perl").unwrap();
        sym.set_cksum();
        builder
            .append_data(&mut sym, "usr/bin/perl-link", io::empty())
            .unwrap();

        let mut escape = tar::Header::new_gnu();
        escape.set_size(3);
        escape.set_mode(0o644);
        escape.set_entry_type(EntryType::Regular);
        escape.set_cksum();
        // The tar crate itself refuses `..` in a path when building, which is
        // its own guard; `is_inside` is the defence for archives that arrive
        // from elsewhere, and is unit-tested separately.
        let _ = builder.append_data(&mut escape, "escaped", &b"bad"[..]);

        builder.into_inner().unwrap()
    }

    #[test]
    fn hard_links_become_copies_even_where_link_is_denied() {
        let dir = std::env::temp_dir().join(format!("pdn-untar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        extract(sample_tar().as_slice(), &dir).expect("extract");

        // Content of the linked name matches the original byte for byte...
        assert_eq!(fs::read(dir.join("usr/bin/perl5.40.1")).unwrap(), b"hello");
        // ...and it is a usable regular file regardless of whether the OS
        // granted us a link or we copied.
        let meta = fs::symlink_metadata(dir.join("usr/bin/perl5.40.1")).unwrap();
        assert!(meta.file_type().is_file());

        // Symlinks stay symlinks.
        let link = fs::symlink_metadata(dir.join("usr/bin/perl-link")).unwrap();
        assert!(link.file_type().is_symlink());
        assert_eq!(
            fs::read_link(dir.join("usr/bin/perl-link")).unwrap(),
            std::path::Path::new("/usr/bin/perl")
        );

        // A plain name inside the archive lands inside the destination, and nothing
        // escapes it.
        assert!(dir.join("escaped").exists());
        assert!(!dir.parent().unwrap().join("escaped").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    /// Append one entry with `size` bytes of filler.
    fn append(builder: &mut Builder<Vec<u8>>, name: &str, kind: EntryType, size: usize) {
        let mut header = tar::Header::new_gnu();
        header.set_size(size as u64);
        header.set_mode(if kind == EntryType::Directory {
            0o755
        } else {
            0o644
        });
        header.set_entry_type(kind);
        header.set_cksum();
        builder
            .append_data(&mut header, name, &vec![b'x'; size][..])
            .unwrap();
    }

    /// Issue #16. A deep tree used to make every entry walk its whole ancestor
    /// chain again through `create_dir_all`, so a few hundred levels with files
    /// under them took minutes. The work is counted, not timed, so this cannot
    /// flake on a slow device.
    #[test]
    fn deep_tree_creates_each_directory_once() {
        const DEPTH: usize = 150;
        const FILES: usize = 400;

        let dir = std::env::temp_dir().join(format!("pdn-untar-deep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut builder = Builder::new(Vec::new());
        let mut deep = String::from("root");
        for i in 0..DEPTH {
            deep.push_str(&format!("/d{}", i));
            append(&mut builder, &format!("{}/", deep), EntryType::Directory, 0);
        }
        for k in 0..FILES {
            append(
                &mut builder,
                &format!("{}/f{}", deep, k),
                EntryType::Regular,
                4,
            );
        }
        let tar = builder.into_inner().unwrap();

        DIR_CALLS.with(|c| c.set(0));
        extract(tar.as_slice(), &dir).expect("extract deep tree");
        let calls = DIR_CALLS.with(|c| c.get());

        // The tree is really there, all the way down.
        assert!(dir.join(&deep).join(format!("f{}", FILES - 1)).exists());

        // One creation per distinct directory is the floor. Anything that grows
        // with FILES means the ancestor chain is being re-walked per entry.
        assert!(
            calls <= DEPTH + 2,
            "{calls} create_dir_all calls for {FILES} files under {DEPTH} levels: \
             the ancestor chain is walked once per entry, not once per directory"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_inside_rejects_traversal() {
        let root = Path::new("/tmp/root");
        assert!(is_inside(root, Path::new("/tmp/root/a/b")));
        assert!(is_inside(root, Path::new("/tmp/root/./a")));
        assert!(!is_inside(root, Path::new("/tmp/root/../escaped")));
        assert!(!is_inside(root, Path::new("/tmp/elsewhere")));
    }

    fn whiteout_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdn-untar-wh-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The defect this closes: `layer_diff` writes `.wh.old.conf` for a deleted file, and the
    /// extractor used to create that marker as an empty file — so the layer's deletion was lost
    /// and the marker leaked into the guest's filesystem.
    #[test]
    fn a_whiteout_removes_what_the_lower_layer_had() {
        let dir = whiteout_dir("file");
        fs::create_dir_all(dir.join("etc")).unwrap();
        fs::write(dir.join("etc/old.conf"), b"lower layer").unwrap();
        fs::write(dir.join("etc/keep.conf"), b"still here").unwrap();

        let mut builder = Builder::new(Vec::new());
        append(&mut builder, "etc/.wh.old.conf", EntryType::Regular, 0);
        let tar = builder.into_inner().unwrap();

        extract(tar.as_slice(), &dir).expect("extract");

        assert!(
            !dir.join("etc/old.conf").exists(),
            "the whiteout did not delete the file"
        );
        assert!(
            !dir.join("etc/.wh.old.conf").exists(),
            "the marker became a file in the tree"
        );
        assert!(
            dir.join("etc/keep.conf").exists(),
            "an unrelated file was removed"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_whiteout_removes_a_directory_and_everything_under_it() {
        let dir = whiteout_dir("dir");
        fs::create_dir_all(dir.join("data/deep")).unwrap();
        fs::write(dir.join("data/deep/x"), b"x").unwrap();

        let mut builder = Builder::new(Vec::new());
        append(&mut builder, ".wh.data", EntryType::Regular, 0);
        let tar = builder.into_inner().unwrap();

        extract(tar.as_slice(), &dir).expect("extract");
        assert!(
            !dir.join("data").exists(),
            "the directory survived its whiteout"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Deletion must not follow a symlink: the whiteout names the link, so the link goes and
    /// whatever it pointed at (outside the destination) stays.
    #[test]
    fn a_whiteout_does_not_follow_a_symlink_out_of_the_tree() {
        let dir = whiteout_dir("symlink");
        let outside = dir
            .parent()
            .unwrap()
            .join(format!("pdn-wh-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"outside the destination").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("victim")).unwrap();

        let mut builder = Builder::new(Vec::new());
        append(&mut builder, ".wh.victim", EntryType::Regular, 0);
        let tar = builder.into_inner().unwrap();

        extract(tar.as_slice(), &dir).expect("extract");

        assert!(
            !dir.join("victim").exists(),
            "the symlink itself should be gone"
        );
        assert!(
            outside.join("keep.txt").exists(),
            "the whiteout followed the link and deleted outside the destination"
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    /// The negative control for the whiteout branch: a marker naming something no lower layer had
    /// is a no-op, not an error. Without it, a delete that silently failed every time would leave
    /// the tests above still passing, because the names they assert on would simply be absent.
    #[test]
    fn a_whiteout_for_a_name_that_never_existed_is_a_no_op() {
        let dir = whiteout_dir("absent");
        fs::write(dir.join("unrelated.txt"), b"present").unwrap();

        let mut builder = Builder::new(Vec::new());
        append(&mut builder, ".wh.never-existed", EntryType::Regular, 0);
        let tar = builder.into_inner().unwrap();

        extract(tar.as_slice(), &dir).expect("a whiteout for an absent name must not fail");
        assert!(dir.join("unrelated.txt").exists());
        assert!(!dir.join(".wh.never-existed").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// The defect this closes: a symlink entry that points outside `dest`, followed by a regular
    /// entry whose path goes *through* that link. `File::create` follows the link, so the later
    /// file's bytes land outside the extraction root — the classic tar write-through. The archive
    /// is built in memory; reaching the bug needs no real image.
    #[test]
    fn a_symlink_cannot_carry_a_later_entry_out_of_the_tree() {
        let dir = std::env::temp_dir().join(format!("pdn-untar-wt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // A sibling of the destination, holding a sentinel that must survive.
        let outside = dir
            .parent()
            .unwrap()
            .join(format!("pdn-untar-wt-out-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"untouched").unwrap();

        let mut builder = Builder::new(Vec::new());

        // (a) a symlink inside the archive that points outside the destination.
        let mut sym = tar::Header::new_gnu();
        sym.set_size(0);
        sym.set_mode(0o777);
        sym.set_entry_type(EntryType::Symlink);
        sym.set_link_name(&outside).unwrap();
        sym.set_cksum();
        builder
            .append_data(&mut sym, "escape", io::empty())
            .unwrap();

        // (b) a later regular entry whose path is reached through that symlink.
        let mut pwn = tar::Header::new_gnu();
        pwn.set_size(5);
        pwn.set_mode(0o644);
        pwn.set_entry_type(EntryType::Regular);
        pwn.set_cksum();
        builder
            .append_data(&mut pwn, "escape/pwned", &b"pwned"[..])
            .unwrap();

        let tar = builder.into_inner().unwrap();
        let result = extract(tar.as_slice(), &dir);

        // The bytes must never leave the destination, on either outcome.
        assert!(
            !outside.join("pwned").exists(),
            "the later entry was written through the archive's symlink, outside the destination"
        );
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"untouched");
        assert_eq!(
            fs::read_dir(&outside).unwrap().count(),
            1,
            "the destination's sibling gained an entry"
        );

        // ...and the write-through is refused by name, not silently skipped.
        let err = result.expect_err("the write-through must be refused, not allowed");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("escape/pwned"),
            "the refusal must name the offending entry: {msg}"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    // A whiteout whose own path escapes the destination cannot be built with this crate: the `tar`
    // builder refuses `..` in a name (see `sample_tar`) and refuses absolute names too. The guard
    // that would stop such an archive, `is_inside`, has its own unit test instead, and it runs
    // before the whiteout branch, on the marker's own path. Hand-assembling header bytes to defeat
    // the builder would exercise a path reachable only from a foreign layer, which arrives with
    // the v0.2.0 import work - where a real hostile-layer fixture belongs.
}
