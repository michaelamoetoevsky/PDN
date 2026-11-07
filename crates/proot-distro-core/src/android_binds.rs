//! The Android bind set a session is given — and the termux-type exclusion.
//!
//! A guest's dynamic linker needs a few host directories (`/apex`, `/system`,
//! `/vendor`, the `linkerconfig` files) and PDN used to hand them over
//! unconditionally, from a hard-coded argument list under a comment
//! (`proot-core`'s `add_android_system_binds`). The reference instead *decides*:
//! each path is resolved, missing paths are dropped, a directory is bound only
//! when the guest could traverse it, and a file only when it can be read
//! (`proot_distro/commands/login/bindings.py:system_bindings`). It also withholds
//! the whole set from a **termux-type** rootfs — one that carries its own
//! `$PREFIX/bin/login` — because such a guest is a Termux installation of its
//! own and shares the host's `$PREFIX` path, so a host bind would let the host's
//! binaries appear inside it (`proot_cmd.py`, the `dist_type != "termux"` guards).
//!
//! Each clause here is a separate filter with its own test at the bottom of this
//! file, so the rule is *shown*, not asserted in a comment:
//!
//! * **mode digit** — a directory is bound only when the last digit of its mode
//!   is world-searchable (`1`, `5` or `7`). The reference skips the rest, since
//!   an untraversable bind is not a bind.
//! * **readability** — a file is bound only when a 1-byte read succeeds.
//! * **termux-type** — a rootfs whose *guest walk* finds `$PREFIX/bin/login` gets
//!   none of this set. The walk never follows a symlink, so a guest-planted
//!   `data -> /` cannot make PDN answer for the host's own Termux login.
//!
//! The filesystem is reached through [`HostFs`] so every branch is driven from a
//! constructed answer in a test, not from whatever this device's `/apex` happens
//! to be.

use std::ffi::CString;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};

/// The paths the reference probes, in its order.
pub const SYSTEM_PATHS: &[&str] = &[
    "/apex",
    "/odm",
    "/product",
    "/system",
    "/system_ext",
    "/vendor",
    "/linkerconfig/ld.config.txt",
    "/linkerconfig/com.android.art/ld.config.txt",
    "/plat_property_contexts",
    "/property_contexts",
];

/// The guest-relative path whose presence marks a rootfs as termux-type.
const TERMUX_LOGIN: &str = "data/data/com.termux/files/usr/bin/login";

/// Emitted when the whole set is withheld from a termux-type rootfs.
pub const SKIP_TERMUX: &str =
    "android bind set skipped: container is termux-type (guest has $PREFIX/bin/login)";

/// Emitted when the whole set is withheld from an `--isolated` session.
pub const SKIP_ISOLATED: &str = "android bind set skipped: --isolated session";

/// What a rootfs is, for the purpose of the exclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistType {
    /// A Termux installation: has its own `$PREFIX/bin/login`. Gets no host
    /// Android binds.
    Termux,
    /// Anything else. Gets the (filtered) Android bind set.
    Normal,
}

/// The facts one candidate path is judged on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub is_dir: bool,
    pub is_file: bool,
    pub mode: u32,
}

/// Filesystem access, split out so a test can drive every clause.
pub trait HostFs {
    /// `stat(p)`, or `None` when it cannot be read — the reference's "skip missing".
    fn stat(&self, path: &Path) -> Option<Stat>;
    /// `realpath(p)`; the reference binds the resolved path, and body-effort is fine.
    fn canonical(&self, path: &Path) -> PathBuf;
    /// `open(p)` and read one byte.
    fn reads_one_byte(&self, path: &Path) -> bool;
}

/// The real host filesystem.
pub struct RealFs;

impl HostFs for RealFs {
    fn stat(&self, path: &Path) -> Option<Stat> {
        let md = std::fs::metadata(path).ok()?;
        Some(Stat {
            is_dir: md.is_dir(),
            is_file: md.is_file(),
            mode: std::os::unix::fs::PermissionsExt::mode(&md.permissions()),
        })
    }

