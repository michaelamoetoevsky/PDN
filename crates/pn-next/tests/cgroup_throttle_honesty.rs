//! Base-tree negative control for the supervisor's cgroup honesty (WORKLIST A20, adoption
//! S8 of `research/DISSECTION_SHROOT.md`).
//!
//! This file deliberately uses only the public surface that exists on the base tree
//! (0c8801d), so the same test can be run against it. On the base tree the quota manager's
//! throttle wrote `memory.max` into `<cgroups_mount>/pdn/<service>` as an ordinary file and
//! returned `Ok()` - a caller saw a completed write and a limit that did not exist - so the
//! absence assertion below fails there. With the private cgroupfs view the write is refused
//! by name and no file claims the limit.

use pn_next::{QuotaViolationAction, ResourceQuotaConfig, ResourceQuotaManager, ResourceUsage};
use tempfile::tempdir;

#[tokio::test]
async fn a_throttle_never_leaves_an_ordinary_file_claiming_a_limit() {
    let mount = tempdir().expect("tempdir");
    // The directory the base tree's throttle wrote into. It exists on purpose: the write
    // must be refused on its merits, not because the path happened to be missing.
    std::fs::create_dir_all(mount.path().join("pdn").join("quota-svc"))
        .expect("seed the cgroup path");

    let mut config = ResourceQuotaConfig::default();
    config.cgroups_mount = mount.path().to_string_lossy().into_owned();
    config.violation_action = QuotaViolationAction::Throttle;
    let manager = ResourceQuotaManager::new(config);

    let usage = ResourceUsage {
        service: "quota-svc".to_string(),
        timestamp: chrono::Utc::now(),
        cpu_usage: 0,          // no CPU violation: only the memory path runs
        memory_usage: 1 << 40, // far over the 512 MiB default
        memory_swap_usage: 0,
        io_read_bps: 0,
        io_write_bps: 0,
        io_read_iops: 0,
        io_write_iops: 0,
        pids_count: 0,
        open_fds: 0,
        network_rx_bps: 0,
        network_tx_bps: 0,
    };
    manager
        .update_usage("quota-svc", usage)
        .await
        .expect("a refused throttle must not fail the usage update");

    let fake = mount
        .path()
        .join("pdn")
        .join("quota-svc")
        .join("memory.max");
    assert!(
        !fake.exists(),
        "no ordinary file may claim a memory limit was applied; found {}",
        fake.display()
    );
}
