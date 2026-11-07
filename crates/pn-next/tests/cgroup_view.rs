//! Adoption S8 (`research/DISSECTION_SHROOT.md`): a private cgroupfs *view*.
//!
//! The reference is `native/shroot/src/cgroup.c` at commit `7ab99d6`, which serves a private
//! file enum and refuses with an explicit errno (`-EBUSY`, `-EPERM`, `-EACCES`, `-EROFS`)
//! what it cannot honour rather than pretending. The two directions are pinned here:
//!
//! (a) a knob PDN can honour from userspace - membership, the sampled CPU/RSS figures,
//!     answered from `/proc` - is served, and the assertions tie the value to the platform
//!     (a live pid is listed and a pid `/proc` does not show is not; the sampled RSS equals
//!     the platform's own `VmRSS` for a process other than this one; the sampled CPU counter
//!     moves when the process burns CPU) so a constant could not satisfy them;
//! (b) a knob PDN cannot honour - a memory, cpuset or device limit - is refused with a named
//!     errno, and no file anywhere claims the limit is in force. That second half is the
//!     assertion that would have caught the original dishonesty (WORKLIST A20).

use pn_next::{
    CgroupErrno, CgroupFile, CgroupView, ProcessConfig, ResourceLimits, RestartPolicy,
    ServiceConfig, ServiceMeta, ServiceSupervisor, ServiceType, StdioConfig, SupervisorBuilder,
    SupervisorHealthCheckConfig,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

/// Kills a spawned helper on every path, including a panic, so the suite leaks no process.
struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn vmrss_kb(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|kb| kb.parse().ok())
    })
}

fn ticks(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

fn sampled_ticks(view: &CgroupView) -> u64 {
    view.read(CgroupFile::CpuStat)
        .expect("cpu.stat is served")
        .lines()
        .find_map(|l| l.strip_prefix("ticks_sampled "))
        .and_then(|n| n.trim().parse().ok())
        .expect("cpu.stat carries a sampled tick count")
}

/// (a) membership listing is answered from the platform: a live pid is listed, a pid `/proc`
/// does not show is not, so the answer cannot come from the view's own list unexamined.
#[test]
fn membership_listing_is_filtered_by_the_platform() {
    let me = std::process::id();
    let ghost = 999_999_999;
    let mut view = CgroupView::with_group(None);
    view.add_pid(me);
    view.add_pid(ghost);

    let procs = view
        .read(CgroupFile::Procs)
        .expect("cgroup.procs is served from the platform");
    let listed: Vec<u32> = procs
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();

    assert!(
        listed.contains(&me),
        "the live process {me} must be listed; got {procs:?}"
    );
    assert!(
        !listed.contains(&ghost),
        "a pid /proc does not show must not be listed; got {procs:?}"
    );
    assert!(
        Path::new(&format!("/proc/{me}")).exists()
            && !Path::new(&format!("/proc/{ghost}")).exists(),
        "the platform must be the reason the filter kept one pid and dropped the other"
    );
}

/// (a) the sampled RSS is the platform's own figure for the named process, not a constant:
/// a helper process's `memory.current` equals that helper's `/proc` `VmRSS`, and this
/// process's own sampled RSS is different, which no constant can be.
#[test]
fn sampled_memory_comes_from_the_platform() {
    let child = ChildGuard(
        Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn a helper process whose RSS is not this process's"),
    );
    let helper = child.0.id();

    let mut view = CgroupView::with_group(None);
    view.add_pid(helper);
    let reported: u64 = view
        .read(CgroupFile::MemoryCurrent)
        .expect("memory.current is served")
        .trim()
        .parse()
        .expect("memory.current is a byte count");

    let platform = vmrss_kb(helper).expect("the helper's own /proc VmRSS") * 1024;
    assert!(reported > 0, "the helper holds memory; got {reported}");
    assert!(
        reported.abs_diff(platform) <= 8 * 1024 * 1024,
        "memory.current ({reported}) must be the platform's VmRSS for pid {helper} ({platform})"
    );

    let mut mine = CgroupView::with_group(None);
    mine.add_pid(std::process::id());
    let mine_rss: u64 = mine
        .read(CgroupFile::MemoryCurrent)
        .expect("memory.current is served")
        .trim()
        .parse()
        .expect("memory.current is a byte count");
    assert!(
        mine_rss > reported,
        "two live processes must give two different sampled figures ({mine_rss} and {reported}); \
         a constant cannot"
    );
}

/// (a) the sampled CPU figure is read live: it is the platform's `utime+stime` for this
/// process, and it grows when this process burns CPU.
#[test]
fn sampled_cpu_comes_from_the_platform_and_moves() {
    let me = std::process::id();
    let mut view = CgroupView::with_group(None);
    view.add_pid(me);

    let before = sampled_ticks(&view);
    let platform = ticks(me).expect("this process's own /proc stat");
    assert!(
        before.abs_diff(platform) <= 5,
        "cpu.stat ({before}) must track the platform's utime+stime ({platform})"
    );

    let start = std::time::Instant::now();
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    while start.elapsed().as_millis() < 80 {
        for _ in 0..10_000 {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        }
        std::hint::black_box(x);
    }

    let after = sampled_ticks(&view);
    assert!(
        after > before,
        "the sampled CPU counter must grow with this process's own CPU use \
         ({before} then {after}); a constant could not"
    );
}

/// (a) the honoured write direction: where the group has a backing directory, membership is
/// served and lands in `cgroup.procs`.
#[test]
fn membership_is_served_where_the_group_is_backed() {
    let dir = tempdir().expect("tempdir");
    let view = CgroupView::with_group(Some(dir.path().to_path_buf()));
    view.write(CgroupFile::Procs, "4242\n")
        .expect("a backed group serves membership");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("cgroup.procs")).expect("cgroup.procs"),
        "4242\n"
    );
}

