//! A private cgroupfs view (adoption S8 of `research/DISSECTION_SHROOT.md`).
//!
//! The reference is `native/shroot/src/cgroup.c` at commit `7ab99d6`. That file is not
//! enforcement: it is a userspace *view* of cgroupfs. It serves a private file enum
//! (`PROCS, THREADS, EVENTS, CONTROLLERS, SUBTREE, TYPE, FREEZE, ALLOW, DENY, LIST, TASKS,
//! NOTIFY, RELEASE, PIDS_MAX, PIDS_CURRENT, PIDS_EVENTS`, source lines 25-26) with per-file
//! modes 0644/0444 (`:31-33`), answers `mkdir` and subtree writes itself (`md_cgroup_mkdir`
//! at `:755`, `subtree_write` at `:872`), and produces its own errno values (`-EBUSY`,
//! `-EPERM`, `-EACCES`, `-EROFS` at `:781-986`) for what it will not honour. Nothing in it
//! writes to the host's `/sys/fs/cgroup`. Its own documentation states the ceiling plainly
//! (`docs/shroot.md:75`): hierarchical `pids.max`, membership, freezer and sampled CPU/RSS
//! counters are served, while "cgroup CPU, cpuset, memory and device policies are not
//! enforced" and "sampled RSS is not kernel page charging".
//!
//! PDN had the other shape, and the shape outlived the first fix. `supervisor.rs` wrote
//! `cgroup.procs`, `cpu.max` and `memory.max` into an ordinary directory and returned `Ok()`
//! (WORKLIST A20): the write succeeded, the caller believed a limit was in force, and no
//! kernel limit existed. Wave 5 named the skip (A20 v2) rather than refusing, but a limit was
//! still *represented by files*, and the only reason it was not faked here was that a probe
//! happened to fail. This module is the structural fix: one view with a named verdict per
//! knob, answering reads from the platform (`/proc`, the only source that is true on a host
//! where cgroupfs is not writable) and refusing the rest by errno, so "a limit is in force"
//! is never something a caller can infer from a completed write.

use std::fmt;
use std::path::{Path, PathBuf};

/// A cgroupfs control file the view knows by name.
///
/// The set is the part of shroot's own enum that PDN has a caller for; the names are the
/// file names in its control table (`cgroup.c:31-39`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgroupFile {
    /// `cgroup.procs` - the tasks in the group.
    Procs,
    /// `cgroup.freeze` - the freezer.
    Freeze,
    /// `cpu.max` - a CPU bandwidth limit.
    CpuMax,
    /// `cpu.stat` - sampled CPU accounting.
    CpuStat,
    /// `memory.max` - a hard memory limit.
    MemoryMax,
    /// `memory.swap.max` - a swap limit.
    MemorySwapMax,
    /// `memory.current` - sampled resident memory.
    MemoryCurrent,
    /// `pids.max` - a task-count limit.
    PidsMax,
    /// `cpuset.cpus` - CPU affinity as a cgroup policy.
    CpusetCpus,
    /// `devices.allow` - a device policy.
    DevicesAllow,
    /// `io.max` - an I/O bandwidth or IOPS limit.
    IoMax,
}

impl CgroupFile {
    /// The file's name in the cgroupfs the view presents.
    pub const fn name(self) -> &'static str {
        match self {
            CgroupFile::Procs => "cgroup.procs",
            CgroupFile::Freeze => "cgroup.freeze",
            CgroupFile::CpuMax => "cpu.max",
            CgroupFile::CpuStat => "cpu.stat",
            CgroupFile::MemoryMax => "memory.max",
            CgroupFile::MemorySwapMax => "memory.swap.max",
            CgroupFile::MemoryCurrent => "memory.current",
            CgroupFile::PidsMax => "pids.max",
            CgroupFile::CpusetCpus => "cpuset.cpus",
            CgroupFile::DevicesAllow => "devices.allow",
            CgroupFile::IoMax => "io.max",
        }
    }

    /// Whether the file is a resource *policy*: something that would have to be enforced
    /// by the kernel for the value written to mean anything.
    ///
    /// shroot's documentation is the authority for this split (`docs/shroot.md:75`): these
    /// are exactly the policies it states it does not enforce, and the reason it gives -
    /// they "need a rootful executor and a delegated subtree" - is true of PDN's executor
    /// (Android shell UID 2000 inside Termux) as well.
    pub const fn is_policy(self) -> bool {
        matches!(
            self,
            CgroupFile::CpuMax
                | CgroupFile::MemoryMax
                | CgroupFile::MemorySwapMax
                | CgroupFile::PidsMax
                | CgroupFile::CpusetCpus
                | CgroupFile::DevicesAllow
                | CgroupFile::IoMax
        )
    }
}

