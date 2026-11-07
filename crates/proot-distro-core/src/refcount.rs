//! Reference-counted eviction for the store (WORKLIST C6).
//!
//! A store unit is deleted exactly when nothing references it: a unit that is
//! still referenced is never deleted, and a unit whose references reach zero is
//! always deleted. The count is **derived**, not stored — it is the size of the
//! set of live references that name the unit, read from the store's own
//! reference source. The reference does the same: `clear-cache --orphan` builds
//! its keep-set from the manifest cache and the build index
//! (`commands/clear_cache.py:_referenced_blob_names`) rather than trusting a
//! counter. A stored count is a second bookkeeping that can disagree with the
//! store it describes, and a count that disagrees deletes live data.
//!
//! ## What was deliberately not built (R3)
//!
//! Leases and the label-graph GC are **out of scope**: the second dissection
//! round puts them out of scope for a single-writer store. What survives is the
//! count. There is no expiry, no renewal, no lease table — a reference is live
//! for exactly as long as its holder is.
//!
//! ## Units today, and how blobs join later
//!
//! The only store unit with a live reference source today is the **staging
//! tree** an install builds a container in before it owns the container's name
//! (`staging_units`). A staging tree is referenced by the live install holding
//! its sidecar `<tree>.lock`; nothing references it once that lock is absent or
//! free, which is the same liveness signal the install path already reads.
//!
//! A content blob (WORKLIST C1) joins by adding one scanner that yields `Unit`s
//! whose `refs` come from the manifest cache and the build index — the reference's
//! two sources. [`evict`] does not change: it deletes iff `refs` is empty.
//!
//! Two things are deliberately **not** units:
//!   * a committed container's rootfs is a *root*, not a refcounted unit — the
//!     operator's data is removed by `remove`, never by an eviction sweep;
//!   * session artifacts already have one accounting (`session::active_sessions`
//!     plus `clear_container_sessions`, A2/A9). A second one here would be the
//!     two-renderings defect that A2 exists to close.

use crate::container::STAGING_PREFIX;
use std::collections::BTreeSet;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// Prefix of the directory a `--force` reinstall moves the superseded container
/// aside to while it swaps the replacement in. Nothing can reference it: the
/// container it held has already been superseded.
pub const REPLACED_PREFIX: &str = ".tmp-replaced-";

/// Identities of the live things that reference one unit. The reference count is
/// `.len()`; the identities are carried so a refusal can name them.
pub type Refs = BTreeSet<String>;

/// One deletable thing in the store, with the references that keep it alive.
#[derive(Debug, Clone)]
pub struct Unit {
    /// The name the refusal quotes, and the store entry this unit removes.
    pub key: String,
    /// What is removed when the count reaches zero.
    pub path: PathBuf,
    /// Live references. Empty means collectable.
    pub refs: Refs,
}

/// The outcome of one eviction decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eviction {
    /// `refs` was empty: the unit was removed.
    Evicted { key: String },
    /// `refs` was non-empty: nothing was touched, and `message` says why.
    Refused { key: String, message: String },
    /// `refs` was empty but the removal itself failed (permissions, a busy
    /// mount). Reported rather than swallowed — a silent failure leaves a stale
    /// unit the next sweep keeps trying to delete.
    Failed { key: String, error: String },
}

impl Eviction {
    /// The line a caller prints for a refusal, or `None` when nothing needs
    /// saying. This is the exact string `podnext` writes when it will not evict.
    pub fn message(&self) -> Option<String> {
        match self {
            Eviction::Refused { message, .. } => Some(message.clone()),
            Eviction::Evicted { .. } | Eviction::Failed { .. } => None,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            Eviction::Evicted { key }
            | Eviction::Refused { key, .. }
            | Eviction::Failed { key, .. } => key,
        }
    }
}

