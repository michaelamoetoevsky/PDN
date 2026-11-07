//! Install a rootfs from a local archive: a plain rootfs tarball, or an OCI
//! image-layout tar (roadmap section 10.1).
//!
//! Two shapes of input share one destination - `unpack into a clean rootfs`:
//!
//! * a **plain rootfs tarball** (`.tar`, `.tar.gz`, `.tar.xz`, `.tar.bz2`,
//!   `.tar.zst`, including a wrapping directory), extracted with the same
//!   Android-safe walker the registry path uses ([`crate::untar`]);
//! * an **OCI image layout** stored inside a tar (`oci-layout` + `index.json` +
//!   `blobs/sha256/*`), whose blobs are read by digest and **verified against
//!   the digest the manifest declares** before they are applied.
//!
//! The OCI layout needs random access (a layer blob is read by the digest a
//! manifest names, in any order), so it is read from a plain, seekable tar. A
//! compressed archive is decoded to a temporary copy first; an uncompressed one
//! is read in place. This is the same boundary the reference draws
//! (`install_local.py:install_from_local_file`, which reopens a seekable handle
//! once it sees the `oci-layout` marker), and the extraction itself is
//! reimplemented against the OCI/Docker specifications, not transcribed.

use anyhow::{bail, Context, Result};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use oci_spec::image::{ImageIndex, ImageManifest};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use xz2::read::XzDecoder;

/// Directories that name a rootfs root, scored to pick a strip depth. The
/// reference's `install_local.py:_ROOTFS_DIRS`.
const ROOTFS_DIRS: &[&str] = &[
    "bin", "dev", "etc", "home", "lib", "lib32", "lib64", "libx32", "media", "mnt", "opt", "proc",
    "root", "run", "sbin", "srv", "sys", "tmp", "usr", "var",
];

/// The outer compression of an archive, by magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outer {
    None,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

/// Fill `rootfs` from the archive at `archive`.
///
/// The rootfs is created if it does not exist. Every failure names what it was
/// doing; nothing is left half-applied silently - the caller discards the tree
/// on error (the install path stages into a directory renamed into place only on
/// success).
pub fn install_from_archive(archive: &Path, rootfs: &Path) -> Result<()> {
    let reader =
        File::open(archive).with_context(|| format!("opening archive {}", archive.display()))?;
    match sniff(&reader)? {
        Outer::None => install_from_tar(archive, rootfs),
        kind => {
            // ponytail: a compressed archive is decoded whole to a temp file so
            // the OCI layout can be seeked. Upgrade to a streaming two-pass only
            // if multi-GB compressed archives become the norm.
            let mut tmp = tempfile::Builder::new()
                .prefix("pdn-archive-")
                .tempfile()
                .context("creating a temporary file for the decoded archive")?;
            let source = BufReader::new(reader);
            match kind {
                Outer::Gzip => io::copy(&mut GzDecoder::new(source), &mut tmp)?,
                Outer::Bzip2 => io::copy(&mut BzDecoder::new(source), &mut tmp)?,
                Outer::Xz => io::copy(&mut XzDecoder::new(source), &mut tmp)?,
                Outer::Zstd => io::copy(&mut zstd::stream::read::Decoder::new(source)?, &mut tmp)?,
                Outer::None => unreachable!(),
            };
            tmp.flush()?;
            install_from_tar(tmp.path(), rootfs)
        }
    }
}

/// The sha256 of a whole file, lowercase hex. Used to check an archive against a
/// caller-supplied `--checksum` before it is unpacked.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = BufReader::new(
        File::open(path).with_context(|| format!("opening {} to hash it", path.display()))?,
    );
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Read the leading bytes and classify the outer compression.
fn sniff(mut file: &File) -> Result<Outer> {
    let mut head = [0u8; 6];
    let n = file.read(&mut head)?;
    file.seek(SeekFrom::Start(0))?;
    let head = &head[..n];
    Ok(if head.starts_with(&[0x1f, 0x8b]) {
        Outer::Gzip
    } else if head.starts_with(&[0x42, 0x5a, 0x68]) {
        Outer::Bzip2
    } else if head.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
        Outer::Xz
    } else if head.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Outer::Zstd
    } else {
        Outer::None
    })
}