/// The errno the view returns for a knob it will not honour, from shroot's own set
/// (`cgroup.c:781-986`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgroupErrno {
    /// `EBUSY` - the operation cannot be completed on a populated group.
    Ebusy,
    /// `EPERM` - the operation is not permitted for this executor.
    Eperm,
    /// `EACCES` - access to the file itself was denied.
    Eacces,
    /// `EROFS` - the cgroupfs at hand is not writable by this process.
    Erofs,
}

impl CgroupErrno {
    /// The errno as the kernel names it.
    pub const fn name(self) -> &'static str {
        match self {
            CgroupErrno::Ebusy => "EBUSY",
            CgroupErrno::Eperm => "EPERM",
            CgroupErrno::Eacces => "EACCES",
            CgroupErrno::Erofs => "EROFS",
        }
    }

    /// The negative Linux errno, so a caller can report the number the kernel would.
    pub const fn code(self) -> i32 {
        match self {
            // Linux errno values are architecture-independent for these four.
            CgroupErrno::Ebusy => -16,
            CgroupErrno::Eperm => -1,
            CgroupErrno::Eacces => -13,
            CgroupErrno::Erofs => -30,
        }
    }
}

/// A named refusal: which file, which errno, and why.
///
/// Every refusal the view produces carries all three, so a caller can log an operation
/// as *not applied* without inventing a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupRefusal {
    /// The file that was asked for.
    pub file: CgroupFile,
    /// The errno the view answers with.
    pub errno: CgroupErrno,
    /// Why this executor cannot honour the file.
    pub reason: String,
}

impl fmt::Display for CgroupRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} ({}) - {}",
            self.file.name(),
            self.errno.name(),
            self.errno.code(),
            self.reason
        )
    }
}

impl std::error::Error for CgroupRefusal {}

/// A private cgroupfs view over the processes this process supervises.
///
/// Reads are answered from the platform (`/proc`), which is true whether or not cgroupfs is
/// writable. Writes are served only where the platform allows - membership and the freezer
/// against a backed group directory - and refused by errno everywhere else. A resource
/// policy is refused even then, because PDN's executor cannot make the kernel enforce it.
#[derive(Debug)]
pub struct CgroupView {
    /// The group directory, present only when it is a real subtree of a cgroup2 mount this
    /// process can write. It is never a plain directory chosen by the caller.
    group: Option<PathBuf>,
    /// The group's members (TIDs): the view's own book-keeping for `cgroup.procs`.
    members: Vec<u32>,
}

impl Default for CgroupView {
    fn default() -> Self {
        Self::for_group(None, "pn-next")
    }
}

impl CgroupView {
    /// A view for a group named `name`, optionally under a configured `mount`.
    ///
    /// `mount`, when given, is used only if it is the cgroup2 mount this process can
    /// actually write - the same probe [`crate::supervisor::writable_cgroupfs`] performs. A
    /// configured path that is an ordinary directory never becomes the target for a limit;
    /// that substitution is the whole bug this module exists to remove (WORKLIST A20).
    /// Without a writable cgroup2 mount the view has no backing tree at all, and refuses
    /// rather than falling back to a plain directory.
    pub fn for_group(mount: Option<&Path>, name: &str) -> Self {
        let base = match (mount, crate::supervisor::writable_cgroupfs()) {
            (Some(m), Some(p)) if m == p => Some(p),
            (None, p) => p,
            _ => None,
        };
        let group = base
            .map(|b| b.join("pn-next").join(name))
            .and_then(|d| std::fs::create_dir_all(&d).ok().map(|_| d));
        Self {
            group,
            members: Vec::new(),
        }
    }

    /// A view bound to an explicit group directory.
    ///
    /// A test passes a directory it controls; production calls [`CgroupView::for_group`],
    /// which resolves the directory from the platform instead of accepting one. A group
    /// that is `None` is a view with no backing cgroupfs: every write is refused.
    pub fn with_group(group: Option<PathBuf>) -> Self {
        Self {
            group,
            members: Vec::new(),
        }
    }

    /// The backing group directory, when the view has a real one.
    pub fn group(&self) -> Option<&Path> {
        self.group.as_deref()
    }

    /// Record a member: the view's answer for `cgroup.procs`.
    pub fn add_pid(&mut self, pid: u32) {
        if !self.members.contains(&pid) {
            self.members.push(pid);
        }
    }

