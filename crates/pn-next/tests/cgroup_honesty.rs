//! WORKLIST A20: `setup_cgroup` must not report a resource limit it never bound.
//!
//! Before the fix, `start_service` composed `<runtime_dir>/cgroups/<name>`,
//! `create_dir_all`ed it, and wrote `cgroup.procs`, `cpu.max` and `memory.max`
//! into that plain directory. Those are ordinary files: the writes succeeded,
//! `setup_cgroup` returned `Ok(())`, and no kernel limit existed. This test
//! observes the truth instead — with no writable cgroup v2 mount on this host the
//! call refuses by name and leaves no directory behind claiming otherwise.

use pn_next::{
    writable_cgroupfs, ProcessConfig, ResourceLimits, RestartPolicy, ServiceConfig, ServiceMeta,
    ServiceSupervisor, ServiceType, StdioConfig, SupervisorBuilder, SupervisorHealthCheckConfig,
};
use std::collections::HashMap;
use std::path::Path;
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

/// The truth on this host: no writable cgroup v2 mount, so a limit cannot be
/// bound and `start_service` must refuse it by name without leaving residue.
#[tokio::test]
async fn a_cgroup_limit_that_cannot_be_bound_is_refused_by_name() {
    // The truth is host-dependent: if a writable cgroup v2 mount exists, the
    // refusal branch is not observable here. Say so once, loudly, and return
    // rather than assert a false negative.
    if writable_cgroupfs().is_some() {
        println!(
            "FIXTURE-UNAVAILABLE: this host has a writable cgroup v2 mount, so the \
             unavailability path cannot be exercised here"
        );
        return;
    }

    let temp = tempdir().expect("tempdir");
    let runtime_dir = temp.path().join("runtime");

    let cg = supervisor(temp.path(), true);
    cg.create_service(service_config("nonsense-cgroup"))
        .await
        .expect("register the service");

    let err = cg
        .start_service("nonsense-cgroup")
        .await
        .expect_err("start_service must refuse when no cgroup can be bound");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("UNAVAILABLE"),
        "the refusal must name the limit as UNAVAILABLE; got: {msg}"
    );
    for op in ["cpu.max", "memory.max", "pids.max"] {
        assert!(
            msg.contains(op),
            "the refusal must name the operation it cannot apply ({op}); got: {msg}"
        );
    }

    // ABSENCE, guarded by the positive observation above (the refusal was
    // observed): no plain directory is left behind claiming a limit was applied.
    let leftover = runtime_dir.join("cgroups").join("nonsense-cgroup");
    assert!(
        !leftover.exists(),
        "a refused start must leave no directory at {} claiming a limit was applied",
        leftover.display()
    );
}

/// Positive control for the test above: the same service starts fine when cgroups
/// are not requested, so the refusal is about cgroups and not about a service
/// that could never start.
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
