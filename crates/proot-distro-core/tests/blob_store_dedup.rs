//! Content-addressed layer store: a shared base layer is stored ONCE, and a
//! container can be materialised from the store rather than from a freshly
//! unpacked archive.
//!
//! The acceptance is a number, not a claim. Two local OCI archives are built
//! here that share one base layer (`img1 = [base, appA]`, `img2 = [base, appB]`);
//! after both are installed the store holds **three** blobs, not four, and its
//! byte total is `base + appA + appB` - not the `2*base + appA + appB` a store
//! without content addressing would hold. A `du`-style line is printed for the
//! throwaway store root, and the count/byte assertions fail if the shared blob
//! is ever duplicated.
//!
//! **Negative control.** The pre-existing suites (`store_integrity`,
//! `atomic_identity`) are the base-tree controls and must stay green; this file
//! adds only the new store behaviour. Nothing here touches the operator's real
//! `~/.podnext`: the store root is redirected through `PN_TEST_ROOT` (the same
//! resolver `paths::resolve_home` honours), and every store in this file lives
//! under a `tempdir`.

use proot_distro_core::archive::{install_from_archive, materialize_from_store};
use proot_distro_core::blobstore::{reuse_message, BlobStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Serialises the tests that mutate the process-global `PN_TEST_ROOT`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ===== fixtures =====

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// A gzipped tar layer from (path, bytes) entries.
fn gzip_layer(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (path, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        tar.append_data(&mut header, path, *data).expect("append");
    }
    let plain = tar.into_inner().expect("finish layer tar");
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&plain).expect("gzip layer");
    enc.finish().expect("finish gzip")
}

/// An OCI image-layout archive whose layers are the given gzipped blobs.
struct Oci {
    path: PathBuf,
    /// `(digest, mediaType)` in application order.
    layers: Vec<(String, String)>,
}

/// Assemble `<dir>/<name>` as an OCI layout: `oci-layout` + `index.json` + the
/// manifest, config and layer blobs the index names.
fn write_oci(dir: &Path, name: &str, layers: &[Vec<u8>]) -> Oci {
    const MEDIA: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
    let manifest_layers: Vec<serde_json::Value> = layers
        .iter()
        .map(|l| {
            json!({
                "mediaType": MEDIA,
                "digest": format!("sha256:{}", sha256_hex(l)),
                "size": l.len(),
            })
        })
        .collect();

    let config = b"{}".to_vec();
    let manifest = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": format!("sha256:{}", sha256_hex(&config)),
            "size": config.len(),
        },
        "layers": manifest_layers,
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let manifest_digest = sha256_hex(&manifest_bytes);

    // A single-manifest index: `select_manifest` uses it as-is on any arch.
    let index = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": format!("sha256:{manifest_digest}"),
            "size": manifest_bytes.len(),
        }],
    });
    let index_bytes = serde_json::to_vec(&index).unwrap();

    let path = dir.join(name);
    let file = fs::File::create(&path).expect("create oci tar");
    let mut b = tar::Builder::new(file);
    let mut add = |path: &str, data: &[u8]| {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        b.append_data(&mut header, path, data)
            .expect("append member");
    };
    add("oci-layout", br#"{"imageLayoutVersion":"1.0.0"}"#);
    add("index.json", &index_bytes);
    add(&format!("blobs/sha256/{manifest_digest}"), &manifest_bytes);
    add(&format!("blobs/sha256/{}", sha256_hex(&config)), &config);
    for l in layers {
        add(&format!("blobs/sha256/{}", sha256_hex(l)), l);
    }
    b.finish().expect("finish oci tar");

    Oci {
        path,
        layers: layers
            .iter()
            .map(|l| (format!("sha256:{}", sha256_hex(l)), MEDIA.to_string()))
            .collect(),
    }
}

// ===== store measurement =====

/// Total bytes of every regular file under `root` (deterministic: file lengths,
/// not filesystem block rounding).
fn dir_bytes(root: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(t) if t.is_file() => total += entry.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    total
}