    /// Read a knob.
    ///
    /// A knob the view can answer from the platform is answered from it, never from a
    /// constant; a knob it cannot answer is refused by name rather than approximated with
    /// an empty string that reads like a value.
    pub fn read(&self, file: CgroupFile) -> Result<String, CgroupRefusal> {
        match file {
            CgroupFile::Procs => Ok(self.procs()),
            CgroupFile::CpuStat => Ok(self.cpu_stat()),
            CgroupFile::MemoryCurrent => Ok(self.memory_current()),
            CgroupFile::Freeze => match &self.group {
                Some(group) => {
                    std::fs::read_to_string(group.join(file.name())).map_err(|_| self.refuse(file))
                }
                None => Err(self.refuse(file)),
            },
            other => Err(self.refuse(other)),
        }
    }

    /// Write a knob.
    ///
    /// Membership and the freezer are served where the group is backed by a writable
    /// cgroup2 subtree. Every resource policy is refused by errno, so a completed write is
    /// never a claim that a limit is being enforced.
    pub fn write(&self, file: CgroupFile, value: &str) -> Result<(), CgroupRefusal> {
        if file.is_policy() {
            return Err(self.refuse(file));
        }
        match (&self.group, file) {
            (Some(group), CgroupFile::Procs | CgroupFile::Freeze) => {
                std::fs::write(group.join(file.name()), value).map_err(|_| self.refuse(file))
            }
            _ => Err(self.refuse(file)),
        }
    }

    /// `cgroup.procs`: the members the platform still shows, one TID per line.
    fn procs(&self) -> String {
        let mut out = String::new();
        for pid in &self.members {
            if Path::new(&format!("/proc/{pid}")).exists() {
                out.push_str(&format!("{pid}\n"));
            }
        }
        out
    }

    /// `cpu.stat`: sampled user+system CPU time of the members, in clock ticks.
    ///
    /// The value is read from `/proc/<pid>/stat` at the moment of the call, so it is a
    /// sample of what the platform reports, not a kernel cgroup counter - shroot's own
    /// boundary, and the reason the field is labelled as sampled rather than as charging.
    fn cpu_stat(&self) -> String {
        let mut ticks: u64 = 0;
        for pid in &self.members {
            if let Some(t) = proc_cpu_ticks(*pid) {
                ticks = ticks.saturating_add(t);
            }
        }
        format!("ticks_sampled {ticks}\n")
    }

    /// `memory.current`: sampled resident memory of the members, in bytes.
    ///
    /// Read from `/proc/<pid>/status` (`VmRSS`), so - as shroot states of its own sampled
    /// figure - this is a sample and not kernel page charging.
    fn memory_current(&self) -> String {
        let mut bytes: u64 = 0;
        for pid in &self.members {
            if let Some(kb) = proc_rss_kb(*pid) {
                bytes = bytes.saturating_add(kb * 1024);
            }
        }
        bytes.to_string()
    }

    /// The named refusal for a file this view will not serve.
    fn refuse(&self, file: CgroupFile) -> CgroupRefusal {
        let (errno, reason) = match &self.group {
            None => (
                CgroupErrno::Erofs,
                format!(
                    "no writable cgroup2 mount is available to this process, so {} cannot be \
                     enforced, and this view will not represent a limit it does not have",
                    file.name()
                ),
            ),
            Some(_) if file.is_policy() => (
                CgroupErrno::Eperm,
                format!(
                    "{} is a resource policy that has to be enforced by the kernel under a \
                     delegated subtree; this executor (unprivileged, no cgroup delegation) \
                     cannot make it take effect, so it is not applied",
                    file.name()
                ),
            ),
            Some(_) => (
                CgroupErrno::Eacces,
                format!("{} is not served by this view", file.name()),
            ),
        };
        CgroupRefusal {
            file,
            errno,
            reason,
        }
    }
}

/// User+system CPU time of `pid`, in clock ticks, from `/proc/<pid>/stat`.
///
/// The fields are located after the last `)`, because the `comm` field (field 2) may itself
/// contain spaces and parentheses; `utime` and `stime` are fields 14 and 15.
fn proc_cpu_ticks(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after.split_whitespace().collect();
    // After `)` the first field is `state` (field 3); utime=14, stime=15 land at 11 and 12.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime.saturating_add(stime))
}

/// Resident set size of `pid`, in KiB, from `/proc/<pid>/status`.
fn proc_rss_kb(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}
