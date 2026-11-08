//! p4 step 3a: the engine spawns and traces its own tracee, and says what it cannot do yet.
//!
//! Written 2026-11-08 by the parent session together with `src/native_run.rs`. The file holds two
//! tests on purpose:
//!
//! * `the_engine_owns_the_tracee_and_records_named_refusals` - the capability that is PROVEN today
//!   and therefore gated: no external `proot` binary is involved, syscall stops are traced, every
//!   syscall the engine does not handle is recorded with a name AND a reason, `strict` refuses by
//!   name, and the binary the kernel executed is reported from `/proc/<pid>/exe`.
//!
//! * `the_engine_translates_execve_into_the_given_rootfs` - the translation the native engine
//!   exists to do: the child's `execve` path is rewritten to `<rootfs>/<path>` BEFORE the kernel
//!   reads it, proven by `/proc/<pid>/exe` naming the fixture's copy. Run it with `--nocapture` to
//!   see the measurements (`[arm-a-observed]`, `[arm-a-child-view]`, ...).
//!
//! MEASURED BLOCKER, and its fix (2026-11-08, PDN master 19df16b + this module):
//! the engine's trace used to begin AFTER an `execve` it never saw: the first entry stops were the
//! STARTUP of an already-exec'd program (`faccessat`, `openat`, `read`), so the path was never
//! rewritten and arm A ran the HOST's `/bin/true` and exited 0 - the exact false positive this test
//! exists to catch. Two independent causes, both fixed in `native_run.rs`:
//!   (a) `PTRACE_TRACEME`'s initial stop is delivered at the child's `execve`; the child now blocks
//!       in a `read` on a pipe while the engine `PTRACE_SEIZE`s + `PTRACE_INTERRUPT`s it, so the
//!       trace is live before the exec.
//!   (b) a preloaded library (`libtermux-exec`) intercepted `libc::execve`, so the child's exec
//!       never surfaced as a syscall stop; the child now issues `execve` as a raw syscall.
//! With both fixed the first entry stops are the child's OWN `close`/`read`/`close`, then its raw
//! `getpid` marker, then its `execve` - the assertion below pins exactly that.
//!
//! What the two fixtures make impossible: an engine that "runs `/bin/true` inside a rootfs" while
//! executing the host's `/bin/true` would look identical from an exit code of 0, so `rootfs-a/bin/true`
//! is a copy of `false` (exits 1) and `rootfs-b/bin/true` is a copy of `true` (exits 0): the same
//! guest path must give different results, decided by the rootfs, and `/proc/<pid>/exe` must name
//! the fixture's copy.

use proot_core::native_run::{run_native, RunPlan};
use std::path::{Path, PathBuf};

/// A host program to copy into a fixture, preferring Termux's prefix layout and falling back to the
/// conventional `/usr/bin` used by the Linux CI runner.
fn host_program(name: &str) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(prefix) = std::env::var("PREFIX") {
        candidates.push(Path::new(&prefix).join("bin").join(name));
    }
    candidates.push(PathBuf::from("/usr/bin").join(name));
    candidates.push(PathBuf::from("/bin").join(name));
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    panic!(
        "no host program named {:?} (looked in {:?})",
        name, candidates
    );
}

/// A per-test fixture directory. The tag is per test because cargo runs the two tests of this file
/// CONCURRENTLY in one process: with a shared directory their `remove_dir_all` calls raced (measured
/// 2026-11-08 - one test's cleanup deleted the other's fixture mid-build), so each test gets its own.
fn fixture_root(tag: &str) -> PathBuf {
    // $PREFIX/tmp on Termux: /tmp is not writable for an Android app uid.
    let base = match std::env::var("PREFIX") {
        Ok(p) => PathBuf::from(p).join("tmp"),
        Err(_) => std::env::temp_dir(),
    };
    base.join(format!("p4-3a-run-{}-{}", std::process::id(), tag))
}

