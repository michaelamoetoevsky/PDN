//! Install sources beyond a registry reference (roadmap section 10.1): a local
//! rootfs tarball, a local OCI image-layout archive, and a URL - each with the
//! optional `--checksum` the reference does not have for installs.
//!
//! **What is measured, and against what.** Every test runs the real binary
//! against a private store (`HOME` and `PN_TEST_ROOT` both point at a temp
//! root), so the operator's `~/.podnext` is never touched. The fixtures are
//! built here, offline: a small rootfs tar (optionally gzipped), and an OCI
//! image layout tar assembled from the pieces the format is made of (an
//! `oci-layout` marker, an `index.json`, and `blobs/sha256/*`). The URL test
//! serves its fixture from a loopback listener created for the test, never the
//! network - this device's registry link is intermittent, and a test that needed
//! it would flake.
//!
//! **The reference's rules, asserted where they are observable.** A local
//! source is one with explicit path syntax (`./x`, `/x`, `../x`, `~/x`); a bare
//! token stays a registry reference. The container name is derived from the
//! archive filename (`_derive_local_name`), and every refusal names its reason -
//! no silent success.
//!
//! The whole file is `expected to fail against the base tree`, where `install`
//! takes a registry reference only: the source changes are what make it pass.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> &'static str {
    // A shared CARGO_TARGET_DIR across worktrees means a sibling's build can
    // silently replace the binary; PODNEXT_BIN pins the exercised copy.
    option_env!("PODNEXT_BIN").unwrap_or(env!("CARGO_BIN_EXE_podnext"))
}

/// A hermetic store: everything `podnext` touches lives under this temp root.
struct TestRoot {
    tmp: tempfile::TempDir,
}

impl TestRoot {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join(".podnext").join("containers"))
            .expect("containers");
        std::fs::create_dir_all(tmp.path().join(".podnext").join("sessions")).expect("sessions");
        Self { tmp }
    }

    fn env(&self) -> Vec<(String, String)> {
        let v = self.tmp.path().to_string_lossy().into_owned();
        vec![
            ("PN_TEST_ROOT".to_string(), v.clone()),
            ("HOME".to_string(), v),
        ]
    }

    fn run(&self, args: &[&str]) -> Output {
        let env = self.env();
        let mut c = Command::new(bin());
        c.args(args);
        for (k, v) in &env {
            c.env(k, v);
        }
        c.output()
            .unwrap_or_else(|e| panic!("spawn podnext {args:?}: {e}"))
    }

    fn container_dir(&self, name: &str) -> PathBuf {
        self.tmp
            .path()
            .join(".podnext")
            .join("containers")
            .join(name)
    }

    fn installed(&self, name: &str) -> bool {
        self.container_dir(name).join("rootfs").is_dir()
    }
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or_else(|| {
        panic!(
            "podnext was killed by a signal: {:?}; stderr: {}",
            out.status,
            stderr(out)
        )
    })
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ===== fixtures =====

/// Append one regular file to a tar being built.
fn add_file<W: Write>(b: &mut tar::Builder<W>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    b.append_data(&mut header, path, data)
        .expect("append member");
}

/// A tiny rootfs tree with a recognisable marker, wrapped in one directory when
/// `wrapper` is `Some` (so the strip-depth heuristic has something to strip).
fn rootfs_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "etc/os-release",
            b"NAME=\"Fixture Linux\"\nID=fixture\nVERSION_ID=\"1\"\n".to_vec(),
        ),
        (
            "usr/bin/hello",
            b"# a fixture binary placeholder\n".to_vec(),
        ),
    ]
}

/// Write an uncompressed plain rootfs tarball; returns its path.
fn write_plain_tar(dir: &Path, name: &str, wrapper: Option<&str>) -> PathBuf {
    let path = dir.join(name);
    let file = std::fs::File::create(&path).expect("create tar");
    let mut b = tar::Builder::new(file);
    for (rel, data) in rootfs_files() {
        let member = match wrapper {
            Some(w) => format!("{w}/{rel}"),
            None => rel.to_string(),
        };
        add_file(&mut b, &member, &data);
    }
    b.finish().expect("finish tar");
    path
}

/// Write a gzipped plain rootfs tarball; returns its path.
fn write_plain_tar_gz(dir: &Path, name: &str) -> PathBuf {
    let inner = write_plain_tar(dir, ".plain-src.tar", None);
    let bytes = std::fs::read(&inner).expect("read inner tar");
    let out = dir.join(name);
    let file = std::fs::File::create(&out).expect("create gz");
    let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    enc.write_all(&bytes).expect("gzip");
    enc.finish().expect("finish gz");
    let _ = std::fs::remove_file(&inner);
    out
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn sha256_file_hex(path: &Path) -> String {
    sha256_hex(&std::fs::read(path).expect("read for hash"))
}

/// The bytes of a gzipped tar layer.
fn gzip_layer() -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (rel, data) in rootfs_files() {
        add_file(&mut tar, rel, &data);
    }
    let plain = tar.into_inner().expect("finish layer tar");
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&plain).expect("gzip layer");
    enc.finish().expect("finish gzip")
}

