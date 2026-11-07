//! WORKLIST A27: `Runtime::apply_resource_limits` must not write a cgroup limit file that
//! enforces nothing.
//!
//! Before the fix this path `create_dir_all`ed `rootfs/sys/fs/cgroup/container_<id>` and wrote
//! `cpu.max`, `memory.max`, `memory.swap.max`, `pids.max` and `cpu.weight` into it with
//! `std::fs::write`. Those are ordinary files in a directory that is not a cgroup mount: the
//! writes succeeded and a caller was left to assume a limit was in force when nothing enforced
//! it. That is the last site of the class WORKLIST A20 fixed in the supervisor and S8 fixed in
//! the throttle/quotas path. The fix routes every knob through the private cgroupfs view
//! (`pn_next::CgroupView`), which refuses a resource policy it cannot enforce by name.
//!
//! The test is the same shape as `no_file_claims_a_limit_the_supervisor_could_not_bind`
//! (`tests/cgroup_view.rs`) and the A20 control (`tests/cgroup_honesty.rs`): it starts the
//! runtime path with limits set, walks the runtime tree, and fails if ANY cgroup control file
//! exists as an ordinary file. It pins the other half too - the caller is TOLD, by a named
//! warning, which limit was not applied - so a silent drop cannot pass.

use pn_next::runtime::{CreateContainerRequest, Runtime, RuntimeConfig};
use proot_distro_core::{ContainerConfig, ResourceLimits as ProotResourceLimits};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

/// A `MakeWriter` that keeps every log line so the assertion can read it back.
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = CaptureWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CaptureWriter(self.0.clone())
    }
}

struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log buffer").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Every file under `dir`, recursively - the walk the absence assertion needs.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn apply_resource_limits_writes_no_file_that_claims_a_limit() {
    let rootfs = tempdir().expect("tempdir");
    let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(Capture(logs.clone()))
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .finish();

    // A multi-thread runtime: `Runtime::new` reaches `ContainerManager::new`, which uses
    // `block_in_place`. The future is driven with `block_on` on this thread so the thread-local
    // subscriber stays live for every warning the call emits (nothing here is spawned).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let container_id = tracing::subscriber::with_default(subscriber, || {
        rt.block_on(async {
            let runtime = Runtime::new(RuntimeConfig::default()).expect("pn-next runtime");

            // The rootfs is a directory this test owns; on the base tree the limit files land
            // under it, which is exactly what this test looks for.
            let config = ContainerConfig {
                name: "a27-limits".to_string(),
                distro: "alpine".to_string(),
                version: "latest".to_string(),
                arch: "aarch64".to_string(),
                rootfs_path: Some(rootfs.path().to_path_buf()),
                resources: ProotResourceLimits {
                    cpu_limit: Some(0.5),
                    memory_limit: Some(64 * 1024 * 1024),
                    disk_limit: None,
                    pids_limit: Some(16),
                },
                ..ContainerConfig::default()
            };
            let id = runtime
                .create_container(CreateContainerRequest {
                    config,
                    sandbox_config: None,
                    runtime_handler: None,
                })
                .await
                .expect("create the container");

            // A limit that cannot be bound is skipped, not refused: refusing would trade a false
            // claim for a container that will not start.
            runtime
                .apply_resource_limits(&id)
                .await
                .expect("a container must start even when a cgroup limit cannot be bound");
            id
        })
    });

    // ABSENCE, the assertion that fails on the base tree: base created
    // `rootfs/sys/fs/cgroup/container_<id>/` and wrote cpu.max/memory.max/pids.max into it as
    // ordinary files, which enforces nothing.
    let mut files = Vec::new();
    collect(rootfs.path(), &mut files);
    let fakes: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            matches!(
                p.file_name().and_then(|n| n.to_str()),
                Some("cpu.max")
                    | Some("memory.max")
                    | Some("memory.swap.max")
                    | Some("pids.max")
                    | Some("cgroup.procs")
                    | Some("cpuset.cpus")
            )
        })
        .collect();
    assert!(
        fakes.is_empty(),
        "no ordinary file may claim a limit is applied; found {fakes:?}"
    );

    // The caller is TOLD: a named warning identifies the limit and the reason, so the skip is
    // never something a caller has to infer from an absent file.
    let captured = String::from_utf8(logs.lock().expect("log buffer").clone()).expect("utf8 logs");
    assert!(
        captured.contains("cpu.max"),
        "the caller must be told which limit was not applied; captured: {captured:?}"
    );
    assert!(
        captured.contains(&container_id),
        "the warning must name the container; captured: {captured:?}"
    );

    match pn_next::writable_cgroupfs() {
        None => println!(
            "A27-PROVED: no writable cgroup2 mount on this host - the refusal branch ran and no \
             file claims a limit it does not have"
        ),
        Some(mount) => println!(
            "A27-NOTE: a writable cgroup2 mount exists at {} - a resource policy is still refused \
             (EPERM) by the view, but the EROFS branch was not the one exercised",
            mount.display()
        ),
    }
}