/// Build `rootfs-<name>/bin/true` as the named host program, or an empty `bin` when `host` is None.
///
/// The fixture must be a program whose EXIT STATUS is decided by its content, not by the `argv[0]`
/// the engine hands it. Host `false` and `true` are normally two distinct programs, so a plain
/// `std::fs::copy` is a faithful fixture. On Termux they are two symlinks to ONE `coreutils`
/// MULTICALL that chooses its applet from `argv[0]` (measured 2026-11-08: `exec -a /bin/true
/// <copy-of-false>` runs the `true` applet and exits 0), so two copies would be the SAME program
/// and arms A and B below would prove nothing. When the two host applets resolve to one file, the
/// fixture is compiled instead, so the arms really are different programs (exit 1 vs 0) for any
/// `argv[0]`.
fn make_rootfs(root: &Path, name: &str, host: Option<&str>) -> PathBuf {
    let rootfs = root.join(format!("rootfs-{}", name));
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    if let Some(prog) = host {
        let src = host_program(prog);
        let dst = rootfs.join("bin").join("true");
        std::fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("copy {} -> {}: {}", src.display(), dst.display(), e));
        let exit_code = if prog == "false" { 1 } else { 0 };
        let compiled = canon(&host_program("false")) == canon(&host_program("true"));
        if compiled {
            // The host applets are one multicall binary: a copy cannot carry WHICH one it is, so
            // build a fixture that returns `exit_code` whatever `argv[0]` says.
            compile_exit_fixture(&dst, exit_code);
        }
        // The fixture must be executable. Assert it rather than assuming it: a non-executable
        // fixture would fail with EACCES and look like an engine bug.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dst)
                .expect("stat fixture")
                .permissions()
                .mode();
            assert!(
                mode & 0o111 != 0,
                "fixture {} is not executable (mode {:o})",
                dst.display(),
                mode
            );
        }
        eprintln!(
            "[fixture-{}] /bin/true = {} of {} ({})",
            name,
            if compiled { "compiled" } else { "copy" },
            src.display(),
            dst.display()
        );
    }
    rootfs
}

