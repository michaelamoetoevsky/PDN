//! WORKLIST A20: `setup_cgroup` must not report a resource limit it never bound.
//!
//! Before the fix, `start_service` composed `<runtime_dir>/cgroups/<name>`,
//! `create_dir_all`ed it, and wrote `cgroup.procs`, `cpu.max` and `memory.max`
//! into that plain directory. Those are ordinary files: the writes succeeded,
//! `setup_cgroup` returned `Ok(())`, and no kernel limit existed.
//!
//! The fix has two halves, and this test pins both. A limit this host cannot
//! bind is (a) never faked - no plain-file cgroup is written - and (b) never a
//! reason to refuse the service, because most non-root hosts and the CI runner
//! have no writable cgroup v2 mount; refusing would trade a false claim for a
//! broken feature. The first version of this fix refused, and CI caught it:
//! `daemon_integration_test` failed 4/4 with
//! `cgroup limits UNAVAILABLE for service 'restart-test'`.

use pn_next::{
    writable_cgroupfs, ProcessConfig, ResourceLimits, RestartPolicy, ServiceConfig, ServiceMeta,
    ServiceSupervisor, ServiceType, StdioConfig, SupervisorBuilder, SupervisorHealthCheckConfig,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn service_config(name: &str) -> ServiceConfig {
    ServiceConfig {
        meta: ServiceMeta {
            name: name.to_string(),
            description: format!("A20 fixture {name}"),
            version: "1.0.0".to_string(),
            service_type: ServiceType::Daemon,
            tags: vec![],
        },
        process: ProcessConfig {
            // `/bin/true` exits at once; the test only needs `start_service` to
            // reach the cgroup step and never relies on the service staying up.
            command: vec!["/bin/true".to_string()],
            args: vec![],
            pre_start: None,
            post_stop: None,
            pre_reload: None,
            cwd: None,
            umask: 0o022,
            nice: None,
            cpu_affinity: None,
        },
        resources: ResourceLimits::default(),
        restart: RestartPolicy::default(),
        health: SupervisorHealthCheckConfig::default(),
        dependencies: vec![],
        env: HashMap::new(),
        working_dir: None,
        user: None,
        stdio: StdioConfig::default(),
        sockets: vec![],
        readiness_timeout_ms: None,
    }
}

fn supervisor(root: &Path, cgroups: bool) -> ServiceSupervisor {
    SupervisorBuilder::new()
        .services_dir(root.join("services"))
        .runtime_dir(root.join("runtime"))
        .log_dir(root.join("logs"))
        .enable_cgroups(cgroups)
        .build()
        .expect("supervisor")
}

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

/// The base bug, observed: with a limit requested, starting the service must
/// succeed *and* leave no plain-file cgroup behind pretending to bind it.
#[tokio::test]
async fn a_limit_that_cannot_be_bound_is_skipped_and_never_faked() {
    let temp = tempdir().expect("tempdir");
    let runtime_dir = temp.path().join("runtime");

    let cg = supervisor(temp.path(), true);
    cg.create_service(service_config("nonsense-cgroup"))
        .await
        .expect("register the service");

    cg.start_service("nonsense-cgroup").await.expect(
        "a service must start even when a cgroup limit cannot be bound on this host - \
         skipping the limit is honest, refusing the service is not",
    );

    // ABSENCE, and it is the assertion that fails on the base tree: base created
    // this directory and wrote cpu.max/memory.max/pids.max into it as ordinary
    // files, which enforces nothing.
    let fake = runtime_dir.join("cgroups").join("nonsense-cgroup");
    assert!(
        !fake.exists(),
        "no plain-file cgroup may be created at {} - a limit that was not bound must not be \
         represented on disk as if it were",
        fake.display()
    );

    // Stronger form: nothing anywhere under the runtime dir may carry a cgroup
    // control-file name.
    let mut files = Vec::new();
    collect(&runtime_dir, &mut files);
    let fake_files: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            matches!(
                p.file_name().and_then(|n| n.to_str()),
                Some("cpu.max") | Some("memory.max") | Some("pids.max") | Some("cgroup.procs")
            )
        })
        .collect();
    assert!(
        fake_files.is_empty(),
        "no plain-file cgroup control file may exist; found {fake_files:?}"
    );

    match writable_cgroupfs() {
        None => println!(
            "A20-PROVED: this host has no writable cgroup v2 mount; the service started and no \
             plain-file cgroup was written, so nothing claims a limit it does not have"
        ),
        Some(mount) => println!(
            "A20-NOTE: a writable cgroup v2 mount exists at {} - the skip path was not exercised \
             on this host",
            mount.display()
        ),
    }
}

/// Positive control for the test above: the same service starts fine when cgroups
/// are not requested at all, so a start is never about the cgroup step alone.
#[tokio::test]
async fn the_same_service_starts_when_cgroups_are_not_requested() {
    let temp = tempdir().expect("tempdir");
    let sup = supervisor(temp.path(), false);
    sup.create_service(service_config("control-cgroup"))
        .await
        .expect("register the service");
    sup.start_service("control-cgroup")
        .await
        .expect("a service with cgroups disabled must still start");
}