/// Regular files directly under `<root>/sha256` (the committed blobs), and any
/// leftover `.ingest-*.tmp` (which must be zero).
fn blob_files(root: &Path) -> (usize, usize) {
    let shard = root.join("sha256");
    let mut committed = 0;
    let mut temps = 0;
    if let Ok(entries) = fs::read_dir(&shard) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                if entry.file_name().to_string_lossy().starts_with(".ingest-") {
                    temps += 1;
                } else {
                    committed += 1;
                }
            }
        }
    }
    (committed, temps)
}

/// The `du -sk` line for a store root, or `None` when `du` is unavailable.
fn du_line(root: &Path) -> Option<String> {
    let out = Command::new("du").arg("-sk").arg(root).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ===== the acceptance =====

/// Two images sharing a base layer store that base once: three blobs, and the
/// byte total proves the shared base was not written twice.
#[test]
fn a_shared_base_layer_is_stored_once() {
    let tmp = tempfile::tempdir().unwrap();
    let fixtures = tempfile::tempdir().unwrap();

    let base = gzip_layer(&[
        ("etc/os-release", b"ID=fixture\n"),
        ("bin/busybox", b"# shared base\n"),
    ]);
    let app_a = gzip_layer(&[("app/a.txt", b"layer A\n")]);
    let app_b = gzip_layer(&[("app/b.txt", b"layer B\n")]);
    let (base_len, a_len, b_len) = (base.len() as u64, app_a.len() as u64, app_b.len() as u64);

    let img1 = write_oci(
        fixtures.path(),
        "img1.oci.tar",
        &[base.clone(), app_a.clone()],
    );
    let img2 = write_oci(
        fixtures.path(),
        "img2.oci.tar",
        &[base.clone(), app_b.clone()],
    );

    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("PN_TEST_ROOT", tmp.path());

    let store = tmp.path().join(".podnext/blobs");

    // Nothing stored before the first install.
    assert_eq!(dir_bytes(&store), 0, "store must start empty");

    install_from_archive(&img1.path, &tmp.path().join("rootfs1")).expect("install image 1");
    let bytes1 = dir_bytes(&store);
    let (blobs1, temps1) = blob_files(&store);
    assert_eq!(blobs1, 2, "image 1 declares two layers");
    assert_eq!(temps1, 0, "a two-phase ingest leaves no temp behind");
    assert_eq!(
        bytes1,
        base_len + a_len,
        "image 1 must store its two layer blobs"
    );
    eprintln!(
        "DU: after image1 store={}B blobs={} du={:?}",
        bytes1,
        blobs1,
        du_line(&store)
    );

    install_from_archive(&img2.path, &tmp.path().join("rootfs2")).expect("install image 2");
    let bytes2 = dir_bytes(&store);
    let (blobs2, temps2) = blob_files(&store);
    eprintln!(
        "DU: after image2 store={}B blobs={} du={:?}",
        bytes2,
        blobs2,
        du_line(&store)
    );

    std::env::remove_var("PN_TEST_ROOT");

    // THE ASSERTION THAT FAILS IF A SHARED BLOB IS DUPLICATED: a store keyed by
    // content holds three blobs; one that keyed blobs by image would hold four.
    assert_eq!(
        blobs2, 3,
        "the shared base layer was duplicated: {blobs2} blobs"
    );
    assert_eq!(temps2, 0, "no ingest temp may survive");

    // ...and the byte total is the three distinct blobs, not four.
    let deduped = base_len + a_len + b_len;
    let naive_without_dedup = (base_len + a_len) + (base_len + b_len);
    assert_eq!(
        bytes2, deduped,
        "store holds {bytes2}B but the distinct blobs total {deduped}B"
    );
    assert_eq!(
        bytes2 - bytes1,
        b_len,
        "installing image 2 must add only its own layer"
    );
    assert_eq!(
        naive_without_dedup - bytes2,
        base_len,
        "content addressing must save exactly one base layer"
    );
    eprintln!(
        "DU: naive(no dedup)={}B deduped={}B saved={}B",
        naive_without_dedup, bytes2, base_len
    );
}

/// A container is materialised from the STORE: the source archive is deleted
/// first, so the rebuilt rootfs can only have come from retained blobs.
#[test]
fn a_container_materialises_from_the_store_without_the_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let fixtures = tempfile::tempdir().unwrap();

    let base = gzip_layer(&[
        ("etc/os-release", b"ID=stored\n"),
        ("bin/busybox", b"# base\n"),
    ]);
    let app_a = gzip_layer(&[("app/a.txt", b"layer A\n")]);
    let img1 = write_oci(fixtures.path(), "img1.oci.tar", &[base, app_a]);

    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("PN_TEST_ROOT", tmp.path());

    let rootfs1 = tmp.path().join("rootfs1");
    install_from_archive(&img1.path, &rootfs1).expect("install image 1");
    assert!(
        rootfs1.join("etc/os-release").is_file(),
        "the first install did not apply the base layer"
    );

    // The store is now the ONLY source: remove the archive.
    fs::remove_file(&img1.path).expect("remove the source archive");
    assert!(!img1.path.exists());

    let store = tmp.path().join(".podnext/blobs");
    let (before, _) = blob_files(&store);

    let rootfs2 = tmp.path().join("rootfs2");
    materialize_from_store(&rootfs2, &img1.layers).expect("materialise from the store");
    std::env::remove_var("PN_TEST_ROOT");

    assert_eq!(
        fs::read_to_string(rootfs2.join("etc/os-release")).unwrap(),
        "ID=stored\n",
        "the base layer was not applied from the store"
    );
    assert!(
        rootfs2.join("app/a.txt").is_file(),
        "the app layer was not applied from the store"
    );
    let (after, _) = blob_files(&store);
    assert_eq!(before, after, "materialisation must not add blobs");
}

