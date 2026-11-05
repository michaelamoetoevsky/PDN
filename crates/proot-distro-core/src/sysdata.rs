//! Fake `/proc` entries for containers.
//!
//! Android restricts or outright blocks a number of `/proc` files, so anything
//! inside a container that reads them (`top`, `htop`, `free`, `uptime`,
//! `nproc`) sees a permission error or nothing at all. proot-distro works
//! around this by writing static stand-ins into `containers/<name>/sysdata/`
//! and bind-mounting them over the real paths; this module does the same.
//!
//! Two rules matter:
//!
//! * The stubs live *beside* the rootfs, never inside it. The rootfs `/proc` is
//!   masked by `--bind=/proc`, so a file written into the image is invisible —
//!   which is what the previous implementation of this module did, and why none
//!   of it ever had an effect.
//! * A stub is bound only when the host's own file cannot be read, so a device
//!   that does expose the real file keeps the real file.
//!
//! The content is static except for `stat` and `uptime`, which are regenerated
//! on every call so their `cpuN` lines agree with the CPU count of the device
//! the container is started on.

use anyhow::Result;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `<name under sysdata/>`, `<path inside the guest>`
const ENTRIES: &[(&str, &str)] = &[
    ("loadavg", "/proc/loadavg"),
    ("stat", "/proc/stat"),
    ("uptime", "/proc/uptime"),
    ("version", "/proc/version"),
    ("vmstat", "/proc/vmstat"),
    ("filesystems", "/proc/filesystems"),
    ("cmdline", "/proc/cmdline"),
    (
        "sysctl_kernel_cap_last_cap",
        "/proc/sys/kernel/cap_last_cap",
    ),
    (
        "sysctl_inotify_max_user_watches",
        "/proc/sys/fs/inotify/max_user_watches",
    ),
    ("sysctl_kernel_overflowuid", "/proc/sys/kernel/overflowuid"),
    ("sysctl_kernel_overflowgid", "/proc/sys/kernel/overflowgid"),
];

const FAKE_LOADAVG: &str = "0.12 0.07 0.02 2/165 765\n";
const FAKE_OVERFLOW_ID: &str = "65534\n";
const FAKE_CAP_LAST_CAP: &str = "40\n";
const FAKE_INOTIFY_WATCHES: &str = "4096\n";

const FAKE_CMDLINE: &str = "boot_cpu=0 quiet\n";

const FAKE_MOUNTS: &str = "\
rootfs / rootfs rw,relatime 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
devpts /dev/pts devpts rw,nosuid,noexec,relatime 0 0
";

const FAKE_FILESYSTEMS: &str = "\
nodev\tsysfs
nodev\trootfs
nodev\tramfs
nodev\tbdev
nodev\tproc
nodev\tcpuset
nodev\tcgroup
nodev\tcgroup2
nodev\ttmpfs
nodev\tdevtmpfs
nodev\tconfigfs
nodev\tdebugfs
nodev\ttracefs
nodev\tsecurityfs
nodev\tsockfs
nodev\tbpf
nodev\tpipefs
nodev\tdevpts
\text2
\text3
\text4
\tvfat
\tmsdos
\tiso9660
\tsquashfs
nodev\toverlay
nodev\tfuse
nodev\tfusectl
";

const FAKE_MOUNTS: &str = "\
rootfs / rootfs rw 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
devtmpfs /dev devtmpfs rw,nosuid,relatime,size=65536k,nr_inodes=16384,mode=755 0 0
tmpfs /dev/shm tmpfs rw,nosuid,nodev,noexec,relatime,size=65536k 0 0
tmpfs /tmp tmpfs rw,nosuid,nodev,relatime,size=65536k 0 0
";

const FAKE_NET_TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
const FAKE_NET_TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
const FAKE_NET_UDP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n";
const FAKE_NET_UDP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n";
const FAKE_NET_DEV: &str = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0\n";
const FAKE_NET_ROUTE: &str =
    "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n";
const FAKE_NET_ARP: &str =
    "IP address       HW type     Flags       HW address            Mask     Device\n";
const FAKE_NET_UNIX: &str = "Num       RefCount Protocol Flags    Type St Path\n";
const FAKE_NET_SOCKSTAT: &str =
    "sockets: used 0\nTCP: inuse 0 orphan 0 tw 0 alloc 0 mem 0\nUDP: inuse 0 mem 0\n";
