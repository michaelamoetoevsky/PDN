//! A content-addressed store for OCI layer blobs.
//!
//! Until this module existed, an install verified each layer and then threw the
//! bytes away: the container was a freshly unpacked tree, and two images sharing
//! a base layer wrote that base twice. The reference keeps every blob under the
//! digest that names it, so a second image that references the same layer adds
//! nothing (moby `core/content/content.go:41-46`, the `blobs/<algo>/<hex>` tree).
//!
//! Two mechanisms are ported here:
//!
//! * **content addressing** - the blob's sha256 *is* its path, so identical bytes
//!   from any source coalesce onto one file and a re-ingest is a no-op;
//! * **two-phase ingest** - [`BlobStore::put_reader`] writes a temp, hashes it,
//!   and only `rename`s it onto the digest path once the bytes match that digest.
//!   A reader therefore never observes a partial blob at a content address: a
//!   failed or interrupted ingest leaves only a `.ingest-*.tmp` that is not
//!   reachable by digest (moby `core/metadata/content.go:604-623,670-694`).
//!
//! The store root is resolved once through [`crate::paths::resolve_home`], so a
//! redirected root (`PN_TEST_ROOT`) is followed and a test never writes into the
//! operator's real `~/.podnext`.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Result of ingesting one blob, including the one line said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutOutcome {
    /// The digest the blob is stored under (`sha256:<hex>`).
    pub digest: String,
    /// True when the bytes were already present and nothing was written.
    pub reused: bool,
    /// Size of the blob's bytes.
    pub bytes: u64,
    /// The exact line printed to stderr for this outcome.
    pub message: String,
}

/// The line printed when a blob was already present and was not duplicated.
///
/// Kept as a function so the wording is asserted in a test rather than only
/// observed, and so the CLI and the test agree on the exact string.
pub fn reuse_message(digest: &str) -> String {
    format!("blob {digest} already in the store; reused the stored copy, no duplicate written")
}

/// The line printed when a blob was written for the first time.
pub fn store_message(digest: &str, bytes: u64) -> String {
    format!("stored blob {digest} ({bytes} bytes) in the content-addressed store")
}

impl PutOutcome {
    fn stored(digest: &str, bytes: u64) -> Self {
        Self {
            digest: digest.to_string(),
            reused: false,
            bytes,
            message: store_message(digest, bytes),
        }
    }

    fn reused(digest: &str, bytes: u64) -> Self {
        Self {
            digest: digest.to_string(),
            reused: true,
            bytes,
            message: reuse_message(digest),
        }
    }
}

/// Monotonic suffix so two ingests in one process never compose the same temp.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A digest-addressed blob directory.
pub struct BlobStore {
    root: PathBuf,
}

/// Check a digest is `sha256:<64 lowercase hex>` and return the hex half.
///
/// The digest is used as a path component, so this is a trust boundary: a
/// crafted `index.json` must not be able to route a write through `..` or `/`.
pub(crate) fn parse_digest(digest: &str) -> Result<&str> {
    let hex = digest.strip_prefix("sha256:").ok_or_else(|| {
        anyhow::anyhow!("refusing OCI digest {digest:?}: expected `sha256:<64 lowercase hex>`")
    })?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        bail!("refusing OCI digest {digest:?}: expected `sha256:<64 lowercase hex>`");
    }
    Ok(hex)
}

impl BlobStore {
    /// The store under the resolved home: `<home>/.podnext/blobs`.
    pub fn new() -> Result<Self> {
        Ok(Self {
            root: crate::paths::resolve_home()?.join(".podnext").join("blobs"),
        })
    }

    /// A store rooted at an explicit directory (tests, and any caller that
    /// already resolved the root).
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/sha256/<hex>` for a validated digest.
    pub fn blob_path(&self, digest: &str) -> Result<PathBuf> {
        Ok(self.root.join("sha256").join(parse_digest(digest)?))
    }

    pub fn contains(&self, digest: &str) -> Result<bool> {
        Ok(self.blob_path(digest)?.is_file())
    }

    /// Open a stored blob for reading (materialising a container from the store).
    pub fn open(&self, digest: &str) -> Result<File> {
        let path = self.blob_path(digest)?;
        File::open(&path).with_context(|| format!("opening stored blob {}", path.display()))
    }

    /// Ingest `reader` under `declared`, two-phase: write, verify, rename.
    ///
    /// Returns early with `reused = true` when the digest is already present -
    /// that early return is what makes two images sharing a layer store its
    /// bytes once.
    pub fn put_reader<R: Read>(&self, declared: &str, mut reader: R) -> Result<PutOutcome> {
        let hex = parse_digest(declared)?;
        let shard = self.root.join("sha256");
        let dest = shard.join(hex);

        if dest.is_file() {
            let bytes = fs::metadata(&dest)?.len();
            let outcome = PutOutcome::reused(declared, bytes);
            eprintln!("{}", outcome.message);
            return Ok(outcome);
        }
        if dest.exists() {
            bail!(
                "refusing to store blob {declared}: {} exists and is not a regular file",
                dest.display()
            );
        }

        fs::create_dir_all(&shard)
            .with_context(|| format!("creating the blob shard {}", shard.display()))?;
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = shard.join(format!(".ingest-{}-{}-{hex}.tmp", std::process::id(), n));

        // Phase 1: write the bytes and hash the same bytes as they land on disk.
        let mut hasher = Sha256::new();
        let mut written = 0u64;
        {
            let mut out = File::create(&tmp)
                .with_context(|| format!("creating the ingest temp {}", tmp.display()))?;
            let mut buf = [0u8; 1 << 16];
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n])?;
                written += n as u64;
            }
            out.flush()?;
            out.sync_all().ok();
        }

        // Phase 2: verify BEFORE the digest path exists. A mismatch removes the
        // temp and_dest is never touched, so nothing partial is reachable.
        let got = hex::encode(hasher.finalize());
        if got != hex {
            let _ = fs::remove_file(&tmp);
            bail!(
                "blob {declared} does not match the digest the ingest computed \
                 (read sha256:{got}); the archive is corrupt or tampered with"
            );
        }

        // Phase 3: commit only while the temp is still the file we wrote.
        let id = crate::storage::file_id(&tmp)?;
        if let Err(e) = crate::storage::rename_if_unchanged(&tmp, id, &dest) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(PutOutcome::stored(declared, written))
    }

    /// [`BlobStore::put_reader`] over an in-memory blob.
    pub fn put_bytes(&self, declared: &str, bytes: &[u8]) -> Result<PutOutcome> {
        self.put_reader(declared, bytes)
    }
}