    fn canonical(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn reads_one_byte(&self, path: &Path) -> bool {
        match std::fs::File::open(path) {
            Ok(mut f) => {
                let mut b = [0u8; 1];
                f.read(&mut b).is_ok()
            }
            Err(_) => false,
        }
    }
}

/// The outcome of one decision: the `--bind` values to add and the reasons some
/// were withheld.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// `--bind=` values: a lone path for a self-bind, `source:target` otherwise.
    /// This set is all self-binds.
    pub args: Vec<String>,
    /// The exact strings the session prints for each withheld bind, in order.
    pub skipped: Vec<String>,
}

/// Decide the Android bind set for a session.
pub fn plan(fs: &dyn HostFs, dist_type: DistType, isolated: bool) -> Plan {
    let mut plan = Plan::default();

    // The exclusion: a termux-type guest is a Termux of its own and must not see
    // the host's Android binds at all.
    if dist_type == DistType::Termux {
        plan.skipped.push(SKIP_TERMUX.to_string());
        return plan;
    }
    // An isolated session gets no host directories either.
    if isolated {
        plan.skipped.push(SKIP_ISOLATED.to_string());
        return plan;
    }

    for path in SYSTEM_PATHS {
        let real = fs.canonical(Path::new(path));
        // Missing paths are dropped silently, exactly as the reference does.
        let Some(st) = fs.stat(&real) else {
            continue;
        };
        if st.is_dir {
            let digit = st.mode & 0o7;
            if matches!(digit, 1 | 5 | 7) {
                plan.args.push(real.to_string_lossy().into_owned());
            } else {
                plan.skipped.push(format!(
                    "android bind {} skipped: mode {:04o} is not world-searchable \
                     (last digit {digit}, need 1, 5 or 7)",
                    real.display(),
                    st.mode & 0o7777
                ));
            }
        } else if st.is_file {
            if fs.reads_one_byte(&real) {
                plan.args.push(real.to_string_lossy().into_owned());
            } else {
                plan.skipped.push(format!(
                    "android bind {} skipped: the 1-byte read failed",
                    real.display()
                ));
            }
        }
    }

    plan
}

/// [`plan`] against the real host, with the distro type detected from `rootfs`.
pub fn live(rootfs: &Path, isolated: bool) -> Plan {
    plan(&RealFs, detect_dist_type(rootfs), isolated)
}

/// Add the Android bind set to a rendered `--bind` list, tracing each withheld
/// bind. One call per session path, so login, run and the direct path agree.
pub fn extend_binds(bind: &mut Vec<String>, rootfs: &Path, isolated: bool) {
    let plan = live(rootfs, isolated);
    for note in &plan.skipped {
        pn_trace!("[android binds] {note}");
    }
    bind.extend(plan.args);
}

/// `Termux` iff `rootfs`'s guest walk finds `$PREFIX/bin/login`.
pub fn detect_dist_type(rootfs: &Path) -> DistType {
    if guest_file_exists(rootfs, TERMUX_LOGIN) {
        DistType::Termux
    } else {
        DistType::Normal
    }
}

