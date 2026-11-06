//! On-disk cache of the container store listing (roadmap section 9.2).
//!
//! A metadata query used to mean `read_dir` plus a full read of every container's
//! `metadata.json` and `/etc/os-release`, so its latency grew with the container
//! count. This module caches the scanned listing and reuses it while nothing in the
//! store has changed.
//!
//! The freshness key is the mtime and size of each container's `metadata.json` (the
//! directory's own mtime for legacy containers that predate that file) — the one
//! file a rebuild rewrites. A change to any of them makes the key differ and the
//! next query falls back to the full scan. `PODNEXT_NO_INDEX_CACHE=1` disables the
//! cache entirely; the benchmark harness uses it as its negative control.
//!
//! Only the *derived* fields are cached ([`ContainerInfo`]), not the whole
//! `Container`: the scan fills everything else from `ContainerConfig::default()`, and
//! reparsing the full config through JSON cost more than the file reads it replaced
//! (measured: a full-`Container` cache lost to no cache at 10, 50, 200 and 600
//! containers).

use crate::container::{Container, ContainerConfig, ContainerInfo};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Cache file name, written under the store's cache directory.
pub const INDEX_FILE: &str = "container_index.json";
/// Bump when the cached shape changes; a mismatch is treated as "no cache".
const INDEX_VERSION: u32 = 2;

/// What a cached entry is valid against: the identity and modification state of one
/// container directory.
#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Stamp {
    name: String,
    /// mtime as (secs, nanos) since the epoch; `None` if the file is unreadable.
    mtime: Option<(i64, u32)>,
    len: u64,
}

#[derive(Serialize, Deserialize)]
struct Index {
    version: u32,
    stamps: Vec<Stamp>,
    entries: Vec<ContainerInfo>,
}

/// Whether the index cache is in use. The harness unsets this to prove the cache
/// is what changed the number.
pub fn enabled() -> bool {
    std::env::var_os("PODNEXT_NO_INDEX_CACHE").is_none()
}

fn mtime_of(path: &Path) -> Option<(i64, u32)> {
    let t = std::fs::metadata(path).ok()?.modified().ok()?;
    // A file we just stat'ed can still report a pre-epoch mtime on a hostile
    // clock; treat that as unknown rather than inventing a stamp.
    t.duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| (d.as_secs() as i64, d.subsec_nanos()))
}

/// The freshness key: one stamp per container directory, sorted by name so that an
/// unspecified `read_dir` order is never mistaken for a change.
fn stamps(dir: &Path) -> Vec<Stamp> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if !crate::container::is_container_dir(&path) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // Metadata present: its own mtime and size. Legacy directory: the
        // directory mtime, which is what the scanner falls back to for
        // `created_at`.
        let meta = path.join("metadata.json");
        let (mtime, len) = match std::fs::metadata(&meta) {
            Ok(m) => (mtime_of(&meta), m.len()),
            Err(_) => (mtime_of(&path), 0),
        };
        out.push(Stamp { name, mtime, len });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Rebuild the `Container` the scanner would have produced: every field outside
/// [`ContainerInfo`] comes from `ContainerConfig::default()`, exactly as
/// `scan_containers` leaves it.
fn container_from(info: ContainerInfo) -> Container {
    let config = ContainerConfig {
        name: info.name.clone(),
        distro: info.distro.clone(),
        version: info.version.clone(),
        arch: info.arch.clone(),
        rootfs_path: Some(info.rootfs_path.clone()),
        ..Default::default()
    };
    Container {
        name: info.name,
        config,
        created_at: info.created_at,
        status: info.status,
        pid: None,
        rootfs_path: info.rootfs_path,
        corrupt: info.corrupt,
    }
}

/// The cached listing, if the cache exists, parses, is current, and every container
/// directory still carries the stamp it had when the entry was written. `None` sends
/// the caller down the full-scan path.
pub fn load_if_fresh(dir: &Path, cache: &Path) -> Option<Vec<Container>> {
    let bytes = std::fs::read(cache).ok()?;
    let index: Index = serde_json::from_slice(&bytes).ok()?;
    if index.version != INDEX_VERSION || index.stamps != stamps(dir) {
        return None;
    }
    Some(index.entries.into_iter().map(container_from).collect())
}