/// Install from an uncompressed, seekable tar: OCI layout or plain rootfs.
fn install_from_tar(tar: &Path, rootfs: &Path) -> Result<()> {
    if has_member(tar, "oci-layout")? {
        extract_oci_layout(tar, rootfs)
    } else {
        let strip = detect_strip_count(tar)?;
        let file = File::open(tar).with_context(|| format!("opening {}", tar.display()))?;
        crate::untar::extract_with_strip(BufReader::new(file), rootfs, strip)
            .with_context(|| format!("extracting rootfs archive {}", tar.display()))
    }
}

/// True when the tar carries a member with exactly this name.
fn has_member(tar: &Path, wanted: &str) -> Result<bool> {
    let file = File::open(tar).with_context(|| format!("opening {}", tar.display()))?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries().context("reading archive members")? {
        let entry = entry.context("reading an archive header")?;
        if entry.path()?.to_string_lossy() == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}

/// How many leading components to strip so a known rootfs directory lands at the
/// root. The reference's `install_local.py:detect_strip_count`: score depths 0-4
/// by how many of the first 500 member names have a known rootfs directory at
/// that depth, and take the highest scorer (ties go to the lower depth).
fn detect_strip_count(tar: &Path) -> Result<usize> {
    let file = File::open(tar).with_context(|| format!("opening {}", tar.display()))?;
    let mut archive = tar::Archive::new(file);
    let mut names: Vec<String> = Vec::with_capacity(500);
    for entry in archive.entries()? {
        let entry = entry?;
        names.push(entry.path()?.to_string_lossy().into_owned());
        if names.len() >= 500 {
            break;
        }
    }

    let mut best_strip = 0usize;
    let mut best_score = -1i64;
    for strip in 0..5usize {
        let mut score = 0i64;
        for name in &names {
            let parts: Vec<&str> = name
                .trim_start_matches('/')
                .trim_end_matches('/')
                .split('/')
                .collect();
            if parts.len() > strip && ROOTFS_DIRS.contains(&parts[strip]) {
                score += 1;
            }
        }
        if score > best_score {
            best_score = score;
            best_strip = strip;
        }
    }
    Ok(best_strip)
}

/// Extract an OCI image layout: pick the archive's manifest for this device's
/// architecture, then apply each of its layers, verifying every blob against the
/// digest the OCI metadata declares.
fn extract_oci_layout(tar: &Path, rootfs: &Path) -> Result<()> {
    let file = File::open(tar).with_context(|| format!("opening {}", tar.display()))?;

    // One pass to index member positions, capturing the small `index.json`.
    let mut offsets: HashMap<String, (u64, u64)> = HashMap::new();
    let mut index_json: Option<Vec<u8>> = None;
    {
        let mut archive = tar::Archive::new(&file);
        for entry in archive.entries().context("indexing OCI archive members")? {
            let mut entry = entry?;
            let name = entry.path()?.to_string_lossy().into_owned();
            let size = entry.size();
            offsets.insert(name.clone(), (entry.raw_file_position(), size));
            if name == "index.json" {
                let mut buf = Vec::with_capacity(size as usize);
                entry.read_to_end(&mut buf)?;
                index_json = Some(buf);
            }
        }
    }
    let index_json =
        index_json.ok_or_else(|| anyhow::anyhow!("OCI archive has no `index.json` member"))?;
    let index: ImageIndex =
        serde_json::from_slice(&index_json).context("parsing the OCI archive's index.json")?;

    let manifest_digest = select_manifest(&index)?.to_string();
    let manifest_path = blob_path(&manifest_digest)?;
    let manifest_bytes = read_member(&file, &offsets, &manifest_path, Some(&manifest_digest))?;
    let manifest: ImageManifest =
        serde_json::from_slice(&manifest_bytes).context("parsing the OCI archive's manifest")?;

    if manifest.layers().is_empty() {
        bail!("OCI archive manifest {manifest_digest} declares no layers to apply");
    }

    for layer in manifest.layers() {
        let digest = layer.digest().to_string();
        let media_type = layer.media_type().to_string();
        let path = blob_path(&digest)?;
        let (offset, size) = *offsets.get(&path).ok_or_else(|| {
            anyhow::anyhow!(
                "OCI archive is missing blob {path} required by manifest {manifest_digest}"
            )
        })?;
        verify_blob(&file, offset, size, &digest)?;

        let reader = open_blob(&file, offset, size, &media_type)?;
        crate::untar::extract(reader, rootfs)
            .with_context(|| format!("applying OCI layer {digest}"))?;
    }
    Ok(())
}

/// The digest of the manifest this device should install.
///
/// A single-manifest index is used as-is; a multi-platform index is searched for
/// this device's architecture, and the absence of one is named rather than
/// guessed at.
fn select_manifest(index: &ImageIndex) -> Result<&oci_spec::image::Digest> {
    let manifests = index.manifests();
    let want_arch = oci_arch();
    if let Some(found) = manifests.iter().find(|d| {
        d.platform()
            .as_ref()
            .map(|p| p.architecture().to_string() == want_arch && p.os().to_string() == "linux")
            .unwrap_or(false)
    }) {
        return Ok(found.digest());
    }
    if manifests.len() == 1 {
        return Ok(manifests[0].digest());
    }
    let found: Vec<String> = manifests
        .iter()
        .map(|d| match d.platform() {
            Some(p) => format!("{}/{}", p.os(), p.architecture()),
            None => "unspecified".to_string(),
        })
        .collect();
    bail!(
        "OCI archive has no manifest for linux/{want_arch} (it offers: {})",
        found.join(", ")
    )
}

/// This device's architecture in the OCI/Docker spelling.
fn oci_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        "x86" | "i686" => "386",
        "arm" => "arm",
        other => other,
    }
}