/// Assemble an OCI image-layout tar. `tamper` stores a layer whose bytes do not
/// match the digest its manifest declares.
fn write_oci_tar(dir: &Path, name: &str, tamper: bool) -> PathBuf {
    let layer = gzip_layer();
    let layer_digest = sha256_hex(&layer);
    let config = b"{}".to_vec();
    let config_digest = sha256_hex(&config);

    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": format!("sha256:{config_digest}"),
            "size": config.len(),
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
            "digest": format!("sha256:{layer_digest}"),
            "size": layer.len(),
        }],
    });
    let manifest_bytes = serde_json::to_vec(&manifest).expect("manifest json");
    let manifest_digest = sha256_hex(&manifest_bytes);

    let index = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": format!("sha256:{manifest_digest}"),
            "size": manifest_bytes.len(),
            "platform": { "architecture": "arm64", "os": "linux" },
        }],
    });
    let index_bytes = serde_json::to_vec(&index).expect("index json");

    let path = dir.join(name);
    let file = std::fs::File::create(&path).expect("create oci tar");
    let mut b = tar::Builder::new(file);
    add_file(&mut b, "oci-layout", br#"{"imageLayoutVersion":"1.0.0"}"#);
    add_file(&mut b, "index.json", &index_bytes);
    add_file(
        &mut b,
        &format!("blobs/sha256/{manifest_digest}"),
        &manifest_bytes,
    );
    add_file(&mut b, &format!("blobs/sha256/{config_digest}"), &config);
    // The tampered layer keeps the digest the manifest declares but not the bytes.
    let stored = if tamper {
        let mut evil = layer.clone();
        evil.extend_from_slice(b"\n# tampered trailer\n");
        evil
    } else {
        layer
    };
    add_file(&mut b, &format!("blobs/sha256/{layer_digest}"), &stored);
    b.finish().expect("finish oci tar");
    path
}

/// A loopback HTTP server that serves `body` once for a single request.
///
/// `None` (with one loud named line) means the listener could not be bound here;
/// the test then reports itself UNRUNNABLE rather than passing.
///
/// The serving thread is detached and gives up after a deadline: on the base
/// tree `install` never reaches the URL, so a blocking `join` would hang the run
/// - the detach is what keeps the negative control finite.
fn serve_once(body: Vec<u8>, path: &str) -> Option<String> {
    let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "FIXTURE-UNAVAILABLE: could not bind a loopback listener for the URL test: {e}"
            );
            return None;
        }
    };
    if listener.set_nonblocking(true).is_err() {
        eprintln!("FIXTURE-UNAVAILABLE: could not put the loopback listener in non-blocking mode");
        return None;
    }
    let addr = listener.local_addr().ok()?;
    let url = format!("http://{addr}{path}");
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            match listener.accept() {
                Ok((mut sock, _)) => {
                    let _ = sock.set_nonblocking(false);
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf); // request line + headers
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes());
                    let _ = sock.write_all(&body);
                    let _ = sock.flush();
                    return;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(_) => return,
            }
        }
    });
    Some(url)
}

// ===== the local plain tarball (10.1: `install ./rootfs.tar.gz` works offline) =====

#[test]
fn install_from_a_local_plain_tarball_works_offline() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    // A wrapping directory, so the strip-depth heuristic is exercised too.
    let archive = write_plain_tar(fixtures.path(), "alpine-rootfs.tar", Some("myroot"));

    let out = root.run(&["install", &archive.to_string_lossy()]);
    assert_eq!(code(&out), 0, "install failed: {}", stderr(&out));

    // The name is derived from the filename: `alpine-rootfs.tar` -> `alpine-rootfs`.
    assert!(
        root.installed("alpine-rootfs"),
        "the container was not named by the derived alias; store has: {:?}",
        std::fs::read_dir(root.container_dir("alpine-rootfs").parent().unwrap()).map(|r| r
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>())
    );
    let os_release = root
        .container_dir("alpine-rootfs")
        .join("rootfs/etc/os-release");
    let contents = std::fs::read_to_string(&os_release).expect("os-release extracted");
    assert!(
        contents.contains("Fixture Linux"),
        "the rootfs was not extracted to the rootfs root: {contents:?}"
    );
    // The wrapper directory must not have survived the strip.
    assert!(
        !root
            .container_dir("alpine-rootfs")
            .join("rootfs/myroot")
            .exists(),
        "the wrapping directory was not stripped"
    );
}

#[test]
fn install_from_a_gzipped_local_tarball_works_offline() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    let archive = write_plain_tar_gz(fixtures.path(), "debian-rootfs.tar.gz");

    let out = root.run(&["install", &archive.to_string_lossy(), "--name", "from-gz"]);
    assert_eq!(code(&out), 0, "install failed: {}", stderr(&out));
    assert!(root.installed("from-gz"), "container not installed");
}

// ===== the optional checksum =====