const FAKE_NET_NETSTAT: &str = "TcpExt: SyncookiesFailed 0\n";
const FAKE_NET_SNMP: &str = "Ip: Forwarding DefaultTTL InReceives\nIp: 2 64 0\n";
const FAKE_UPTIME_SECONDS: &str = "124.08";
const FAKE_VMSTAT: &str = "\
nr_free_pages 1743136
nr_zone_inactive_anon 179281
nr_zone_active_anon 7183
nr_zone_inactive_file 22858
nr_zone_active_file 51328
nr_zone_unevictable 642
nr_zone_write_pending 0
nr_mlock 0
nr_bounce 0
nr_zspages 0
nr_free_cma 0
numa_hit 1259626
numa_miss 0
numa_foreign 0
numa_interleave 720
pgpgin 200232
pgpgout 180764
pswpin 0
pswpout 0
";
const FAKE_STAT_TAIL: &str = "\
intr 127541 38 290 0 0 0 0 4 0 1 0 0 25329 258 0 5777 277 0 0 0 0 0 0 0 0 0
ctxt 140223
btime 1750000000
processes 8944
procs_running 2
procs_blocked 0
>>>>>>> 5b40f68 (fix(proc): sysdata virtualization, /sys empty mask, /proc/net stubs, umask (Batch G))
";

/// System uptime in seconds (from CLOCK_BOOTTIME).
pub fn system_uptime() -> f64 {
    unsafe {
        let mut ts: libc::timespec = std::mem::zeroed();
        if libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) == 0 {
            ts.tv_sec as f64 + (ts.tv_nsec as f64 / 1_000_000_000.0)
        } else {
            124.08
        }
    }
}

/// Boot time in seconds since Unix epoch, matching `system_uptime`.
pub fn boot_time_seconds(uptime: f64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    if now > uptime {
        (now - uptime) as u64
    } else {
        0
    }
}

fn host_mem_free_kb() -> Option<u64> {
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    for line in content.lines() {
        if line.starts_with("MemFree:") {
            let mut parts = line.split_whitespace();
            parts.next(); // "MemFree:"
            return parts.next().and_then(|val| val.parse::<u64>().ok());
        }
    }
    None
}

pub fn fake_vmstat() -> String {
    let free_pages = host_mem_free_kb().map(|kb| kb / 4).unwrap_or(1743136);
    format!(
        "nr_free_pages {}\n\
nr_zone_inactive_anon 179281\n\
nr_zone_active_anon 7183\n\
nr_zone_inactive_file 22858\n\
nr_zone_active_file 51328\n\
nr_zone_unevictable 642\n\
nr_zone_write_pending 0\n\
nr_mlock 0\n\
nr_bounce 0\n\
nr_zspages 0\n\
nr_free_cma 0\n\
numa_hit 1259626\n\
numa_miss 0\n\
numa_foreign 0\n\
numa_interleave 720\n\
pgpgin 200232\n\
pgpgout 180764\n\
pswpin 0\n\
pswpout 0\n",
        free_pages
    )
}

/// Per-CPU snapshot in USER_HZ ticks, in `/proc/stat` column order:
/// user nice system idle iowait irq softirq steal guest guest_nice.
/// Cycled when the host has more than eight CPUs.
const FAKE_CPU_ROWS: [[u64; 10]; 8] = [
    [31, 0, 226, 12027, 82, 10, 4, 9, 0, 0],
    [45, 0, 664, 11144, 21, 263, 233, 12, 0, 0],
    [494, 0, 537, 11283, 27, 10, 3, 8, 0, 0],
    [359, 0, 234, 11723, 24, 26, 5, 7, 0, 0],
    [295, 0, 268, 11772, 10, 12, 2, 12, 0, 0],
    [270, 0, 251, 11833, 15, 3, 1, 10, 0, 0],
    [430, 0, 520, 11386, 30, 8, 1, 12, 0, 0],
    [30, 0, 172, 12108, 50, 8, 1, 13, 0, 0],
];

const IDLE_COLUMN: usize = 3;
const USER_HZ: u64 = 100;

/// Directory holding the stubs: a sibling of the rootfs, so it is removed
/// together with the container.
pub fn sysdata_dir(container_dir: &Path) -> PathBuf {
    container_dir.join("sysdata")
}