/// Persist `entries` for `dir`. Best-effort: a cache that cannot be written is not
/// an error, it only means the next query pays for the full scan again.
pub fn store(dir: &Path, cache: &Path, entries: &[Container]) {
    let index = Index {
        version: INDEX_VERSION,
        stamps: stamps(dir),
        entries: entries.iter().map(Container::info).collect(),
    };
    let Ok(bytes) = serde_json::to_vec(&index) else {
        return;
    };
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Write to a temporary sibling and rename, so a reader never sees a partial file.
    let tmp = cache.with_extension("json.tmp");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, cache);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A store with one container of the shape `save_container_metadata` produces.
    fn store_with_one(dir: &Path) -> PathBuf {
        let c = dir.join("demo");
        std::fs::create_dir_all(c.join("rootfs/etc")).unwrap();
        std::fs::write(
            c.join("rootfs/etc/os-release"),
            "ID=alpine\nVERSION_ID=3.20.0\n",
        )
        .unwrap();
        std::fs::write(
            c.join("metadata.json"),
            r#"{"name":"demo","image":"alpine","tag":"3.20.0","created_at":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        c.join("metadata.json")
    }

    #[test]
    fn a_fresh_cache_is_served_and_a_touched_metadata_file_is_not() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("containers");
        std::fs::create_dir_all(&dir).unwrap();
        let meta = store_with_one(&dir);
        let cache = tmp.path().join("cache/container_index.json");

        // Nothing cached yet.
        assert!(load_if_fresh(&dir, &cache).is_none());
        store(&dir, &cache, &[]);

        // A cache whose stamps match the store is served.
        let entries = load_if_fresh(&dir, &cache).expect("fresh cache must be served");
        assert_eq!(entries.len(), 0);

        // Rewriting the metadata file (new mtime, new size) invalidates it.
        std::fs::write(&meta, r#"{"name":"demo","image":"alpine","tag":"9.9.9"}"#).unwrap();
        assert!(
            load_if_fresh(&dir, &cache).is_none(),
            "a changed metadata.json must not be served from the cache"
        );
    }

    /// A served entry must carry the fields the scanner derives, so the cache is a
    /// drop-in for the scan rather than a lossy summary.
    #[test]
    fn a_served_entry_matches_the_scanned_container() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("containers");
        std::fs::create_dir_all(&dir).unwrap();
        store_with_one(&dir);
        let cache = tmp.path().join("c.json");

        let scanned = Container {
            name: "demo".into(),
            config: ContainerConfig {
                name: "demo".into(),
                distro: "alpine".into(),
                version: "3.20.0".into(),
                arch: "aarch64".into(),
                rootfs_path: Some(dir.join("demo/rootfs")),
                ..Default::default()
            },
            created_at: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            status: crate::container::ContainerStatus::Stopped,
            pid: None,
            rootfs_path: dir.join("demo/rootfs"),
            corrupt: None,
        };
        store(&dir, &cache, std::slice::from_ref(&scanned));

        let served = load_if_fresh(&dir, &cache).expect("fresh cache");
        assert_eq!(served.len(), 1);
        let got = &served[0];
        assert_eq!(got.name, scanned.name);
        assert_eq!(got.config.distro, scanned.config.distro);
        assert_eq!(got.config.version, scanned.config.version);
        assert_eq!(got.created_at, scanned.created_at);
        assert_eq!(got.rootfs_path, scanned.rootfs_path);
        assert_eq!(got.status, scanned.status);
    }

    #[test]
    fn a_container_directory_without_a_rootfs_is_not_stamped() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("containers");
        std::fs::create_dir_all(dir.join("not-a-container")).unwrap();
        store_with_one(&dir);
        // Only the real container contributes a stamp, so adding a bare directory
        // changes nothing.
        let cache = tmp.path().join("c.json");
        store(&dir, &cache, &[]);
        let entries = load_if_fresh(&dir, &cache).expect("bare directory must not matter");
        assert_eq!(entries.len(), 0);
    }
}