/// `blobs/<algo>/<hex>` for a content-addressed digest, with the digest's shape
/// checked first so a crafted `index.json` cannot route the lookup through a
/// name containing `..` or `/` (`install_local.py:_oci_blob_path`).
fn blob_path(digest: &str) -> Result<String> {
    let ok = digest.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    });
    if !ok {
        bail!("refusing OCI digest {digest:?}: expected `sha256:<64 lowercase hex>`");
    }
    let (algo, hex) = digest.split_once(':').unwrap();
    Ok(format!("blobs/{algo}/{hex}"))
}

/// Read the whole of one indexed member. With `expected`, its sha256 is checked
/// against that digest and a mismatch is refused by name.
fn read_member(
    file: &File,
    offsets: &HashMap<String, (u64, u64)>,
    path: &str,
    expected: Option<&str>,
) -> Result<Vec<u8>> {
    let (offset, size) = *offsets
        .get(path)
        .ok_or_else(|| anyhow::anyhow!("OCI archive is missing {path}"))?;
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(size as usize);
    reader.take(size).read_to_end(&mut buf)?;
    if let Some(expected) = expected {
        let got = hex::encode(Sha256::digest(&buf));
        let want = expected.strip_prefix("sha256:").unwrap_or(expected);
        if got != want {
            bail!("OCI blob {expected} does not match the archive's bytes (read sha256:{got})");
        }
    }
    Ok(buf)
}

/// Verify one blob against its declared digest before it is applied.
fn verify_blob(file: &File, offset: u64, size: u64, digest: &str) -> Result<()> {
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(offset))?;
    let mut limited = reader.take(size);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    loop {
        let n = limited.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got = hex::encode(hasher.finalize());
    let want = digest.strip_prefix("sha256:").unwrap_or(digest);
    if got != want {
        bail!(
            "OCI blob {digest} does not match the digest the manifest declares \
             (read sha256:{got}); the archive is corrupt or tampered with"
        );
    }
    Ok(())
}

/// Open one layer blob, decoding it by the media type the manifest declares.
fn open_blob(file: &File, offset: u64, size: u64, media_type: &str) -> Result<Box<dyn Read>> {
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(offset))?;
    let limited = reader.take(size);
    let reader: Box<dyn Read> = if media_type.contains("gzip") {
        Box::new(GzDecoder::new(limited))
    } else if media_type.contains("zstd") {
        Box::new(zstd::stream::read::Decoder::new(limited).context("decoding a zstd layer")?)
    } else if media_type.contains("bzip") {
        Box::new(BzDecoder::new(limited))
    } else {
        Box::new(limited)
    };
    Ok(reader)
}