// ===== two-phase ingest =====

/// A blob that does not match its declared digest is never committed, and a
/// re-ingest of an identical blob is a reuse - the exact reuse line is asserted.
#[test]
fn a_mismatched_blob_is_never_committed_and_a_reuse_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let store = BlobStore::at(tmp.path().join("blobs"));

    // Wrong digest for the bytes: refused, and NOTHING lands in the store.
    let wrong = format!("sha256:{}", "0".repeat(64));
    let err = store
        .put_bytes(&wrong, b"garbage")
        .expect_err("a digest mismatch must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("does not match the digest"),
        "the refusal must name the mismatch: {msg}"
    );
    assert_eq!(
        dir_bytes(store.root()),
        0,
        "a refused ingest left bytes in the store"
    );
    assert_eq!(
        blob_files(store.root()),
        (0, 0),
        "a refused ingest left a blob or a temp"
    );

    // The right digest: stored once, with no temp surviving.
    let good = format!("sha256:{}", sha256_hex(b"hello"));
    let first = store.put_bytes(&good, b"hello").expect("store");
    assert!(!first.reused, "the first ingest is not a reuse");
    assert_eq!(first.bytes, 5);
    assert_eq!(blob_files(store.root()), (1, 0));

    // The same bytes again: reused, and the exact reuse line is emitted.
    let second = store.put_bytes(&good, b"hello").expect("reuse");
    assert!(second.reused, "an already-stored blob must be reused");
    assert_eq!(
        second.message,
        reuse_message(&good),
        "the reuse line must be the exact string"
    );
    assert!(
        second.message.contains("reused the stored copy"),
        "reuse line must say the stored copy was reused: {}",
        second.message
    );
    assert_eq!(
        blob_files(store.root()),
        (1, 0),
        "reuse must not add a blob"
    );
    assert_eq!(dir_bytes(store.root()), 5, "reuse must not add bytes");
}