/// Overwrite `dst` with a minimal program that returns `code` from `main`, so its exit status is
/// independent of `argv[0]`. Used only when the host `false`/`true` cannot tell the two arms apart
/// (see `make_rootfs`). `$CC` selects the compiler, defaulting to `cc`.
fn compile_exit_fixture(dst: &Path, code: i32) {
    let src = dst.with_extension("c");
    std::fs::write(&src, format!("int main(void) {{ return {}; }}\n", code))
        .unwrap_or_else(|e| panic!("write fixture source {}: {}", src.display(), e));
    let cc = std::env::var("CC").unwrap_or_else(|_| {
        // The test process may not have `$PREFIX/bin` on PATH; prefer the absolute path when the
        // prefix is known, falling back to a bare `cc`.
        match std::env::var("PREFIX") {
            Ok(p) => {
                let abs = Path::new(&p).join("bin").join("cc");
                if abs.exists() {
                    abs.to_string_lossy().into_owned()
                } else {
                    "cc".to_string()
                }
            }
            Err(_) => "cc".to_string(),
        }
    });
    let out = std::process::Command::new(&cc)
        .arg(&src)
        .arg("-o")
        .arg(dst)
        .output()
        .unwrap_or_else(|e| panic!("run {} to build the fixture: {}", cc, e));
    assert!(
        out.status.success(),
        "{} failed to build the fixture {}: {}",
        cc,
        dst.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_file(&src);
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The proven half: the engine owns its tracee and reports honestly about what it does not handle.
#[test]
fn the_engine_owns_the_tracee_and_records_named_refusals() {
    let root = fixture_root("owns");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs_a = make_rootfs(&root, "a", Some("false"));
    let argv = vec!["/bin/true".to_string()];
    let plan = RunPlan {
        rootfs: Some(rootfs_a),
        binds: Vec::new(),
    };

    let run = run_native(&argv, &plan, false).expect("traced run of the fixture");
    eprintln!(
        "[proven] exit={} stops={} entries={} executed={:?} child_view={:?} observed[..4]={:?}",
        run.exit_code,
        run.stops,
        run.entries,
        run.executed,
        run.child_view,
        &run.observed[..run.observed.len().min(4)]
    );

    // No external proot binary is involved: the engine forked and owned this tracee itself, and the
    // child published its own view of the shared page (which is why the engine never has to assume
    // that its own address for that page is the child's).
    assert!(
        run.child_view.is_some(),
        "the child must publish its self-report before stopping; without it the engine cannot know \
         where the path it may rewrite lives"
    );
    let view = run.child_view.as_ref().unwrap();
    assert_eq!(
        view.path_len,
        argv[0].len(),
        "the child reports the path it prepared"
    );

    // Real syscalls were traced, and every one the engine does not handle carries a name or a
    // number plus a reason - never a bare count.
    assert!(
        run.entries > 0 && !run.refusals.is_empty(),
        "no syscalls were traced"
    );
    assert!(
        run.refusals.iter().all(|r| r.label().contains('[')),
        "every refusal must state a reason, got {:?}",
        run.refusal_summary()
    );
    assert!(
        run.refusals.iter().any(|r| r.name.is_some()),
        "at least one refusal should be nameable from the engine's table, got {:?}",
        run.refusal_summary()
    );

    // Which binary the kernel executed is REPORTED, not judged here: the translation itself is
    // judged by `the_engine_translates_execve_into_the_given_rootfs` below.
    let executed = run
        .executed
        .as_ref()
        .expect("an exec event must be reported");
    eprintln!("[proven] executed={}", executed.display());

    // `strict` must refuse by name rather than proceed as if the syscall were handled.
    let strict = run_native(&argv, &plan, true).expect("strict run returns a report, not a crash");
    let failure = strict
        .strict_failure
        .expect("strict mode must name the syscall it refused");
    eprintln!("[proven] strict_failure={}", failure);
    assert!(
        failure.contains("(") || failure.contains("number "),
        "the strict failure must carry a syscall name and number, got: {}",
        failure
    );
    assert!(
        strict.translations.is_empty(),
        "a run stopped in strict mode must not report a translation as if it had proceeded"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The translation half: the child's `execve` path is rewritten into the given rootfs.
#[test]
fn the_engine_translates_execve_into_the_given_rootfs() {
    let root = fixture_root("translate");
    let _ = std::fs::remove_dir_all(&root);
    // rootfs-a and rootfs-b hold DIFFERENT programs under the same guest path.
    let rootfs_a = make_rootfs(&root, "a", Some("false"));
    let rootfs_b = make_rootfs(&root, "b", Some("true"));
    let rootfs_c = make_rootfs(&root, "c", None);

    let argv = vec!["/bin/true".to_string()];

    // ---- arm A: the guest's /bin/true is the fixture's copy, which exits 1 -------------------
    let plan_a = RunPlan {
        rootfs: Some(rootfs_a.clone()),
        binds: Vec::new(),
    };
    let a = run_native(&argv, &plan_a, false).expect("run against rootfs-a");
    eprintln!(
        "[arm-a] exit={} stops={} entries={} translations={:?} executed={:?} refusals={} distinct={:?}",
        a.exit_code,
        a.stops,
        a.entries,
        a.translations,
        a.executed,
        a.refusals.len(),
        a.refusal_summary().into_iter().take(6).collect::<Vec<_>>()
    );
    // What the engine actually saw, in order. Printed unconditionally: when an arm fails, this is
    // the line that says whether the engine saw the wrong syscall, the wrong argument, or nothing
    // at all - and a report that cannot answer that is not evidence.
    eprintln!("[arm-a-observed] {:?}", a.observed);
    eprintln!("[arm-a-child-view] {:?}", a.child_view);

    // The sync gate. The engine released the child from its opening read() and the trace must begin
    // with the child's OWN pre-exec syscalls, not the startup of a program whose execve the engine
    // never saw. The child's release read (SYS_read), its close of the read end (SYS_close) and its
    // raw getpid marker (SYS_getpid) must be the entry stops IMMEDIATELY before its execve. Adjusted
    // 2026-11-08 from the previous exact `[-1, SYS_getpid, SYS_execve]` prefix: the invalid-syscall
    // marker was dropped (on this kernel an invalid syscall raises SIGSYS, and the signal-delivery
    // stop it leaves breaks the following trace), and the release read/close are now part of the
    // child's own pre-exec sequence. When this fails, the trace did not start where the engine
    // thinks it did and no other assertion here means anything.
    let nums: Vec<i64> = a.observed.iter().map(|(n, _)| *n).collect();
    let exec_at = nums
        .iter()
        .position(|n| *n == nix::libc::SYS_execve as i64)
        .unwrap_or_else(|| {
            panic!(
                "the child's own execve entry stop must be the first SYS_execve seen (SYS_execve={}), got {:?}",
                nix::libc::SYS_execve, a.observed
            )
        });
    assert!(
        exec_at >= 3,
        "the engine's trace must begin with the child's own syscalls before it reaches execve, got {:?}",
        a.observed
    );
    assert_eq!(
        nums[exec_at - 1],
        nix::libc::SYS_getpid as i64,
        "the entry stop immediately before the translated execve must be the child's getpid marker \
         (SYS_getpid={}), got {:?}",
        nix::libc::SYS_getpid,
        a.observed
    );
    assert_eq!(
        nums[exec_at - 2],
        nix::libc::SYS_close as i64,
        "the child's close of its read end must be the entry stop before its marker (SYS_close={}), \
         got {:?}",
        nix::libc::SYS_close,
        a.observed
    );
    assert_eq!(
        nums[exec_at - 3],
        nix::libc::SYS_read as i64,
        "the child's release read() must be the entry stop before its close (SYS_read={}) - if the \
         first entry stops are a dynamic loader's (faccessat/openat), the trace began after an \
         execve the engine never saw. got {:?}",
        nix::libc::SYS_read,
        a.observed
    );
    assert_eq!(
        a.translations.len(),
        1,
        "exactly one execve path should have been translated, got {:?}",
        a.translations
    );
    assert_eq!(a.translations[0].guest, "/bin/true");
    assert_eq!(a.translations[0].syscall, "SYS_execve");
    assert_eq!(
        canon(&a.translations[0].host),
        canon(&rootfs_a.join("bin").join("true")),
        "the translation must land inside the requested rootfs"
    );
    assert_eq!(
        a.exit_code, 1,
        "arm A ran a copy of `false`, so a correct run exits 1; 0 would mean the HOST's /bin/true \
         ran and the translation was decorative"
    );
    match a.executed.as_ref() {
        Some(p) => assert_eq!(
            canon(p),
            canon(&rootfs_a.join("bin").join("true")),
            "the kernel executed {} which is not the fixture's binary",
            p.display()
        ),
        None => eprintln!(
            "[arm-a] /proc/<pid>/exe was not readable; the exit code (1, not 0) carries the verdict"
        ),
    }

    // ---- arm B: same guest path, other rootfs, other program ---------------------------------
    let plan_b = RunPlan {
        rootfs: Some(rootfs_b.clone()),
        binds: Vec::new(),
    };
    let b = run_native(&argv, &plan_b, false).expect("run against rootfs-b");
    eprintln!(
        "[arm-b] exit={} translations={:?} executed={:?}",
        b.exit_code, b.translations, b.executed
    );
    assert_eq!(
        b.exit_code, 0,
        "arm B ran a copy of `true`; a non-zero exit means the rootfs was not honoured"
    );
    assert_eq!(b.translations.len(), 1);
    assert_eq!(
        canon(&b.translations[0].host),
        canon(&rootfs_b.join("bin").join("true"))
    );
    assert_ne!(
        canon(&a.translations[0].host),
        canon(&b.translations[0].host),
        "the two arms must have run different files for this test to prove anything"
    );

    // ---- control C: no such file in the rootfs must fail LOUDLY ------------------------------
    let plan_c = RunPlan {
        rootfs: Some(rootfs_c),
        binds: Vec::new(),
    };
    let c = run_native(&argv, &plan_c, false);
    let err =
        c.expect_err("an absent guest path must be refused, not silently resolved on the host");
    let msg = format!("{:#}", err);
    eprintln!("[control-c] {}", msg);
    assert!(
        msg.contains("does not exist") && msg.contains("/bin/true"),
        "the refusal must name the guest path and say it does not exist, got: {}",
        msg
    );

    // ---- control D: no plan means no translation, so the guest path is used verbatim ----------
    let absent = vec!["/p4-3a-absent/true".to_string()];
    let d = run_native(&absent, &RunPlan::default(), false).expect("run with no plan");
    eprintln!(
        "[control-d] exit={} translations={:?} executed={:?}",
        d.exit_code, d.translations, d.executed
    );
    assert!(
        d.translations.is_empty(),
        "with no plan the engine must not claim to have translated anything"
    );
    assert_eq!(
        d.exit_code, 127,
        "with no plan the kernel is handed the guest path as-is, and execve fails with 127 - this \
         is what makes the translation in arm A load-bearing"
    );

    let _ = std::fs::remove_dir_all(&root);
}