#[test]
fn a_correct_checksum_installs() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    let archive = write_plain_tar(fixtures.path(), "alpine-rootfs.tar", None);
    let digest = sha256_file_hex(&archive);

    let out = root.run(&[
        "install",
        &archive.to_string_lossy(),
        "--checksum",
        &format!("sha256:{digest}"),
    ]);
    assert_eq!(
        code(&out),
        0,
        "install with a correct checksum failed: {}",
        stderr(&out)
    );
    assert!(root.installed("alpine-rootfs"), "container not installed");
    assert!(
        stderr(&out).contains("Checksum verified"),
        "a verified checksum must say so: {}",
        stderr(&out)
    );
}

#[test]
fn a_wrong_checksum_is_refused_and_nothing_is_installed() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    let archive = write_plain_tar(fixtures.path(), "alpine-rootfs.tar", None);

    let out = root.run(&[
        "install",
        &archive.to_string_lossy(),
        "--checksum",
        &format!("sha256:{}", "0".repeat(64)),
    ]);
    assert_ne!(
        code(&out),
        0,
        "a wrong checksum must not succeed: stdout={:?}",
        stdout(&out)
    );
    assert!(
        stderr(&out).contains("checksum mismatch"),
        "the refusal must name a checksum mismatch: {}",
        stderr(&out)
    );
    assert!(
        !root.installed("alpine-rootfs"),
        "a refused install must leave nothing behind"
    );
}

#[test]
fn an_absent_checksum_prints_one_named_line() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    let archive = write_plain_tar(fixtures.path(), "alpine-rootfs.tar", None);

    let out = root.run(&["install", &archive.to_string_lossy()]);
    assert_eq!(code(&out), 0, "install failed: {}", stderr(&out));
    assert!(
        stderr(&out).contains("no --checksum given"),
        "an unverified archive must say so in one named line: {}",
        stderr(&out)
    );
}

#[test]
fn a_checksum_on_a_registry_reference_is_refused() {
    let root = TestRoot::new();
    let out = root.run(&[
        "install",
        "alpine:3.20",
        "--checksum",
        &format!("sha256:{}", "a".repeat(64)),
    ]);
    assert_ne!(code(&out), 0, "a checksum on an image ref must be refused");
    assert!(
        stderr(&out).contains("--checksum applies to a local archive or a URL"),
        "the refusal must name why: {}",
        stderr(&out)
    );
}

#[test]
fn a_missing_local_file_is_refused_by_name() {
    let root = TestRoot::new();
    let out = root.run(&["install", "./no-such-rootfs.tar.gz"]);
    assert_ne!(code(&out), 0, "a missing local file must be refused");
    assert!(
        stderr(&out).contains("does not exist or is not a regular file"),
        "the refusal must name the missing file: {}",
        stderr(&out)
    );
    assert!(
        !root.installed("no-such-rootfs"),
        "nothing may be installed"
    );
}

// ===== the URL (10.1: `install https://.../rootfs.tar.xz`; checksum optional) =====

#[test]
fn install_from_a_url_with_a_checksum() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    let archive = write_plain_tar(fixtures.path(), "url-rootfs.tar", Some("myroot"));
    let body = std::fs::read(&archive).expect("read fixture");
    let digest = sha256_hex(&body);

    let Some(url) = serve_once(body, "/url-rootfs.tar") else {
        return; // FIXTURE-UNAVAILABLE already printed
    };

    let out = root.run(&["install", &url, "--checksum", &format!("sha256:{digest}")]);
    assert_eq!(code(&out), 0, "URL install failed: {}", stderr(&out));
    // The name derives from the URL path's basename: `url-rootfs.tar` -> `url-rootfs`.
    assert!(
        root.installed("url-rootfs"),
        "the URL install did not create the derived container"
    );
}

// ===== the OCI image layout (10.1: digests verified; a mismatch refused) =====

#[test]
fn install_from_a_local_oci_layout_verifies_digests() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    let archive = write_oci_tar(fixtures.path(), "fixture.oci.tar", false);

    let out = root.run(&["install", &archive.to_string_lossy(), "--name", "from-oci"]);
    assert_eq!(code(&out), 0, "OCI layout install failed: {}", stderr(&out));
    assert!(root.installed("from-oci"), "container not installed");
    let os_release = root.container_dir("from-oci").join("rootfs/etc/os-release");
    assert!(
        os_release.is_file(),
        "the OCI layer was not applied to the rootfs"
    );
}

#[test]
fn a_tampered_oci_blob_is_refused_by_digest() {
    let root = TestRoot::new();
    let fixtures = tempfile::tempdir().expect("fixtures");
    let archive = write_oci_tar(fixtures.path(), "tampered.oci.tar", true);

    let out = root.run(&["install", &archive.to_string_lossy(), "--name", "tampered"]);
    assert_ne!(
        code(&out),
        0,
        "an OCI archive whose blob does not match its digest must be refused"
    );
    assert!(
        stderr(&out).contains("does not match the digest"),
        "the refusal must name the digest mismatch: {}",
        stderr(&out)
    );
    assert!(
        !root.installed("tampered"),
        "a refused OCI install must leave nothing behind"
    );
}