/// Does `rootfs/rel` exist as a regular file, asked of the guest walk?
///
/// Every component is opened with `O_NOFOLLOW` relative to the previous one, so a
/// symlink anywhere — including a guest-planted `data -> /` — makes the answer
/// `false` rather than letting the walk escape to the host. The reference makes
/// this exact point: a composed-path `isfile()` followed such a link straight to
/// the host's own Termux login binary, and that answer changes the whole shape of
/// the session.
pub fn guest_file_exists(rootfs: &Path, rel: &str) -> bool {
    let Ok(root) = std::fs::File::open(rootfs) else {
        return false;
    };
    let mut owned: Option<std::fs::File> = None;
    let mut dir = root.as_raw_fd();
    let comps: Vec<&str> = rel.split('/').filter(|c| !c.is_empty()).collect();
    if comps.is_empty() {
        return false;
    }

    for (i, comp) in comps.iter().enumerate() {
        let Ok(c) = CString::new(*comp) else {
            return false;
        };
        let last = i + 1 == comps.len();
        let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        let fd = unsafe { libc::openat(dir, c.as_ptr(), flags) };
        if fd < 0 {
            return false;
        }

        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let fstat_ok = unsafe { libc::fstat(fd, &mut st) } == 0;
        if last {
            let is_reg = fstat_ok && (st.st_mode & libc::S_IFMT) == libc::S_IFREG;
            unsafe { libc::close(fd) };
            return is_reg;
        }
        let is_dir = fstat_ok && (st.st_mode & libc::S_IFMT) == libc::S_IFDIR;
        if !is_dir {
            unsafe { libc::close(fd) };
            return false;
        }
        // Hold this component open and descend from it; drop the previous one
        // only after `dir` points at the new fd (never use a closed fd).
        let old = owned.replace(unsafe { std::fs::File::from_raw_fd(fd) });
        dir = fd;
        drop(old);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// A host filesystem that answers from what a test declared.
    #[derive(Default)]
    struct FakeFs {
        entries: HashMap<PathBuf, Stat>,
        canon: HashMap<PathBuf, PathBuf>,
        readable: HashSet<PathBuf>,
    }

    impl FakeFs {
        fn dir(mut self, p: &str, mode: u32) -> Self {
            self.entries.insert(
                PathBuf::from(p),
                Stat {
                    is_dir: true,
                    is_file: false,
                    mode,
                },
            );
            self
        }
        fn file(mut self, p: &str, readable: bool) -> Self {
            self.entries.insert(
                PathBuf::from(p),
                Stat {
                    is_dir: false,
                    is_file: true,
                    mode: 0o100644,
                },
            );
            if readable {
                self.readable.insert(PathBuf::from(p));
            } else {
                self.readable.remove(Path::new(p));
            }
            self
        }
        fn link(mut self, p: &str, to: &str) -> Self {
            self.canon.insert(PathBuf::from(p), PathBuf::from(to));
            self
        }
    }

    impl HostFs for FakeFs {
        fn stat(&self, path: &Path) -> Option<Stat> {
            self.entries.get(path).copied()
        }
        fn canonical(&self, path: &Path) -> PathBuf {
            self.canon
                .get(path)
                .cloned()
                .unwrap_or_else(|| path.to_path_buf())
        }
        fn reads_one_byte(&self, path: &Path) -> bool {
            self.readable.contains(path)
        }
    }

    /// The device's answer, as the reference measured it: six world-searchable
    /// directories and two readable linker configs; the two property-context
    /// files are absent.
    fn device_fs() -> FakeFs {
        FakeFs::default()
            .dir("/apex", 0o40755)
            .dir("/odm", 0o40755)
            .dir("/product", 0o40755)
            .dir("/system", 0o40755)
            .dir("/system_ext", 0o40755)
            .dir("/vendor", 0o40755)
            .file("/linkerconfig/ld.config.txt", true)
            .file("/linkerconfig/com.android.art/ld.config.txt", true)
    }

    #[test]
    fn a_normal_rootfs_gets_the_filtered_android_binds() {
        let plan = plan(&device_fs(), DistType::Normal, false);
        assert_eq!(
            plan.args,
            vec![
                "/apex",
                "/odm",
                "/product",
                "/system",
                "/system_ext",
                "/vendor",
                "/linkerconfig/ld.config.txt",
                "/linkerconfig/com.android.art/ld.config.txt",
            ],
            "a normal session gets exactly the reachable Android paths"
        );
        assert!(
            plan.skipped.is_empty(),
            "nothing withheld when every candidate qualifies: {:?}",
            plan.skipped
        );
    }

    #[test]
    fn a_missing_path_is_dropped_silently() {
        // device_fs() has neither property-context file: both are absent.
        let fs = device_fs();
        let plan = plan(&fs, DistType::Normal, false);
        assert!(
            !plan.args.iter().any(|a| a.contains("property_contexts")),
            "an absent path is not bound: {:?}",
            plan.args
        );
        assert!(
            plan.skipped.is_empty(),
            "and it is dropped without a note, as the reference does: {:?}",
            plan.skipped
        );
    }

    #[test]
    fn a_directory_without_the_world_execute_digit_is_skipped() {
        // mode 0700: last digit 0, not 1/5/7. A guest could not traverse it.
        let fs = device_fs().dir("/apex", 0o40700);
        let plan = plan(&fs, DistType::Normal, false);
        assert!(
            !plan.args.iter().any(|a| a == "/apex"),
            "/apex must not be bound: {:?}",
            plan.args
        );
        assert_eq!(
            plan.skipped,
            vec![
                "android bind /apex skipped: mode 0700 is not world-searchable \
                 (last digit 0, need 1, 5 or 7)"
                    .to_string()
            ],
            "the mode-digit clause emits exactly this"
        );
    }

    #[test]
    fn every_world_searchable_digit_is_accepted() {
        for digit in [1u32, 5, 7] {
            let fs = device_fs().dir("/apex", 0o40700 | digit);
            let plan = plan(&fs, DistType::Normal, false);
            assert!(
                plan.args.iter().any(|a| a == "/apex"),
                "mode digit {digit} is world-searchable",
            );
        }
    }

    #[test]
    fn a_file_whose_read_fails_is_skipped() {
        let fs = device_fs().file("/linkerconfig/ld.config.txt", false);
        let plan = plan(&fs, DistType::Normal, false);
        assert!(
            !plan.args.iter().any(|a| a == "/linkerconfig/ld.config.txt"),
            "an unreadable file is not bound: {:?}",
            plan.args
        );
        assert_eq!(
            plan.skipped,
            vec![
                "android bind /linkerconfig/ld.config.txt skipped: the 1-byte read failed"
                    .to_string()
            ],
            "the readability clause emits exactly this"
        );
    }

    #[test]
    fn a_termux_type_rootfs_gets_no_android_binds() {
        let plan = plan(&device_fs(), DistType::Termux, false);
        assert!(plan.args.is_empty(), "no binds for a termux-type guest");
        assert_eq!(
            plan.skipped,
            vec![SKIP_TERMUX.to_string()],
            "the exclusion is stated once, with its reason"
        );
    }

    #[test]
    fn an_isolated_session_gets_no_android_binds() {
        let plan = plan(&device_fs(), DistType::Normal, true);
        assert!(plan.args.is_empty(), "no binds when isolated");
        assert_eq!(plan.skipped, vec![SKIP_ISOLATED.to_string()]);
    }

    #[test]
    fn the_bound_path_is_the_resolved_one() {
        // /system is a symlink on Android; the reference binds realpath().
        let fs = device_fs()
            .link("/system", "/real/system")
            .dir("/real/system", 0o40755);
        let plan = plan(&fs, DistType::Normal, false);
        assert!(
            plan.args.iter().any(|a| a == "/real/system"),
            "the resolved path is bound: {:?}",
            plan.args
        );
        assert!(!plan.args.iter().any(|a| a == "/system"));
    }

    // ---- detect_dist_type: the guest walk ----

    fn scratch(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("pdn-android-binds-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_rootfs_without_the_termux_login_is_normal() {
        let r = scratch("normal");
        assert_eq!(detect_dist_type(&r), DistType::Normal);
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn a_rootfs_that_carries_the_termux_login_is_termux_type() {
        let r = scratch("termux");
        let bin = r.join("data/data/com.termux/files/usr/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("login"), b"x").unwrap();
        assert_eq!(detect_dist_type(&r), DistType::Termux);
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn a_planted_data_symlink_does_not_escape_to_the_host() {
        // The host really has /data/data/com.termux/files/usr/bin/login (this is
        // a Termux device). A composed-path check would follow this symlink and
        // call the rootfs termux-type; the guest walk must not.
        let r = scratch("escape");
        std::os::unix::fs::symlink("/", r.join("data")).unwrap();
        assert_eq!(
            detect_dist_type(&r),
            DistType::Normal,
            "a guest-planted `data -> /` must not reach the host's own Termux login"
        );
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn a_directory_named_login_is_not_a_login_binary() {
        // The reference checks for a *file*: a concurrent proot session can leave
        // a bind target materialised as a directory, which must not misidentify.
        let r = scratch("dirlogin");
        let bin = r.join("data/data/com.termux/files/usr/bin");
        std::fs::create_dir_all(bin.join("login")).unwrap();
        assert_eq!(detect_dist_type(&r), DistType::Normal);
        let _ = std::fs::remove_dir_all(&r);
    }
}
