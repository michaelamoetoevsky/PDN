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

/// The host runtime trees a DYNAMICALLY LINKED fixture's loader probes for its libraries, bound to
/// themselves.
///
/// Why these tests need it (2026-11-08, p7): the engine now translates the loader's own
/// path-taking syscalls at entry, so a rootfs carrying only `bin/true` no longer borrows the
/// host's libc by accident - the loader's `openat` is rewritten into the rootfs and answers
/// ENOENT, the loader aborts, and the run exits 1. That is the CORRECT container behaviour, and
/// `native_dynamic_guest.rs` proves it with a fixture that links a library existing only inside
/// the rootfs. These four tests are about `execve` translation, not about library loading, so they
/// bind the host's runtime trees over themselves: the loader finds the libraries it found before.
/// A bind mounted on itself is a rewrite that changes nothing, so the engine does not record it -
/// which is what keeps `translations` here holding exactly the `execve` rewrite under test.
fn host_runtime_binds() -> Vec<(String, PathBuf)> {
    [
        "/system",
        "/odm",
        "/vendor",
        "/lib",
        "/lib64",
        "/usr/lib",
        "/usr/lib64",
        "/proc",
        "/dev",
    ]
    .iter()
    .map(PathBuf::from)
    .filter(|p| p.exists())
    .map(|p| (p.to_string_lossy().into_owned(), p))
    .collect()
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
        binds: host_runtime_binds(),
        cwd: None,
        env: Vec::new(),
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
        binds: host_runtime_binds(),
        cwd: None,
        env: Vec::new(),
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
    // Only the `execve` rewrite is this arm's subject. The loader's own opens are translated too
    // now (see `native_dynamic_guest.rs`), and those this plan binds to themselves the engine does
    // not record at all, so filtering keeps the count meaningful without weakening it.
    let a_execve: Vec<_> = a
        .translations
        .iter()
        .filter(|t| t.syscall == "SYS_execve")
        .collect();
    assert_eq!(
        a_execve.len(),
        1,
        "exactly one execve path should have been translated, got {:?}",
        a.translations
    );
    assert_eq!(a_execve[0].guest, "/bin/true");
    assert_eq!(a_execve[0].syscall, "SYS_execve");
    assert_eq!(
        canon(&a_execve[0].host),
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
        binds: host_runtime_binds(),
        cwd: None,
        env: Vec::new(),
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
    let b_execve: Vec<_> = b
        .translations
        .iter()
        .filter(|t| t.syscall == "SYS_execve")
        .collect();
    assert_eq!(b_execve.len(), 1);
    assert_eq!(
        canon(&b_execve[0].host),
        canon(&rootfs_b.join("bin").join("true"))
    );
    assert_ne!(
        canon(&a_execve[0].host),
        canon(&b_execve[0].host),
        "the two arms must have run different files for this test to prove anything"
    );

    // ---- control C: no such file in the rootfs must fail LOUDLY ------------------------------
    let plan_c = RunPlan {
        rootfs: Some(rootfs_c),
        binds: Vec::new(),
        cwd: None,
        env: Vec::new(),
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

// ---------------------------------------------------------------------------------------------
// p4 step 4: fork tracing and bind mounts (added 2026-11-08). These two tests are new; the two
// tests above are UNCHANGED. Each has a negative control (sabotage the feature, watch the test go
// red for a behaviour reason) documented in the module report, not in code.
// ---------------------------------------------------------------------------------------------

/// Resolve the C compiler the same way `compile_exit_fixture` does, so a fixture build is never
/// silently skipped.
fn compiler() -> String {
    std::env::var("CC").unwrap_or_else(|_| match std::env::var("PREFIX") {
        Ok(p) => {
            let abs = Path::new(&p).join("bin").join("cc");
            if abs.exists() {
                abs.to_string_lossy().into_owned()
            } else {
                "cc".to_string()
            }
        }
        Err(_) => "cc".to_string(),
    })
}

/// Compile `source` into `dst`, or fail LOUDLY. A missing compiler is a RED test, never a silent
/// skip: a skipped fixture build would let the test pass having proven nothing.
fn compile_c(dst: &Path, source: &str) {
    let src = dst.with_extension("c");
    std::fs::write(&src, source)
        .unwrap_or_else(|e| panic!("write fixture source {}: {}", src.display(), e));
    let cc = compiler();
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

/// A fixture that forks and whose CHILD execs `child_path` (a guest path); the parent waits and
/// returns the child's exit status, so the run's exit code IS the exec'd program's. The child is
/// compiled so its exec is a real `execve`, and it is run with the engine's empty environment (no
/// `LD_PRELOAD`), so the syscall reaches the tracer.
fn make_forker(dst: &Path, child_path: &str) {
    let source = format!(
        r#"#include <unistd.h>
#include <sys/wait.h>
int main(void) {{
    pid_t p = fork();
    if (p == 0) {{
        char *const a[] = {{ (char *)"{0}", 0 }};
        char *const e[] = {{ 0 }};
        execve("{0}", a, e);
        _exit(90);
    }}
    if (p < 0) {{ return 91; }}
    int st = 0;
    if (waitpid(p, &st, 0) < 0) {{ return 93; }}
    if (WIFEXITED(st)) {{ return WEXITSTATUS(st); }}
    if (WIFSIGNALED(st)) {{ return 128 + WTERMSIG(st); }}
    return 92;
}}
"#,
        child_path
    );
    compile_c(dst, &source);
}

/// A forked grandchild is traced at all: its own `execve` is translated into the rootfs. Without
/// fork tracing (`TRACEFORK`/`VFORK`/`CLONE`) the grandchild's execve is never seen, so no
/// translation names `/bin/child` and the guest path it execs does not exist on the host (the
/// forker then exits 90).
#[test]
fn the_engine_traces_a_forked_child_and_translates_its_execve() {
    let root = fixture_root("fork");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-fork");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    make_forker(&rootfs.join("bin").join("forker"), "/bin/child");
    compile_exit_fixture(&rootfs.join("bin").join("child"), 1);

    let argv = vec!["/bin/forker".to_string()];
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds: host_runtime_binds(),
        cwd: None,
        env: Vec::new(),
    };
    let run = run_native(&argv, &plan, false).expect("traced run of the forking fixture");
    eprintln!(
        "[fork] exit={} translations={:?} executed={:?} stops={}",
        run.exit_code, run.translations, run.executed, run.stops
    );

    assert_eq!(
        run.exit_code, 1,
        "the forked child exec'd the rootfs /bin/child (exit 1), which the forker propagated; 90 \
         means the child's execve was never translated and failed, 127/1 on the host would mean the \
         engine ran a host binary"
    );
    let t = run
        .translations
        .iter()
        .find(|t| t.guest == "/bin/child")
        .unwrap_or_else(|| {
            panic!(
                "the grandchild's execve must be translated into the rootfs; translations={:?}",
                run.translations
            )
        });
    assert_eq!(
        canon(&t.host),
        canon(&rootfs.join("bin").join("child")),
        "the grandchild's translation must land inside the requested rootfs"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A bind mount wins over the rootfs, for a fork descendant's `execve`: `/opt/tool` exists ONLY in
/// the bind target, and the engine's report names the bind host path it executed. With binds
/// ignored, `resolve` falls to `<rootfs>/opt/tool`, which does not exist, so the run is refused.
#[test]
fn a_bind_prefix_wins_over_the_rootfs_for_execve() {
    let root = fixture_root("bind");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-bind");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let bind_dir = root.join("bind-target");
    std::fs::create_dir_all(&bind_dir).expect("create bind target");
    // /opt exists NOWHERE in the rootfs; the guest path is supplied entirely by the bind.
    make_forker(&rootfs.join("bin").join("forker"), "/opt/tool");
    let bind_tool = bind_dir.join("tool");
    compile_exit_fixture(&bind_tool, 7);

    let argv = vec!["/bin/forker".to_string()];
    let mut binds = vec![("/opt".to_string(), bind_dir.clone())];
    // The fixture is dynamic too, so its loader needs the host runtime trees visible for the same
    // reason as in the tests above.
    binds.extend(host_runtime_binds());
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: None,
        env: Vec::new(),
    };
    let run = run_native(&argv, &plan, false).unwrap_or_else(|e| {
        panic!("the bind must supply the guest path, but the engine refused the run: {e:#}")
    });
    eprintln!(
        "[bind] exit={} translations={:?} executed={:?}",
        run.exit_code, run.translations, run.executed
    );

    assert_eq!(
        run.exit_code, 7,
        "the bind-target binary exits 7 and the forker propagated it; a refusal or another code \
         means the bind did not supply the guest path"
    );
    let t = run
        .translations
        .iter()
        .find(|t| t.guest == "/opt/tool")
        .unwrap_or_else(|| {
            panic!(
                "the bind path must be translated, not left to the host; translations={:?}",
                run.translations
            )
        });
    assert_eq!(
        canon(&t.host),
        canon(&bind_tool),
        "the bind target must win over the rootfs"
    );
    match run.executed.as_ref() {
        Some(p) => assert_eq!(
            canon(p),
            canon(&bind_tool),
            "the kernel executed {} which is not the bind target",
            p.display()
        ),
        None => eprintln!(
            "[bind] /proc/<pid>/exe was not readable; the exit code (7) and the translation carry \
             the verdict"
        ),
    }

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------------------------
// p7 cwd: the guest's WORKING DIRECTORY (added 2026-11-09). Closes the FIRST of the three named
// engine-parity gaps.
// ---------------------------------------------------------------------------------------------

/// The guest's starting working directory matches the proot engine's.
///
/// Measured before this increment (`scripts/engine_differential.py`, PDN 36ff688): under `--engine
/// proot` (the default) the guest sees cwd `/`, while under `--engine native` it inherits the HOST
/// process's cwd, because `RunPlan` had no cwd field and nothing was ever `chdir`'d in the child.
/// The proot path sets the guest cwd from `container.config.working_dir.unwrap_or("/")`
/// (`crates/proot-distro-core/src/container.rs:234-240`, `runtime.rs:152-158`); this proves the
/// native engine now does the same.
///
/// The guest is a real compiled program whose only job is to report its OWN cwd: it `getcwd`s and
/// writes the result to `/probe/cwd`, a guest path the plan binds to a host directory this test
/// reads back. The positive arm sets `cwd: Some("/")` and must land the guest in
/// `resolve(rootfs, "/")` - the rootfs root. The NEGATIVE CONTROL runs the SAME guest with the SAME
/// binds and ONLY `cwd: None`: it must inherit this test process's own cwd instead, which is what
/// proves the `chdir` (not the environment) produces the difference. A third arm proves a cwd that
/// would escape the rootfs (`..`) and a relative cwd are refused BY NAME.
#[test]
fn the_engine_chdirs_the_guest_into_the_plans_cwd() {
    let root = fixture_root("cwd");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-cwd");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");

    // The host directory the guest's report lands in, bound at `/probe` so the guest's `open` is
    // translated HERE instead of into the rootfs.
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    // The guest: getcwd() into a buffer, then write it to /probe/cwd.
    compile_c(
        &rootfs.join("bin").join("cwd"),
        r#"#include <unistd.h>
#include <fcntl.h>
#include <string.h>
int main(void) {
    char buf[4096];
    if (getcwd(buf, sizeof buf) == 0) { return 60; }
    int fd = open("/probe/cwd", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { return 61; }
    if (write(fd, buf, strlen(buf)) < 0) { return 62; }
    close(fd);
    return 0;
}
"#,
    );

    let argv = vec!["/bin/cwd".to_string()];
    let mut binds = vec![("/probe".to_string(), probe.clone())];
    binds.extend(host_runtime_binds());

    // Read back what the guest wrote, naming the tag so a missing report is legible in the failure.
    let read_report = |tag: &str| -> String {
        std::fs::read_to_string(probe.join("cwd"))
            .unwrap_or_else(|e| panic!("[{tag}] the guest did not report its cwd: {e}"))
    };

    // ---- positive arm: cwd: Some("/") lands the guest in the rootfs root ----------------------
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds: binds.clone(),
        cwd: Some("/".to_string()),
        env: Vec::new(),
    };
    let run = run_native(&argv, &plan, false).expect("traced run with a cwd");
    let guest_cwd = read_report("some");
    eprintln!(
        "[cwd] cwd=Some(\"/\") exit={} guest_getcwd={} expected_rootfs={}",
        run.exit_code,
        guest_cwd,
        rootfs.display()
    );
    assert_eq!(
        run.exit_code, 0,
        "the fixture must run to completion; a 126 means the engine's own chdir failed"
    );
    assert_eq!(
        canon(Path::new(&guest_cwd)),
        canon(&rootfs),
        "with cwd: Some(\"/\") the guest's own getcwd() must be the translated host path of \"/\" \
         (the rootfs root); got {} - a HOST cwd means the child was never chdir'd",
        guest_cwd
    );
    // The engine's OWN pre-exec chdir is setup on an already-translated host path, NOT a guest path:
    // it must not be recorded as an untranslated-path refusal, or `native_verdict` (podnext) would
    // fail every `--engine native` run on the engine's own bootstrap.
    assert!(
        run.refusals.iter().all(|r| r.name != Some("SYS_chdir")),
        "the engine's own pre-exec chdir must not be recorded as a refusal; got {:?}",
        run.refusal_summary()
    );

    // ---- negative control: cwd: None must NOT put the guest at the rootfs ---------------------
    // SAME guest, SAME binds; the ONLY difference is the cwd field. If this arm also landed at the
    // rootfs root, the positive assertion above would prove nothing about the chdir.
    let host_cwd = std::env::current_dir().expect("this test's own cwd");
    let plan_none = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: None,
        env: Vec::new(),
    };
    let run_none = run_native(&argv, &plan_none, false).expect("traced run without a cwd");
    let guest_cwd_none = read_report("none");
    eprintln!(
        "[cwd] cwd=None       exit={} guest_getcwd={} host_cwd={}",
        run_none.exit_code,
        guest_cwd_none,
        host_cwd.display()
    );
    assert_eq!(run_none.exit_code, 0, "the fixture must run to completion");
    assert_ne!(
        canon(Path::new(&guest_cwd_none)),
        canon(&rootfs),
        "NEGATIVE CONTROL: with cwd: None the guest must inherit the HOST cwd, never end up at the \
         rootfs - if it does, the chdir is not what produced the positive arm's result"
    );
    assert_eq!(
        canon(Path::new(&guest_cwd_none)),
        canon(&host_cwd),
        "with cwd: None the guest must inherit this process's own working directory"
    );

    // ---- refusal arm: a cwd that escapes the rootfs, and a relative cwd, are named refusals ----
    for bad in ["/../etc", "relative/dir"] {
        let plan_bad = RunPlan {
            rootfs: Some(rootfs.clone()),
            binds: Vec::new(),
            cwd: Some(bad.to_string()),
            env: Vec::new(),
        };
        let err = run_native(&argv, &plan_bad, false)
            .expect_err("a cwd that is not a plain absolute under-rootfs path must be refused");
        let msg = format!("{err:#}");
        eprintln!("[cwd] refusal for {bad:?}: {msg}");
        assert!(
            msg.contains("cwd"),
            "the refusal must name the cwd field, got: {msg}"
        );
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// A RELATIVE guest path is translatable ONCE the engine knows the guest's starting directory
/// (2026-11-09, p7 relative). Regression guard for a defect this engine's own differential harness
/// found: after `RunPlan::cwd` was introduced, a relative open still produced an untranslated-path
/// refusal, and podnext turns that refusal into a failed run even though the write landed correctly
/// inside the rootfs (measured: `exit 1` on three consecutive runs with the file present).
///
/// The guest here starts in `/probe` (a bind to a HOST directory) and writes a report with a RELATIVE
/// name. Three arms, same guest, same binds - only the plan differs:
///   * `cwd: Some("/probe")`  -> the relative name resolves through the cwd, lands in the host bind,
///     and NO refusal is recorded;
///   * `cwd: None`            -> the kernel would resolve it against the HOST cwd, so it must still be
///     refused by name (the refusal was correct before the cwd existed, and must stay correct);
///   * `cwd: Some("/probe")` with a `..` component -> refused, because collapsing `.`/`..` is lexical
///     work the engine does not do, and guessing it is how a path escapes the rootfs.
#[test]
fn a_relative_path_is_translated_against_the_plans_cwd() {
    let root = fixture_root("relative");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-relative");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    // Writes "relative-ok" (or "dotdot-ok") to a RELATIVE name, then reports which arm it took.
    compile_c(
        &rootfs.join("bin").join("rel"),
        r#"#include <fcntl.h>
#include <errno.h>
#include <stdio.h>
#include <unistd.h>
#include <string.h>
int main(int argc, char **argv) {
    const char *name = (argc > 1) ? argv[1] : "relative-report";
    char cwd[4096];
    // Diagnostics go to the inherited stderr, so the test log says what the GUEST saw rather than
    // leaving the harness to infer it from files that may or may not exist.
    fprintf(stderr, "[guest] name=%s cwd=%s\n", name,
            getcwd(cwd, sizeof cwd) ? cwd : "<no cwd>");
    int fd = open(name, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { fprintf(stderr, "[guest] open failed errno=%d\n", errno); return 41; }
    if (write(fd, "relative-ok", 11) < 0) { return 42; }
    close(fd);
    fprintf(stderr, "[guest] wrote %s\n", name);
    return 0;
}
"#,
    );

    // SECOND guest for the `..` arm, with the name HARDCODED: this engine publishes only `argv[0]`
    // into the shared page (`native_run.rs:604-609`, where `argv_slots[1]` is set to null), so a guest
    // that reads `argv[1]` sees `argc == 1` and silently falls back to its default. Measured here: the
    // dotdot arm wrote `relative-report` because argv[1] never arrived. Passing the name as a compiled-in
    // constant keeps THIS test about path translation; the dropped arguments are recorded as the next
    // increment rather than papered over.
    compile_c(
        &rootfs.join("bin").join("reldotdot"),
        r#"#include <fcntl.h>
#include <unistd.h>
int main(void) {
    int fd = open("../escaped-report", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { return 41; }
    if (write(fd, "relative-ok", 11) < 0) { return 42; }
    close(fd);
    return 0;
}
"#,
    );

    let mut binds = vec![("/probe".to_string(), probe.clone())];
    binds.extend(host_runtime_binds());

    // ---- arm 1: the cwd is known, so the relative name IS translatable ---------------------------
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds: binds.clone(),
        cwd: Some("/probe".to_string()),
        env: Vec::new(),
    };
    let run = run_native(
        &["/bin/rel".to_string(), "relative-report".to_string()],
        &plan,
        false,
    )
    .expect("traced run with a cwd");
    let landed = probe.join("relative-report");
    eprintln!(
        "[rel] cwd=Some(\"/probe\") exit={} landed={} translations={:?} refusals={:?}",
        run.exit_code,
        landed.display(),
        run.translations
            .iter()
            .map(|t| (&t.guest, t.syscall))
            .collect::<Vec<_>>(),
        run.refusal_summary()
    );
    assert_eq!(
        run.exit_code, 0,
        "the guest must reach its write; a non-zero code here means the engine refused the run"
    );
    assert!(
        landed.is_file(),
        "the relative name must resolve through the cwd into the bound HOST directory; {} is absent",
        landed.display()
    );
    assert!(
        run.translations
            .iter()
            .any(|t| t.guest == "/probe/relative-report"),
        "the report must name the cwd-JOINED guest path, not the bare relative one; got {:?}",
        run.translations
            .iter()
            .map(|t| (&t.guest, &t.host))
            .collect::<Vec<_>>()
    );
    assert!(
        run.refusals.iter().all(|r| r.name != Some("SYS_openat")),
        "a relative path that was translated must NOT also be recorded as an untranslated refusal; \
         got {:?}",
        run.refusal_summary()
    );

    // ---- arm 2: negative control - no known cwd, so the same open must still be refused -----------
    // Without this arm the assertion above would also pass if the engine simply stopped refusing
    // relative paths everywhere, which would be a host-fallback regression.
    let plan_none = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds: binds.clone(),
        cwd: None,
        env: Vec::new(),
    };
    let run_none = run_native(
        &["/bin/rel".to_string(), "relative-report".to_string()],
        &plan_none,
        false,
    )
    .expect("traced run without a cwd");
    eprintln!(
        "[rel] cwd=None exit={} refusals={:?}",
        run_none.exit_code,
        run_none.refusal_summary()
    );
    // The refusal does not BLOCK the syscall in non-strict mode: the kernel resolves the relative name
    // against this test process's own cwd, which is why `relative-report` appears in the crate directory
    // and why the CLI fails the whole run on such a refusal. Remove it here so the test leaves nothing
    // behind - the same "a fixture cleans what it creates" rule the Drop guard follows elsewhere.
    let _ = std::fs::remove_file("relative-report");
    assert!(
        run_none
            .refusals
            .iter()
            .any(|r| r.name == Some("SYS_openat")),
        "with no known cwd the kernel would resolve the relative name against the HOST cwd, so the \
         engine must still refuse it by name; got {:?}",
        run_none.refusal_summary()
    );

    // ---- arm 3: a `..` component is refused rather than joined ------------------------------------
    // NON-STRICT, because strict mode cannot reach this point: the engine's own bootstrap read on the
    // release pipe is a pass-through refusal, so `strict` stops on the engine itself (measured here:
    // exit -1 with only `SYS_read` in the report). What this arm asserts is therefore the REFUSAL, not
    // the file's absence: in non-strict mode the kernel still resolves the untranslated name (measured:
    // the `..` write lands on the host side of the bind), and that is precisely why the CLI fails a whole
    // run when it sees a path refusal - `native_verdict` is the layer that turns this into a non-run.
    let run_dotdot = run_native(&["/bin/reldotdot".to_string()], &plan, false)
        .expect("traced run with a dotdot name");
    eprintln!(
        "[rel] dotdot exit={} refusals={:?}",
        run_dotdot.exit_code,
        run_dotdot.refusal_summary()
    );
    assert!(
        run_dotdot
            .refusals
            .iter()
            .any(|r| r.name == Some("SYS_openat")),
        "a relative name with `..` must be refused by name, not joined and normalised; got {:?}",
        run_dotdot.refusal_summary()
    );
}

/// The guest receives EVERY argument, not only `argv[0]` (2026-11-09, p7 argv).
///
/// This engine published `argv[0]` and then terminated the pointer array, so a guest invoked with
/// arguments saw `argc == 1` and silently fell back to its own defaults - measured, and undocumented.
/// For the operator's goal the consequence is direct: `podnext --engine native run c -- /bin/sh -c 'cmd'`
/// would run `/bin/sh` with no arguments and no error.
#[test]
fn the_guest_receives_every_argument() {
    let root = fixture_root("argv");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-argv");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    // Reports argc and every argv entry into the bound HOST directory, because the child's stdout and
    // stderr are inherited and therefore unassertable from here.
    compile_c(
        &rootfs.join("bin").join("argv"),
        r#"#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>
int main(int argc, char **argv) {
    char buf[2048];
    int n = 0;
    n += snprintf(buf + n, sizeof buf - n, "argc=%d", argc);
    for (int i = 0; i < argc && n < (int)sizeof buf - 96; i++) {
        n += snprintf(buf + n, sizeof buf - n, "|argv[%d]=%s", i, argv[i]);
    }
    int fd = open("/probe/argv-report", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { return 51; }
    if (write(fd, buf, n) < 0) { return 52; }
    close(fd);
    return 0;
}
"#,
    );

    let mut binds = vec![("/probe".to_string(), probe.clone())];
    binds.extend(host_runtime_binds());
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: Some("/".to_string()),
        env: Vec::new(),
    };

    // An entry with a space and one with `=` are included on purpose: a naive re-split would mangle them.
    let argv: Vec<String> = ["/bin/argv", "one", "two words", "--flag=value"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let run = run_native(&argv, &plan, false).expect("traced run with arguments");
    let report = std::fs::read_to_string(probe.join("argv-report")).unwrap_or_else(|e| {
        panic!(
            "the guest did not report its argv (exit {}): {e}",
            run.exit_code
        )
    });
    eprintln!("[argv] exit={} report={report}", run.exit_code);
    assert_eq!(run.exit_code, 0, "the guest must run to completion");
    assert_eq!(
        report, "argc=4|argv[0]=/bin/argv|argv[1]=one|argv[2]=two words|argv[3]=--flag=value",
        "every argument must reach the guest, in order, unchanged - including spaces and `=`"
    );

    // ---- the two size limits are refused BY NAME, before the fork --------------------------------
    // A silent truncation here would be the same defect in a different place: the guest would run with
    // fewer arguments than the caller asked for and nothing would say so.
    let too_many: Vec<String> = std::iter::once("/bin/argv".to_string())
        .chain((0..80).map(|i| format!("a{i}")))
        .collect();
    let err = run_native(&too_many, &plan, false).expect_err("81 entries must be refused");
    let msg = err.to_string();
    eprintln!("[argv] too many -> {msg}");
    assert!(
        msg.contains("needs") && msg.contains("slots"),
        "the refusal must name the slot limit; got {msg}"
    );

    let too_long: Vec<String> = vec!["/bin/argv".to_string(), "x".repeat(3000)];
    let err =
        run_native(&too_long, &plan, false).expect_err("a 3000-byte argument must be refused");
    let msg = err.to_string();
    eprintln!("[argv] too long -> {msg}");
    assert!(
        msg.contains("do not fit") && msg.contains("byte region"),
        "the refusal must name the byte limit; got {msg}"
    );
}

/// p7 gap 3 (2026-11-09). podnext prints, for BOTH engines, "the guest runs as root (uid 0): PRoot
/// maps every guest id to the single host uid via `--change-id=0:0:0`" - and the native path used to
/// print that sentence while handing the HOST ids straight through (measured uid=0 under proot
/// against uid=10396 under native). This locks the claim to the behaviour, and it is the reason
/// `id_reporting`/`override_id_result` exist in the engine.
///
/// The executed negative control for this mechanism is NOT in this test, because flipping an
/// environment variable in-process would race the sibling tests: it is `PDN_NATIVE_NO_ID_MAP=1`
/// against `scripts/engine_differential.py`, measured as `uid=0/10396 uid differs` with rc=1 while the
/// default run is `EQUAL` with rc=0. Both runs are quoted in research/architecture/CHANGE_LOG.md.
#[test]
fn the_guest_sees_the_mapped_root_identity() {
    let root = fixture_root("idmap");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-idmap");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    compile_c(
        &rootfs.join("bin").join("idmap"),
        r#"#include <unistd.h>
#include <fcntl.h>
#include <stdio.h>
int main(void) {
    char buf[128];
    int n = snprintf(buf, sizeof buf, "uid=%d euid=%d gid=%d egid=%d\n",
                     (int)getuid(), (int)geteuid(), (int)getgid(), (int)getegid());
    int fd = open("/probe/ids", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { return 61; }
    if (write(fd, buf, (unsigned)n) < 0) { return 62; }
    close(fd);
    return 0;
}
"#,
    );

    let argv = vec!["/bin/idmap".to_string()];
    let mut binds = vec![("/probe".to_string(), probe.clone())];
    binds.extend(host_runtime_binds());
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: Some("/".to_string()),
        env: Vec::new(),
    };

    let run = run_native(&argv, &plan, false).expect("traced run of the identity fixture");
    let report = std::fs::read_to_string(probe.join("ids"))
        .unwrap_or_else(|e| panic!("the guest did not report its ids: {e}"));
    eprintln!("[idmap] exit={} ids={}", run.exit_code, report.trim());

    assert_eq!(
        run.exit_code, 0,
        "the fixture must run to completion (61/62 mean it could not write its report)"
    );
    assert_eq!(
        report.trim(),
        "uid=0 euid=0 gid=0 egid=0",
        "the guest must see the mapped root identity that the note promised it for every engine"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// p7 envp (2026-11-09). A guest that never reads its environment cannot tell an empty `envp` from a
/// populated one, so the differential harness could not see this gap - the guest here reads it. Two
/// arms, so the test carries its own control: with the plan's environment the guest must find the
/// variable, and with an empty plan it must not, which proves the value travelled through `RunPlan`
/// rather than through the engine's own process environment.
#[test]
fn the_guest_receives_the_configured_environment() {
    let root = fixture_root("envp");
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-envp");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    compile_c(
        &rootfs.join("bin").join("envp"),
        r#"#include <unistd.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
int main(void) {
    char buf[256];
    const char *probe = getenv("PDN_TEST_ENV");
    const char *path = getenv("PATH");
    int n = snprintf(buf, sizeof buf, "probe=%s\npath=%s\n",
                     probe ? probe : "<unset>", path ? path : "<unset>");
    int fd = open("/probe/env", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { return 61; }
    if (write(fd, buf, (unsigned)n) < 0) { return 62; }
    close(fd);
    return 0;
}
"#,
    );

    let argv = vec!["/bin/envp".to_string()];
    let mut binds = vec![("/probe".to_string(), probe.clone())];
    binds.extend(host_runtime_binds());
    let base = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: Some("/".to_string()),
        env: Vec::new(),
    };

    let read_report = |tag: &str| -> String {
        std::fs::read_to_string(probe.join("env"))
            .unwrap_or_else(|e| panic!("[{tag}] the guest did not report its environment: {e}"))
    };

    // ---- arm with the variable: it must reach the guest -----------------------------------------
    let with_env = RunPlan {
        env: vec![("PDN_TEST_ENV".to_string(), "from-the-plan".to_string())],
        ..base
    };
    let run =
        run_native(&argv, &with_env, false).expect("traced run with a configured environment");
    let report = read_report("configured");
    eprintln!(
        "[envp] configured exit={} report={:?}",
        run.exit_code, report
    );
    assert_eq!(run.exit_code, 0, "the fixture must run to completion");
    assert!(
        report.contains("probe=from-the-plan"),
        "the plan's variable must reach the guest: {report:?}"
    );
    assert!(
        report.contains("path=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"),
        "the engine's own allow-list must be present too: {report:?}"
    );

    // ---- control arm: no plan, no variable ------------------------------------------------------
    let empty = RunPlan {
        env: Vec::new(),
        ..with_env
    };
    let run =
        run_native(&argv, &empty, false).expect("traced run without a configured environment");
    let report = read_report("empty");
    eprintln!("[envp] empty exit={} report={:?}", run.exit_code, report);
    assert_eq!(
        run.exit_code, 0,
        "the control arm must run to completion too"
    );
    assert!(
        report.contains("probe=<unset>"),
        "with no configured environment the variable must be absent, which is what proves the value \
         came from the plan and not from the engine's process environment: {report:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------------------------
// p7 interp (added 2026-11-09): a guest whose PT_INTERP does NOT exist on the host.
//
// MEASURED problem (2026-11-09, real Debian rootfs `deb-pdn`): the guest `/usr/bin/dash` names
// `PT_INTERP /lib/ld-linux-aarch64.so.1`, and this Android host has no `/lib` at all, so
// `podnext --engine native run <ctr> -- /bin/sh -c 'echo hi'` exited 127 with NO guest output while
// the same command under proot printed `hi` and exited 0. The KERNEL opens PT_INTERP on the HOST
// before the tracee runs a single instruction, so entry-time path translation cannot reach it; the
// fix execs the guest's OWN interpreter with the guest program as its argument (the
// `ld.so PROGRAM [ARGS...]` form), and the loader then opens the program and its libraries through
// the SAME traced syscalls the engine already rewrites.
//
// This test builds that situation WITHOUT a Debian rootfs so it runs on every host: the guest is
// compiled with `-Wl,--dynamic-linker=<path absent on the host>`, and the host's REAL interpreter is
// copied INSIDE the rootfs at exactly that path - which is the shape a Debian rootfs has
// (`/lib/ld-linux-aarch64.so.1` absent on the host, present in the rootfs).
//
// Both directions are asserted, so the test carries its own control: running the guest binary
// DIRECTLY on the host must fail (the kernel cannot find PT_INTERP), and running it under the engine
// must succeed with the interpreter recorded. Sabotage the substitution - make `guest_interpreter`
// in `src/native_run.rs` return `None` - and the positive arm goes red, which is the negative
// control: it is the substitution, not something else, that starts the guest.
// ---------------------------------------------------------------------------------------------

/// `compile_c`, but with extra compiler/linker flags - here `-Wl,--dynamic-linker=<path>`, so the
/// guest's `PT_INTERP` names a path the host does not have. A build failure is a RED test, never a
/// silent skip.
fn compile_c_with(dst: &Path, source: &str, extra: &[&str]) {
    let src = dst.with_extension("c");
    std::fs::write(&src, source).unwrap_or_else(|e| panic!("write {}: {e}", src.display()));
    let cc = compiler();
    let out = std::process::Command::new(&cc)
        .arg(&src)
        .args(extra)
        .arg("-o")
        .arg(dst)
        .output()
        .unwrap_or_else(|e| panic!("run {cc} to build the fixture: {e}"));
    assert!(
        out.status.success(),
        "{} failed to build {}: {}",
        cc,
        dst.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_file(&src);
}

/// Removes the fixture root when the test ends, INCLUDING when an assertion panics (copied from
/// `native_dynamic_guest.rs`): the trailing `remove_dir_all` is skipped the moment an assertion
/// panics, which is exactly when a fixture tree is most likely to survive.
struct InterpGuard(PathBuf);

impl Drop for InterpGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_guest_whose_pt_interp_is_missing_on_the_host_runs_via_its_own_interpreter() {
    let root = fixture_root("interp");
    let _guard = InterpGuard(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-interp");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    // Reports argc and the first two argv entries into a bound HOST directory, then returns a
    // sentinel the assertions can read back. stdout is NOT used: cargo runs this file's tests
    // concurrently in one process, so capturing fd 1 (as `native_dynamic_guest.rs` may, being a
    // single-test file) would swallow a sibling's output.
    let source = r#"#include <unistd.h>
#include <fcntl.h>
#include <stdio.h>
int main(int argc, char **argv) {
    char buf[256];
    int n = snprintf(buf, sizeof buf, "argc=%d|argv0=%s|argv1=%s", argc,
                     argc > 0 ? argv[0] : "<none>", argc > 1 ? argv[1] : "<none>");
    int fd = open("/probe/interp-report", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { return 71; }
    if (write(fd, buf, (unsigned)n) < 0) { return 72; }
    close(fd);
    return 9;
}
"#;

    // The host's OWN interpreter: taken from a guest the host toolchain builds normally, so the file
    // is one this host can really run (bionic `linker64` on Termux, glibc `ld.so` on the runner).
    let normal = root.join("guest-normal");
    compile_c(&normal, source);
    let real_interp = proot_core::native_run::elf_interp(&normal).unwrap_or_else(|| {
        panic!(
            "{} names no PT_INTERP; it is not the dynamically linked guest this test needs",
            normal.display()
        )
    });
    assert!(
        Path::new(&real_interp).exists(),
        "the host's own interpreter {real_interp} must exist for this fixture to mean anything"
    );

    // The guest under test names an interpreter that does NOT exist on this host.
    const MISSING: &str = "/pdn-interp-missing/ld.so";
    assert!(
        !Path::new(MISSING).exists(),
        "the fixture's PT_INTERP path {MISSING} exists on this host, so it would not reproduce the \
         measured failure (the interpreter is created INSIDE the rootfs, never on the host)"
    );
    let guest = rootfs.join("bin").join("guest");
    compile_c_with(
        &guest,
        source,
        &[&format!("-Wl,--dynamic-linker={MISSING}")],
    );
    assert_eq!(
        proot_core::native_run::elf_interp(&guest).as_deref(),
        Some(MISSING),
        "the fixture guest must name the absent interpreter as its PT_INTERP"
    );

    // The interpreter the host CAN run is placed INSIDE the rootfs at exactly that path - the shape a
    // Debian rootfs has (`/lib/ld-linux-aarch64.so.1` absent on the host, present in the rootfs).
    let interp_in_rootfs = rootfs.join(MISSING.trim_start_matches('/'));
    std::fs::create_dir_all(interp_in_rootfs.parent().unwrap()).expect("create interp dir");
    std::fs::copy(&real_interp, &interp_in_rootfs).unwrap_or_else(|e| {
        panic!(
            "copy {} -> {}: {e}",
            real_interp,
            interp_in_rootfs.display()
        )
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&interp_in_rootfs)
            .expect("stat interpreter copy")
            .permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&interp_in_rootfs, perm).expect("chmod interpreter copy");
    }
    eprintln!("[interp] host interpreter {real_interp}");
    eprintln!("[interp] absent PT_INTERP {MISSING}");
    eprintln!("[interp] interp in rootfs {}", interp_in_rootfs.display());

    // ---- negative arm: the guest is NOT runnable directly on the host --------------------------
    // The measured failure, reproduced without the engine: the kernel opens PT_INTERP before the
    // program runs and it is absent, so `execve` fails (ENOENT) and the guest never executes. A run
    // that SUCCEEDED here would mean the fixture does not reproduce the problem at all.
    let direct = std::process::Command::new(&guest).arg("host").output();
    match &direct {
        Ok(o) => assert!(
            !o.status.success(),
            "the guest RAN directly on the host (exit {:?}); the fixture does not reproduce the \
             missing-interpreter failure and this test would prove nothing",
            o.status.code()
        ),
        Err(e) => eprintln!("[interp] direct host exec refused as expected: {e}"),
    }

    // ---- positive arm: the engine starts it by executing its own interpreter -------------------
    // `host_runtime_binds` binds the host runtime trees to themselves so the interpreter finds its
    // OWN libraries (libc): the claim under test is that the interpreter is exec'd at all, NOT that
    // the rootfs is a closed namespace (that is `native_dynamic_guest.rs`'s subject).
    let mut binds = host_runtime_binds();
    binds.push(("/probe".to_string(), probe.clone()));
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: Some("/".to_string()),
        env: Vec::new(),
    };
    let run = run_native(
        &["/bin/guest".to_string(), "engine".to_string()],
        &plan,
        false,
    )
    .expect("traced run of the missing-interpreter guest");
    eprintln!(
        "[interp] exit={} interpreter={:?} executed={:?} translations={:?}",
        run.exit_code, run.interpreter, run.executed, run.translations
    );

    assert_eq!(
        run.interpreter.as_deref(),
        Some(MISSING),
        "the engine must record that it exec'd the guest's interpreter; `None` means it tried the \
         guest directly, which the kernel cannot start on this host"
    );
    assert_eq!(
        run.exit_code, 9,
        "the guest's own exit code (9) proves it ran; 127 is the missing-interpreter failure this \
         test exists to catch, and 71/72 mean it could not write its report"
    );

    // The interpreter exec'd is the one INSIDE the rootfs, not a host path or the guest binary.
    let t = run
        .translations
        .iter()
        .find(|t| t.syscall == "SYS_execve" && t.guest == MISSING)
        .unwrap_or_else(|| {
            panic!(
                "the engine must record the execve of the guest's interpreter {MISSING}; \
                 translations={:?}",
                run.translations
            )
        });
    assert_eq!(
        canon(&t.host),
        canon(&interp_in_rootfs),
        "the substituted exec must land on the interpreter inside the rootfs"
    );
    match run.executed.as_ref() {
        Some(p) => assert_eq!(
            canon(p),
            canon(&interp_in_rootfs),
            "the kernel executed {} which is not the guest's interpreter",
            p.display()
        ),
        None => eprintln!(
            "[interp] /proc/<pid>/exe was not readable; the interpreter field and the translation \
             carry the verdict"
        ),
    }

    // The guest saw its OWN argv[0] and its argument - the loader form preserves them, which is why
    // no `--argv0` is needed (see the module docs).
    let report = std::fs::read_to_string(probe.join("interp-report")).unwrap_or_else(|e| {
        panic!(
            "the guest did not write its report (exit {}): {e}",
            run.exit_code
        )
    });
    eprintln!("[interp] guest report {report:?}");
    assert_eq!(
        report, "argc=2|argv0=/bin/guest|argv1=engine",
        "the guest must see its own argv[0] and its argument, unchanged, through the interpreter form"
    );

    // ---- control: with NO rootfs there is nothing to substitute, and the kernel refuses ----------
    // The same guest, same interpreter, but a plan with no rootfs: `guest_interpreter` returns `None`
    // (it needs a rootfs), so the engine hands the guest path to the kernel unchanged and the run
    // fails exactly as the direct exec did. This is what makes the positive arm's success load-bearing.
    let bare = RunPlan::default();
    let no_rootfs = run_native(
        &["/bin/guest".to_string(), "engine".to_string()],
        &bare,
        false,
    )
    .expect("a no-plan run returns a report, not a crash");
    eprintln!(
        "[interp] no-rootfs exit={} interpreter={:?}",
        no_rootfs.exit_code, no_rootfs.interpreter
    );
    assert_eq!(
        no_rootfs.interpreter, None,
        "with no rootfs the engine must not claim an interpreter substitution"
    );
    assert_eq!(
        no_rootfs.exit_code, 127,
        "with no rootfs the kernel is handed the guest path and its absent PT_INTERP, so execve \
         fails with 127 - the failure the substitution exists to remove"
    );
}

// ---------------------------------------------------------------------------------------------
// p7 argv0 (added 2026-11-09): the KERNEL-LEVEL argv[0] a guest sees through /proc/<pid>/cmdline.
//
// MEASURED defect, on a real Debian rootfs run as `-- /bin/sh -c '/probe/cmdline0 $$'`:
//   proot  : cmdline0=/bin/sh|-c|/probe/cmdline0 $$|
//   native : cmdline0=<rootfs>/lib/ld-linux-aarch64.so.1|/bin/sh|-c|/probe/cmdline0 $$|
// The substituted run's argv slot 0 pointed at PATH_OFFSET - the SAME string the engine rewrites to
// the translated HOST path at the execve stop - so the parent's rewrite of the exec path dragged the
// host path into argv[0]. The fix publishes the interpreter's GUEST path as its OWN string and points
// argv[0] at it, so the rewrite of the exec path cannot reach it.
//
// Two arms, so the test carries its own control:
//   * SUBSTITUTED - the guest's PT_INTERP is absent on the host and present in the rootfs, so the
//     engine execs the interpreter; argv[0] must be the interpreter's GUEST path, NOT the translated
//     host path (revert the fix in `src/native_run.rs::child_main` and this arm goes red);
//   * NOT SUBSTITUTED - an ordinarily linked guest (its interpreter exists on the host), so no
//     substitution happens; argv[0] must stay the guest PROGRAM path with no interpreter prefix.
// ---------------------------------------------------------------------------------------------

/// A guest that reads its OWN kernel argv from `/proc/self/cmdline` and writes it, `|`-separated, to
/// the bound host directory - the only way a test process can observe the KERNEL-level argv[0] rather
/// than the loader-constructed one the program's `main` receives.
const CMDLINE_PROBE_SOURCE: &str = r#"#include <fcntl.h>
#include <unistd.h>
int main(void) {
    char buf[4096];
    int fd = open("/proc/self/cmdline", O_RDONLY);
    if (fd < 0) { return 81; }
    int n = read(fd, buf, sizeof buf - 1);
    if (n < 0) { return 82; }
    for (int i = 0; i < n; i++) { if (buf[i] == 0) buf[i] = '|'; }
    int out = open("/probe/cmdline-report", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (out < 0) { return 83; }
    if (write(out, buf, (unsigned)n) < 0) { return 84; }
    close(out);
    return 9;
}
"#;

#[test]
fn the_kernel_argv0_is_the_guest_path_under_a_substitution() {
    let root = fixture_root("argv0");
    let _guard = InterpGuard(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-argv0");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    // The interpreter the host CAN run, and a path it CANNOT - the same shape as the interp test above.
    let normal = root.join("guest-normal");
    compile_c(&normal, CMDLINE_PROBE_SOURCE);
    let real_interp = proot_core::native_run::elf_interp(&normal).unwrap_or_else(|| {
        panic!(
            "{} names no PT_INTERP; not the dynamically linked guest this test needs",
            normal.display()
        )
    });
    const MISSING: &str = "/pdn-interp-missing/ld.so";
    assert!(
        !Path::new(MISSING).exists(),
        "the fixture's PT_INTERP path {MISSING} exists on this host, so no substitution would occur"
    );

    // ---- the SUBSTITUTED guest: its PT_INTERP is absent on the host, present in the rootfs --------
    let guest_sub = rootfs.join("bin").join("guest");
    compile_c_with(
        &guest_sub,
        CMDLINE_PROBE_SOURCE,
        &[&format!("-Wl,--dynamic-linker={MISSING}")],
    );
    assert_eq!(
        proot_core::native_run::elf_interp(&guest_sub).as_deref(),
        Some(MISSING)
    );
    let interp_in_rootfs = rootfs.join(MISSING.trim_start_matches('/'));
    std::fs::create_dir_all(interp_in_rootfs.parent().unwrap()).expect("create interp dir");
    std::fs::copy(&real_interp, &interp_in_rootfs).unwrap_or_else(|e| {
        panic!(
            "copy {} -> {}: {e}",
            real_interp,
            interp_in_rootfs.display()
        )
    });

    // ---- the NOT-SUBSTITUTED guest: an ordinary dynamic binary whose interpreter IS on the host ---
    let guest_prog = rootfs.join("bin").join("guestn");
    std::fs::copy(&normal, &guest_prog)
        .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", normal.display(), guest_prog.display()));

    let mut binds = host_runtime_binds();
    binds.push(("/probe".to_string(), probe.clone()));
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: Some("/".to_string()),
        env: Vec::new(),
    };

    let read_report = |tag: &str| -> String {
        std::fs::read_to_string(probe.join("cmdline-report"))
            .unwrap_or_else(|e| panic!("[{tag}] the guest did not write its cmdline report: {e}"))
    };

    // ---- arm S: SUBSTITUTED -> argv[0] is the interpreter's GUEST path, never the host path ------
    let _ = std::fs::remove_file(probe.join("cmdline-report"));
    let run_s = run_native(
        &["/bin/guest".to_string(), "engine".to_string()],
        &plan,
        false,
    )
    .expect("traced run of the substituted guest");
    let report_s = read_report("substituted");
    eprintln!(
        "[argv0] substituted exit={} interpreter={:?} cmdline={report_s:?}",
        run_s.exit_code, run_s.interpreter
    );
    assert_eq!(
        run_s.interpreter.as_deref(),
        Some(MISSING),
        "the engine must have substituted"
    );
    assert_eq!(
        run_s.exit_code, 9,
        "the guest must run (81..84 mean it could not read/write)"
    );
    assert_eq!(
        report_s,
        format!("{MISSING}|/bin/guest|engine|"),
        "the kernel argv[0] must be the interpreter's GUEST path ({MISSING}); a <rootfs>/...\n\
         prefix would be the translated HOST path the engine writes at PATH_OFFSET"
    );
    assert!(
        !report_s.contains(&rootfs.display().to_string()),
        "argv[0] leaked the translated HOST path {:?}: {report_s:?}",
        rootfs.display()
    );

    // ---- arm N: NOT SUBSTITUTED -> argv[0] is the guest PROGRAM path, with no interpreter prefix --
    let _ = std::fs::remove_file(probe.join("cmdline-report"));
    let run_n = run_native(
        &["/bin/guestn".to_string(), "engine".to_string()],
        &plan,
        false,
    )
    .expect("traced run of the ordinary guest");
    let report_n = read_report("not-substituted");
    eprintln!(
        "[argv0] not-substituted exit={} interpreter={:?} cmdline={report_n:?}",
        run_n.exit_code, run_n.interpreter
    );
    assert_eq!(
        run_n.interpreter, None,
        "an on-host interpreter must not be substituted"
    );
    assert_eq!(run_n.exit_code, 9, "the guest must run");
    assert_eq!(
        report_n, "/bin/guestn|engine|",
        "with no substitution the kernel argv[0] must stay the guest PROGRAM path, unchanged - a\n\
         substitution here would be the NON-substituted regression this arm guards"
    );
}

// ---------------------------------------------------------------------------------------------
// p7 interp-desc (added 2026-11-09): a fork DESCENDANT whose PT_INTERP is absent on the host.
//
// MEASURED defect (2026-11-09, real Debian rootfs `deb-pdn`): `run realroot -- /bin/sh -c
// '/bin/dash -c "echo CHILD-RAN"'` exited 127 under `--engine native` with `/bin/sh: 1: /bin/dash:
// not found`, while the same command under proot printed CHILD-RAN and exited 0. The engine
// substituted the interpreter for its OWN child (the first `/bin/sh`, via the shared page) but NOT
// for the descendant (`/bin/dash`), whose path went through `translate_execve_remote`: the kernel
// then opened the rootfs copy's ABSENT PT_INTERP on the host and the descendant's execve failed.
// The fix gives the descendant path the SAME substitution `guest_interpreter` already enforces,
// written onto the descendant's own stack.
//
// Fixture, no Debian rootfs needed: a forker is the engine's OWN child - an ordinary dynamic binary
// whose loader IS on the host, so NO substitution happens for it - and it forks a child that execs a
// guest whose PT_INTERP is absent on the host and whose interpreter is present in the rootfs. The
// failure under test is therefore only the DESCENDANT's exec. The descendant writes its own argv[0]
// and exits 9, which the forker propagates.
//
// Negative control: sabotage the substitution (make `guest_interpreter` return `None`) and the
// descendant's execve fails - the forker's child falls through to `_exit(90)` - so the exit-code
// assertion goes red. The arm that is NOT the descendant needs nothing, which is what isolates the
// descendant path as the subject.
#[test]
fn a_forked_descendants_missing_interpreter_is_substituted_too() {
    let root = fixture_root("interp-desc");
    let _guard = InterpGuard(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs-interp-desc");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create fixture rootfs");
    let probe = root.join("probe-out");
    std::fs::create_dir_all(&probe).expect("create probe dir");

    // The descendant: a dynamically linked guest whose loader the host does NOT have. It writes its
    // own argv[0] into a bound HOST directory and exits 9, so "it ran" and "its name survived the
    // interpreter form" are both readable back. stdout is not used (this file's tests share one
    // process; capturing fd 1 would swallow a sibling's output).
    let source = r#"#include <unistd.h>
#include <fcntl.h>
int main(int argc, char **argv) {
    int fd = open("/probe/desc-report", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { return 71; }
    const char *a0 = (argc > 0 && argv[0]) ? argv[0] : "<none>";
    unsigned n = 0;
    while (a0[n]) { n++; }
    if (write(fd, a0, n) < 0) { return 72; }
    close(fd);
    return 9;
}
"#;

    // The host's OWN interpreter - taken from a guest the host toolchain links normally - and a path
    // the host does NOT have.
    let normal = root.join("guest-normal");
    compile_c(&normal, source);
    let real_interp = proot_core::native_run::elf_interp(&normal).unwrap_or_else(|| {
        panic!(
            "{} names no PT_INTERP; it is not the dynamically linked guest this test needs",
            normal.display()
        )
    });
    assert!(
        Path::new(&real_interp).exists(),
        "the host's own interpreter {real_interp} must exist for this fixture to mean anything"
    );
    const MISSING: &str = "/pdn-interp-missing/ld.so";
    assert!(
        !Path::new(MISSING).exists(),
        "the fixture's PT_INTERP path {MISSING} exists on this host, so it would not reproduce the \
         measured failure (the interpreter is created INSIDE the rootfs, never on the host)"
    );

    let guest = rootfs.join("bin").join("guest");
    compile_c_with(
        &guest,
        source,
        &[&format!("-Wl,--dynamic-linker={MISSING}")],
    );
    assert_eq!(
        proot_core::native_run::elf_interp(&guest).as_deref(),
        Some(MISSING),
        "the fixture guest must name the absent interpreter as its PT_INTERP"
    );

    let interp_in_rootfs = rootfs.join(MISSING.trim_start_matches('/'));
    std::fs::create_dir_all(interp_in_rootfs.parent().unwrap()).expect("create interp dir");
    std::fs::copy(&real_interp, &interp_in_rootfs).unwrap_or_else(|e| {
        panic!(
            "copy {} -> {}: {e}",
            real_interp,
            interp_in_rootfs.display()
        )
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&interp_in_rootfs)
            .expect("stat interpreter copy")
            .permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&interp_in_rootfs, perm).expect("chmod interpreter copy");
    }
    eprintln!("[interp-desc] host interpreter {real_interp}");
    eprintln!("[interp-desc] absent PT_INTERP {MISSING}");
    eprintln!(
        "[interp-desc] interp in rootfs {}",
        interp_in_rootfs.display()
    );

    // The forker is the engine's OWN child and its loader is on the host, so it is NOT substituted:
    // whatever goes wrong below is the DESCENDANT's exec, not the child path.
    make_forker(&rootfs.join("bin").join("forker"), "/bin/guest");

    // The fixture reproduces the problem: the guest is NOT runnable directly on the host.
    let direct = std::process::Command::new(&guest).arg("host").output();
    match &direct {
        Ok(o) => assert!(
            !o.status.success(),
            "the guest RAN directly on the host (exit {:?}); the fixture does not reproduce the \
             missing-interpreter failure and this test would prove nothing",
            o.status.code()
        ),
        Err(e) => eprintln!("[interp-desc] direct host exec refused as expected: {e}"),
    }

    // ---- the descendant's exec must be substituted and run ------------------------------------
    let mut binds = host_runtime_binds();
    binds.push(("/probe".to_string(), probe.clone()));
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds,
        cwd: Some("/".to_string()),
        env: Vec::new(),
    };
    let run = run_native(&["/bin/forker".to_string()], &plan, false)
        .expect("traced run of the forking fixture");
    eprintln!(
        "[interp-desc] exit={} interpreter={:?} executed={:?} translations={:?}",
        run.exit_code, run.interpreter, run.executed, run.translations
    );

    assert_eq!(
        run.interpreter.as_deref(),
        Some(MISSING),
        "the engine must record the DESCENDANT's interpreter; `None` means the descendant's exec was \
         left to the kernel, which cannot open the absent PT_INTERP"
    );
    let t = run
        .translations
        .iter()
        .find(|t| t.syscall == "SYS_execve" && t.guest == MISSING)
        .unwrap_or_else(|| {
            panic!(
                "the engine must record the descendant's execve of its interpreter {MISSING}; \
                 translations={:?}",
                run.translations
            )
        });
    assert_eq!(
        canon(&t.host),
        canon(&interp_in_rootfs),
        "the descendant's substituted exec must land on the interpreter inside the rootfs"
    );

    // The forker propagates the descendant's own exit code (9). 90 is the forker's child falling
    // through because its execve failed - the missing-interpreter failure this test exists to catch.
    assert_eq!(
        run.exit_code, 9,
        "the descendant ran through its interpreter and exited 9; 90 means its execve failed (the \
         missing-interpreter failure), 127 the kernel's refusal"
    );

    let report = std::fs::read_to_string(probe.join("desc-report")).unwrap_or_else(|e| {
        panic!(
            "the descendant did not write its report (exit {}): {e}",
            run.exit_code
        )
    });
    eprintln!("[interp-desc] descendant report {report:?}");
    assert_eq!(
        report, "/bin/guest",
        "the descendant must see its OWN argv[0] - the guest path the caller passed - preserved as \
         the interpreter's argv[1]"
    );
}