/// True when `path` can be opened and a byte read from it.
fn readable(path: &str) -> bool {
    match fs::File::open(path) {
        Ok(mut f) => {
            let mut byte = [0u8; 1];
            f.read(&mut byte).is_ok()
        }
        Err(_) => false,
    }
}

/// True when `path` directory can be listed.
fn readable_dir(path: &str) -> bool {
    match fs::read_dir(path) {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// True when `path` metadata can be queried without permission errors.
fn readable_path(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

fn boottime_seconds() -> f64 {
    unsafe {
        let mut ts: libc::timespec = std::mem::zeroed();
        if libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) == 0 {
            return ts.tv_sec as f64 + (ts.tv_nsec as f64 / 1_000_000_000.0);
        }
    }
    124.08
}

/// Kernel release as the running system reports it.
///
/// PDN deliberately does not fake the guest's identity, so `/proc/version`
/// describes the real kernel rather than proot-distro's synthetic
/// `6.17.0-PRoot-Distro`.
fn kernel_release() -> String {
    // SAFETY: `uname` only writes into the struct handed to it.
    unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) == 0 {
            let bytes: Vec<u8> = uts
                .release
                .iter()
                .take_while(|c| **c != 0)
                .map(|c| *c as u8)
                .collect();
            if let Ok(s) = String::from_utf8(bytes) {
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    "0.0.0".to_string()
}

/// Online CPU ids, mirroring what the guest's own `/proc/cpuinfo` will list.
pub fn host_cpu_ids() -> Vec<u32> {
    let mut ids = Vec::new();
    if let Ok(text) = fs::read_to_string("/proc/cpuinfo") {
        for line in text.lines() {
            if let Some((key, value)) = line.split_once(':') {
                if key.trim() == "processor" {
                    if let Ok(n) = value.trim().parse::<u32>() {
                        ids.push(n);
                    }
                }
            }
        }
    }
    if ids.is_empty() {
        if let Ok(online) = fs::read_to_string("/sys/devices/system/cpu/online") {
            for part in online.trim().split(',') {
                if let Some((start, end)) = part.split_once('-') {
                    if let (Ok(s), Ok(e)) = (start.trim().parse::<u32>(), end.trim().parse::<u32>())
                    {
                        for i in s..=e {
                            ids.push(i);
                        }
                    }
                } else if let Ok(n) = part.trim().parse::<u32>() {
                    ids.push(n);
                }
            }
        }
    }
    if ids.is_empty() {
        // SAFETY: `sysconf` is a plain query with no side effects.
        let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
        ids = (0..if n > 0 { n as u32 } else { 1 }).collect();
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn cpu_rows(cpus: &[u32]) -> Vec<(u32, [u64; 10])> {
    if cpus.is_empty() {
        return vec![(0, FAKE_CPU_ROWS[0])];
    }
    cpus.iter()
        .enumerate()
        .map(|(i, cpu)| (*cpu, FAKE_CPU_ROWS[i % FAKE_CPU_ROWS.len()]))
        .collect()
}

fn fake_stat_tail() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1750000000);
    let uptime = boottime_seconds().round() as u64;
    let btime = if now > uptime {
        now - uptime
    } else {
        1750000000
    };
    format!(
        "intr 127541 38 290 0 0 0 0 4 0 1 0 0 25329 258 0 5777 277 0 0 0 0 0 0 0 0 0\n\
ctxt 140223\n\
btime {}\n\
processes 8944\n\
procs_running 2\n\
procs_blocked 0\n",
        btime
    )
}

/// `/proc/stat` with one `cpuN` line per online CPU. The aggregate `cpu` line
/// is the column-wise sum of those, as the kernel's is, so a tool that
/// cross-checks the two sees them agree.
pub fn fake_stat(cpus: &[u32]) -> String {
    let uptime = system_uptime();
    fake_stat_with_uptime(cpus, uptime)
}

pub fn fake_stat_with_uptime(cpus: &[u32], uptime: f64) -> String {
    let rows = cpu_rows(cpus);
    let mut totals = [0u64; 10];
    for (_, row) in &rows {
        for (t, v) in totals.iter_mut().zip(row.iter()) {
            *t += *v;
        }
    }
    let join = |row: &[u64; 10]| row.iter().map(u64::to_string).collect::<Vec<_>>().join(" ");
    let mut out = format!("cpu  {}\n", join(&totals));
    for (cpu, row) in &rows {
        out.push_str(&format!("cpu{} {}\n", cpu, join(row)));
    }
    let btime = boot_time_seconds(uptime);
    out.push_str(&format!(
        "intr 127541 38 290 0 0 0 0 4 0 1 0 0 25329 258 0 5777 277 0 0 0 0 0 0 0 0 0\n\
         ctxt 140223\n\
         btime {}\n\
         processes 8944\n\
         procs_running 2\n\
         procs_blocked 0\n",
        btime
    ));
    out
}

/// `/proc/uptime`, the second field being total idle ticks over all CPUs so it
/// stays consistent with the idle column of `cpu` in `fake_stat`.
pub fn fake_uptime(cpus: &[u32]) -> String {
    let uptime = system_uptime();
    fake_uptime_with_uptime(cpus, uptime)
}

pub fn fake_uptime_with_uptime(cpus: &[u32], uptime: f64) -> String {
    let idle: u64 = cpu_rows(cpus).iter().map(|(_, r)| r[IDLE_COLUMN]).sum();
    format!("{:.2} {:.2}\n", uptime, idle as f64 / USER_HZ as f64)
}

fn fake_vmstat() -> String {
    let mut free_pages = 1743136u64;
    if let Ok(mem) = fs::read_to_string("/proc/meminfo") {
        for line in mem.lines() {
            if line.starts_with("MemFree:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        free_pages = kb / 4;
                    }
                }
                break;
            }
        }
    }
    format!(
        "nr_free_pages {}\n\
nr_zone_inactive_anon 179281\n\
nr_zone_active_anon 7183\n\
nr_zone_inactive_file 22858\n\
nr_zone_active_file 51328\n\
nr_zone_unevictable 642\n\
nr_zone_write_pending 0\n\
nr_mlock 0\n\
nr_bounce 0\n\
nr_zspages 0\n\
nr_free_cma 0\n\
numa_hit 1259626\n\
numa_miss 0\n\
numa_foreign 0\n\
numa_interleave 720\n\
pgpgin 200232\n\
pgpgout 180764\n\
pswpin 0\n\
pswpout 0\n",
        free_pages
    )
}

fn fake_version() -> String {
    format!(
        "Linux version {} (proot@termux) (gcc (GCC) 13.3.0, GNU ld (GNU Binutils) 2.42) \
         #1 SMP PREEMPT_DYNAMIC Fri, 10 Oct 2025 00:00:00 +0000\n",
        kernel_release()
    )
}

/// Write the stubs under `<container_dir>/sysdata/`.
///
/// Idempotent. An entry that is not a plain file — a symlink or a directory
/// planted where a stub belongs — is replaced rather than followed, so a guest
/// cannot leave a link behind that later sessions mount into the guest.
pub fn ensure(container_dir: &Path) -> Result<PathBuf> {
    let dir = sysdata_dir(container_dir);
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(dir.join("sys_empty"))?;

    // Create stub /dev hierarchy for restricted environments
    let dev_dir = dir.join("dev");
    fs::create_dir_all(&dev_dir)?;
    fs::create_dir_all(dev_dir.join("shm"))?;
    fs::create_dir_all(dev_dir.join("pts"))?;
    for dev_node in ["null", "zero", "random", "urandom", "ptmx", "tty"] {
        let p = dev_dir.join(dev_node);
        if !p.exists() {
            let _ = fs::write(&p, b"");
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let symlinks = [
            ("/proc/self/fd", dev_dir.join("fd")),
            ("/proc/self/fd/0", dev_dir.join("stdin")),
            ("/proc/self/fd/1", dev_dir.join("stdout")),
            ("/proc/self/fd/2", dev_dir.join("stderr")),
        ];
        for (src, dst) in symlinks {
            if !dst.exists() && fs::symlink_metadata(&dst).is_err() {
                let _ = symlink(src, &dst);
            }
        }
    }

    // Create stub /proc/net hierarchy
    let net_dir = dir.join("net");
    fs::create_dir_all(&net_dir)?;
    let net_entries = [
        ("tcp", FAKE_NET_TCP),
        ("tcp6", FAKE_NET_TCP6),
        ("udp", FAKE_NET_UDP),
        ("udp6", FAKE_NET_UDP6),
        ("dev", FAKE_NET_DEV),
        ("route", FAKE_NET_ROUTE),
        ("arp", FAKE_NET_ARP),
        ("unix", FAKE_NET_UNIX),
        ("sockstat", FAKE_NET_SOCKSTAT),
        ("netstat", FAKE_NET_NETSTAT),
        ("snmp", FAKE_NET_SNMP),
    ];
    for (name, content) in net_entries {
        let p = net_dir.join(name);
        if !p.exists() {
            let _ = fs::write(&p, content);
        }
    }

    let cpus = host_cpu_ids();
    let uptime = system_uptime();
    let entries = [
        ("loadavg", FAKE_LOADAVG.to_string()),
        ("stat", fake_stat_with_uptime(&cpus, uptime)),
        ("uptime", fake_uptime_with_uptime(&cpus, uptime)),
        ("version", fake_version()),
        ("vmstat", fake_vmstat()),
        ("filesystems", FAKE_FILESYSTEMS.to_string()),
        ("cmdline", FAKE_CMDLINE.to_string()),
        ("mounts", FAKE_MOUNTS.to_string()),
        ("sysctl_kernel_cap_last_cap", FAKE_CAP_LAST_CAP.to_string()),
        (
            "sysctl_inotify_max_user_watches",
            FAKE_INOTIFY_WATCHES.to_string(),
        ),
        ("sysctl_kernel_overflowuid", FAKE_OVERFLOW_ID.to_string()),
        ("sysctl_kernel_overflowgid", FAKE_OVERFLOW_ID.to_string()),
    ];

    for (name, content) in entries {
        let path = dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_file() => {
                if fs::read_to_string(&path).ok().as_deref() == Some(content.as_str()) {
                    continue;
                }
            }
            Ok(_) => {
                // Wrong type (symlink, directory): drop it, do not follow it.
                fs::remove_file(&path).or_else(|_| fs::remove_dir_all(&path))?;
            }
            Err(_) => {}
        }
        fs::write(&path, content)?;
    }

    Ok(dir)
}

/// `--bind=<src>:<dst>` arguments for the stubs whose host counterpart cannot
/// be read. Order matters: these are appended after `--bind=/proc` so they win.
pub fn bindings(container_dir: &Path) -> Vec<String> {
    let dir = sysdata_dir(container_dir);
    if !dir.is_dir() {
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::new();

    // If host /sys is not readable (e.g. Android SELinux denies reading /sys dir),
    // mask it with sys_empty so `ls /sys` succeeds without EACCES.
    let empty = dir.join("sys_empty");
    if !readable_dir("/sys") && empty.is_dir() {
        out.push(format!("{}:/sys", empty.display()));
    } else if empty.is_dir() {
        // Android keeps SELinux state out of reach; an empty directory stands in.
        out.push(format!("{}:/sys/fs/selinux", empty.display()));
    }

    // Mask /dev if host /dev directory cannot be listed
    let dev_dir = dir.join("dev");
    if !readable_dir("/dev") && dev_dir.is_dir() {
        out.push(format!("{}:/dev", dev_dir.display()));
        for d in [
            "/dev/null",
            "/dev/zero",
            "/dev/urandom",
            "/dev/random",
            "/dev/ptmx",
            "/dev/tty",
        ] {
            if readable(d) {
                out.push(format!("{}:{}", d, d));
            }
        }
    }

    // Mask /proc/net if blocked
    let net_dir = dir.join("net");
    if !readable("/proc/net/tcp") && net_dir.is_dir() {
        out.push(format!("{}:/proc/net", net_dir.display()));
    }

    // Mask /proc/1 if blocked
    if !readable_path("/proc/1") {
        out.push("/proc/self:/proc/1".to_string());
    }

    // Truthful mount table showing rootfs as rw. Trailing ! avoids dereferencing symlink.
    let mounts_path = dir.join("mounts");
    if mounts_path.is_file() {
        out.push(format!("{}:/proc/mounts!", mounts_path.display()));
        out.push(format!("{}:/etc/mtab!", mounts_path.display()));
    }
    // Mask host processes so guest cannot inspect host process command lines.
    if empty.is_dir() {
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.chars().all(|c| c.is_ascii_digit()) {
                        out.push(format!("{}:/proc/{}", empty.display(), name));
                    }
                }
            }
        }
    }
    for (name, guest_path) in ENTRIES {
        if readable(guest_path) {
            continue;
        }
        let stub = dir.join(name);
        if stub.is_file() {
            out.push(format!("{}:{}", stub.display(), guest_path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_aggregate_equals_column_sum() {
        let stat = fake_stat(&[0, 1, 2, 3]);
        // one `cpu` + four `cpuN`
        assert_eq!(stat.lines().filter(|l| l.starts_with("cpu")).count(), 5);

        let mut sum = [0u64; 10];
        for line in stat
            .lines()
            .filter(|l| l.starts_with("cpu") && l.split_whitespace().next() != Some("cpu"))
        {
            for (i, field) in line.split_whitespace().skip(1).enumerate() {
                sum[i] += field.parse::<u64>().unwrap();
            }
        }
        let agg: Vec<u64> = stat
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .skip(1)
            .map(|f| f.parse().unwrap())
            .collect();
        assert_eq!(agg, sum.to_vec());
    }

    #[test]
    fn uptime_idle_matches_stat_idle_column() {
        let cpus = [0u32, 1, 2, 3];
        let idle: u64 = cpu_rows(&cpus).iter().map(|(_, r)| r[IDLE_COLUMN]).sum();
        let uptime = fake_uptime(&cpus);
        assert_eq!(
            uptime.split_whitespace().nth(1).unwrap(),
            format!("{:.2}", idle as f64 / USER_HZ as f64)
        );
    }

    #[test]
    fn uptime_and_stat_btime_are_temporally_consistent() {
        let cpus = [0u32, 1];
        let uptime_val = 5000.50;
        let uptime_str = fake_uptime_with_uptime(&cpus, uptime_val);
        let stat_str = fake_stat_with_uptime(&cpus, uptime_val);

        let up: f64 = uptime_str
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let btime_line = stat_str.lines().find(|l| l.starts_with("btime ")).unwrap();
        let btime: u64 = btime_line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let expected_btime = (now - uptime_val) as u64;
        assert!((btime as i64 - expected_btime as i64).abs() <= 2);
        assert!((up - uptime_val).abs() < 0.01);
    }

    #[test]
    fn ensure_writes_every_entry_then_bindings_cover_the_blocked_ones() {
        let tmp = std::env::temp_dir().join(format!("pdn-sysdata-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let dir = ensure(&tmp).expect("ensure");
        for (name, _) in ENTRIES {
            assert!(dir.join(name).is_file(), "missing stub {name}");
        }
        assert!(dir.join("mounts").is_file(), "missing mounts stub");
        assert!(dir.join("sys_empty").is_dir());

        let b = bindings(&tmp);
        // Truthful mounts table bound directly
        assert!(b.iter().any(|a| a.ends_with(":/proc/mounts!")));
        // SELinux empty dir bound
        assert!(b.iter().any(|a| a.ends_with(":/sys/fs/selinux")));
        // Every entry whose host file is blocked is bound.
        for (_, guest_path) in ENTRIES {
            if !readable(guest_path) {
                assert!(
                    b.iter().any(|a| a.ends_with(&format!(":{}", guest_path))),
                    "missing binding for blocked {guest_path}"
                );
            }
        }
        assert!(b
            .iter()
            .any(|a| a.contains("sys_empty") || a.ends_with(":/sys")));
        // If /proc/uptime is blocked on this host (e.g. Android), verify it is bound.
        if !readable("/proc/uptime") {
            assert!(b.iter().any(|a| a.ends_with(":/proc/uptime")));
        }
        // A host path that IS readable is never shadowed.
        assert!(!b.iter().any(|a| a.ends_with(":/etc/hostname")));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ensure_replaces_a_planted_symlink() {
        let tmp = std::env::temp_dir().join(format!("pdn-sysdata-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let dir = sysdata_dir(&tmp);
        fs::create_dir_all(&dir).unwrap();

        let victim = tmp.join("host-secret");
        fs::write(&victim, "host data").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, dir.join("loadavg")).unwrap();

        ensure(&tmp).expect("ensure");
        let meta = fs::symlink_metadata(dir.join("loadavg")).unwrap();
        assert!(
            meta.file_type().is_file(),
            "symlink should have been replaced"
        );
        assert_eq!(
            fs::read_to_string(dir.join("loadavg")).unwrap(),
            FAKE_LOADAVG
        );
        assert_eq!(fs::read_to_string(&victim).unwrap(), "host data");

        let _ = fs::remove_dir_all(&tmp);
    }
}