/// The exact refusal line for a unit whose reference count is non-zero.
///
/// One format, in one place, so the count and the holders in the message are
/// always the ones the decision was made on.
pub fn refusal_message(key: &str, refs: &Refs) -> String {
    format!(
        "refusing to evict '{}': {} reference(s) still hold it ({})",
        key,
        refs.len(),
        refs.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Evict `unit` iff nothing references it.
///
/// The check is the whole contract: a non-empty `refs` returns [`Eviction::Refused`]
/// **without touching the path**, so a live unit cannot be deleted by a sweep that
/// raced its owner.
pub fn evict(unit: &Unit) -> Eviction {
    if !unit.refs.is_empty() {
        return Eviction::Refused {
            key: unit.key.clone(),
            message: refusal_message(&unit.key, &unit.refs),
        };
    }
    match remove(&unit.path) {
        Ok(()) => Eviction::Evicted {
            key: unit.key.clone(),
        },
        Err(error) => Eviction::Failed {
            key: unit.key.clone(),
            error: error.to_string(),
        },
    }
}

/// Evict every unit whose references reached zero, in order.
pub fn sweep(units: &[Unit]) -> Vec<Eviction> {
    units.iter().map(evict).collect()
}

/// Remove `path` whether it is a directory or a file, never following a symlink.
///
/// `symlink_metadata` classifies the directory entry itself: a planted symlink
/// under the store is unlinked, never descended into. A path that is already
/// gone counts as success, so two units naming one entry (a staging tree and its
/// sidecar) are order-independent.
fn remove(path: &Path) -> std::io::Result<()> {
    let md = match std::fs::symlink_metadata(path) {
        Ok(md) => md,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if md.file_type().is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// `<tree>.lock` — the sidecar whose `flock` state is a staging tree's liveness.
pub fn sidecar_lock_path(tree: &Path) -> PathBuf {
    let mut name = tree.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

/// The store units owned by the container store, each with its reference set.
///
/// Reference source: the sidecar lock. An install takes an exclusive `flock` on
/// `<tree>.lock` before it creates the tree and holds it for its whole run, so a
/// lock this process can take means its owner is gone and the tree is residue.
/// The derivation is the same one the install path has always used; what is new
/// is that it now answers with a **count** (one live holder, or none) instead of
/// a keep/delete bit.
pub fn staging_units(containers_dir: &Path) -> Vec<Unit> {
    let mut units = Vec::new();
    let Ok(entries) = std::fs::read_dir(containers_dir) else {
        return units;
    };
    for entry in entries.flatten() {
        let key = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();

        if key.starts_with(REPLACED_PREFIX) {
            units.push(Unit {
                key,
                path,
                refs: Refs::new(),
            });
            continue;
        }
        if !key.starts_with(STAGING_PREFIX) {
            continue;
        }

        let is_dir = std::fs::symlink_metadata(&path)
            .map(|md| md.file_type().is_dir())
            .unwrap_or(false);
        if !is_dir {
            // A sidecar whose tree still exists is decided with that tree (its
            // count is the tree's count), so it is skipped here.
            if let Some(owner) = key.strip_suffix(".lock") {
                if containers_dir.join(owner).exists() {
                    continue;
                }
            }
            units.push(Unit {
                key,
                path,
                refs: Refs::new(),
            });
            continue;
        }

        let refs = sidecar_refs(&path);
        if refs.is_empty() {
            // The tree is residue; its sidecar goes with it, in the same pass.
            let lock = sidecar_lock_path(&path);
            if std::fs::symlink_metadata(&lock).is_ok() {
                units.push(Unit {
                    key: format!("{key}.lock"),
                    path: lock,
                    refs: Refs::new(),
                });
            }
        }
        units.push(Unit { key, path, refs });
    }
    units
}

/// The references holding one staging tree: exactly the live install that owns
/// its sidecar lock, or none.
fn sidecar_refs(tree: &Path) -> Refs {
    let mut refs = Refs::new();
    let lock_path = sidecar_lock_path(tree);
    let md = match std::fs::symlink_metadata(&lock_path) {
        Ok(md) => md,
        // No sidecar: no install ever claimed this tree.
        Err(_) => return refs,
    };
    if !md.file_type().is_file() {
        // An unreadable reference is not an absent one (clear_cache makes the same
        // distinction): a lock we cannot read must not read as "nothing holds it".
        refs.insert("unreadable-lock".to_string());
        return refs;
    }
    let file = match std::fs::File::open(&lock_path) {
        Ok(file) => file,
        Err(_) => {
            refs.insert("unreadable-lock".to_string());
            return refs;
        }
    };
    let taken = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if taken {
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        return refs;
    }
    // Held: one live install. Name it by the pid it recorded, when it recorded one.
    let holder = std::fs::read_to_string(&lock_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|pid| format!("install-holder(pid={pid})"))
        .unwrap_or_else(|| "install-holder(pid=unknown)".to_string());
    refs.insert(holder);
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A scratch directory under the project's own temp base; `/tmp` is not
    /// writable for an app uid on Android.
    fn scratch(tag: &str) -> PathBuf {
        let dir =
            crate::paths::temp_base().join(format!("pdn-refcount-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plant_staging(containers: &Path, name: &str) -> PathBuf {
        let tree = containers.join(format!(".tmp-install-{name}-{}", std::process::id()));
        fs::create_dir_all(tree.join("rootfs")).unwrap();
        fs::write(tree.join("rootfs/half-written"), b"partial").unwrap();
        tree
    }

    /// Half one: a unit with a live reference is refused, and its bytes stay.
    #[test]
    fn a_referenced_unit_is_refused_and_survives() {
        let dir = scratch("refused");
        let path = dir.join("unit");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("sentinel"), b"live").unwrap();

        let mut refs = Refs::new();
        refs.insert("install-holder(pid=4242)".to_string());
        let unit = Unit {
            key: "unit".to_string(),
            path: path.clone(),
            refs,
        };

        match evict(&unit) {
            Eviction::Refused { key, message } => {
                assert_eq!(key, "unit");
                assert_eq!(
                    message,
                    "refusing to evict 'unit': 1 reference(s) still hold it \
                     (install-holder(pid=4242))",
                    "the refusal must name the count and the holder"
                );
            }
            other => panic!("a referenced unit must be refused, got {other:?}"),
        }
        assert!(path.join("sentinel").exists(), "a refused unit was deleted");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Half two: a unit whose references reached zero is deleted.
    #[test]
    fn a_unit_at_zero_references_is_deleted() {
        let dir = scratch("evicted");
        let path = dir.join("unit");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("sentinel"), b"garbage").unwrap();

        let unit = Unit {
            key: "unit".to_string(),
            path: path.clone(),
            refs: Refs::new(),
        };

        assert_eq!(
            evict(&unit),
            Eviction::Evicted {
                key: "unit".to_string()
            }
        );
        assert!(!path.exists(), "a zero-reference unit must be deleted");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A symlink planted under the store is unlinked, never descended.
    #[test]
    fn a_symlinked_unit_is_unlinked_not_followed() {
        let dir = scratch("symlink");
        let target = dir.join("outside");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep"), b"host data").unwrap();
        let link = dir.join("unit");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let unit = Unit {
            key: "unit".to_string(),
            path: link.clone(),
            refs: Refs::new(),
        };
        assert!(matches!(evict(&unit), Eviction::Evicted { .. }));
        assert!(!link.exists(), "the link must be gone");
        assert!(
            target.join("keep").exists(),
            "an eviction followed a symlink out of the store"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The scanner derives one reference from a held sidecar lock.
    #[test]
    fn the_scanner_counts_a_live_install_as_one_reference() {
        let dir = scratch("scanner-live");
        let containers = dir.join("containers");
        fs::create_dir_all(&containers).unwrap();
        let tree = plant_staging(&containers, "live");
        let lock_path = sidecar_lock_path(&tree);
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .unwrap();
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        fs::write(&lock_path, std::process::id().to_string()).unwrap();

        let units = staging_units(&containers);
        let unit = units
            .iter()
            .find(|u| u.key.starts_with(".tmp-install-live-"))
            .expect("the staging tree must be a unit");
        assert_eq!(unit.refs.len(), 1, "a held lock is one reference: {unit:?}");
        assert_eq!(
            unit.refs.iter().next().unwrap(),
            &format!("install-holder(pid={})", std::process::id())
        );

        let report = sweep(&units);
        assert!(
            report.iter().any(|e| matches!(
                e,
                Eviction::Refused { key, .. } if key.starts_with(".tmp-install-live-")
            )),
            "a live staging tree must be refused: {report:?}"
        );
        assert!(
            tree.join("rootfs/half-written").exists(),
            "live tree deleted"
        );
        assert!(lock_path.exists(), "the live tree's sidecar was deleted");

        drop(lock);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The scanner finds no reference for a tree nobody owns, and the sweep
    /// deletes the tree and its sidecar.
    #[test]
    fn the_scanner_finds_no_reference_for_an_unowned_tree() {
        let dir = scratch("scanner-dead");
        let containers = dir.join("containers");
        fs::create_dir_all(&containers).unwrap();
        let tree = plant_staging(&containers, "dead");
        let lock_path = sidecar_lock_path(&tree);
        fs::write(&lock_path, b"").unwrap();

        let units = staging_units(&containers);
        let unit = units
            .iter()
            .find(|u| u.key.starts_with(".tmp-install-dead-"))
            .expect("the staging tree must be a unit");
        assert!(unit.refs.is_empty(), "an unowned tree has no references");

        let report = sweep(&units);
        assert!(
            report
                .iter()
                .all(|e| !matches!(e, Eviction::Refused { .. })),
            "nothing holds an unowned tree: {report:?}"
        );
        assert!(!tree.exists(), "an unowned staging tree must be deleted");
        assert!(!lock_path.exists(), "its sidecar must go with it");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A superseded container a `--force` reinstall moved aside is unreferenced.
    #[test]
    fn a_replaced_container_directory_is_unreferenced() {
        let dir = scratch("replaced");
        let containers = dir.join("containers");
        fs::create_dir_all(&containers).unwrap();
        let tree = containers.join(format!("{REPLACED_PREFIX}old-{}", std::process::id()));
        fs::create_dir_all(tree.join("rootfs")).unwrap();

        let units = staging_units(&containers);
        let unit = units
            .iter()
            .find(|u| u.key.starts_with(REPLACED_PREFIX))
            .expect("a replaced directory must be a unit");
        assert!(unit.refs.is_empty());

        sweep(&units);
        assert!(!tree.exists(), "a superseded container must be reclaimed");
        let _ = fs::remove_dir_all(&dir);
    }
}