/// (b) the named refusal, with the exact errno and message, for the three knob classes this
/// device cannot honour (a memory, cpuset or device limit on a cgroupfs it cannot write).
#[test]
fn a_limit_that_cannot_be_honoured_is_refused_by_named_errno() {
    let view = CgroupView::with_group(None);
    for file in [
        CgroupFile::MemoryMax,
        CgroupFile::CpuMax,
        CgroupFile::MemorySwapMax,
        CgroupFile::PidsMax,
        CgroupFile::CpusetCpus,
        CgroupFile::DevicesAllow,
    ] {
        let refusal = view
            .write(file, "67108864")
            .expect_err("a policy limit this executor cannot enforce must be refused");
        assert_eq!(
            refusal.errno,
            CgroupErrno::Erofs,
            "{} must be refused because the cgroupfs is not writable",
            file.name()
        );
        assert_eq!(refusal.errno.code(), -30);
        assert_eq!(
            refusal.to_string(),
            format!(
                "{}: EROFS (-30) - no writable cgroup2 mount is available to this process, so {} \
                 cannot be enforced, and this view will not represent a limit it does not have",
                file.name(),
                file.name()
            ),
            "the refusal must name the file, the errno and the reason"
        );
    }
}

/// (b) the same refusal driven through the supervisor, plus the absence assertion that would
/// have caught the original dishonesty: with a limit requested and cgroups enabled, the
/// service still starts and no ordinary file anywhere claims a limit was applied.
#[tokio::test]
async fn no_file_claims_a_limit_the_supervisor_could_not_bind() {
    let temp = tempdir().expect("tempdir");
    let runtime_dir = temp.path().join("runtime");

    let mut config = service_config("view-refused");
    config.resources = ResourceLimits {
        cpu_limit: Some(0.5),
        memory_limit: Some(64 * 1024 * 1024),
        pids_limit: Some(16),
        ..ResourceLimits::default()
    };

    let sup = supervisor(temp.path(), true);
    sup.create_service(config)
        .await
        .expect("register the service");
    sup.start_service("view-refused").await.expect(
        "a service must start even when a cgroup limit cannot be bound; skipping is honest, \
         refusing the service is not",
    );

    // NB if this host DOES have a writable cgroup2 mount, the group lives under that mount
    // and never under the runtime dir - either way nothing in the runtime tree is a cgroup.
    let mut files = Vec::new();
    collect(&runtime_dir, &mut files);
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
}

fn service_config(name: &str) -> ServiceConfig {
    ServiceConfig {
        meta: ServiceMeta {
            name: name.to_string(),
            description: format!("S8 fixture {name}"),
            version: "1.0.0".to_string(),
            service_type: ServiceType::Daemon,
            tags: vec![],
        },
        process: ProcessConfig {
            // `/bin/true` exits at once; the test only needs `start_service` to reach the
            // cgroup step.
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
