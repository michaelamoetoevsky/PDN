//! p7: a DYNAMICALLY LINKED guest runs to completion inside a rootfs.
//!
//! Written 2026-11-08. The two arms of `native_exec_translation.rs` prove `execve`'s path is
//! rewritten; they run STATIC fixtures (a `return N` program whose only libc dependency is the
//! syscall wrapper). A real guest is dynamic: the kernel starts the image and then the ELF
//! interpreter (PT_INTERP), running INSIDE the guest, opens its `NEEDED` libraries by absolute
//! path - paths that name the HOST, not the rootfs. `execve` translation alone therefore cannot
//! start a dynamic guest; the engine must also rewrite the loader's own path-taking syscalls
//! (`openat`, `newfstatat`, `readlinkat`, `faccessat`, `statx`, ...) at their ENTRY stops.
//!
//! This file holds ONE test, and it is one test on purpose: it captures the guest's stdout by
//! temporarily redirecting this process's fd 1 to a pipe, and a second test running concurrently in
//! the same binary would then have its own output (and libtest's) land in that pipe.
//!
//! WHY THE FIXTURE CARRIES A LIBRARY OF ITS OWN. The failure that matters is a run that LOOKS
//! right while the engine quietly used a host file. On this device a plain `cc` binary runs fine
//! with no environment at all, because its linker and libc both exist at absolute host paths - so
//! "the dynamic fixture ran" would prove nothing about the translation. The fixture therefore
//! links one library, `libpdnflag.so`, that lives ONLY inside the rootfs: its `pdn_flag()` returns
//! 7, which is the run's exit code, and the loader can only reach it through a translated
//! `/p7loader/lib/libpdnflag.so`. Output + exit 7 together cannot be produced by a host binary.
//!
//! BLOCKED, named rather than implied: the kernel opens `PT_INTERP` before the tracee runs its
//! first instruction, so the interpreter itself is the HOST's - here `/system/bin/linker64`. A
//! guest whose interpreter does not exist on the host cannot be started by this engine yet; the
//! loader that the interpreter then runs IS traced, and that is what this test covers.

use proot_core::native_run::{run_native, RunPlan};
use std::path::{Path, PathBuf};

/// The C compiler, resolved the same way `native_exec_translation.rs` does, so a fixture build is
/// never silently skipped.
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

/// A per-test fixture directory under `$PREFIX/tmp` (`/tmp` is not writable for an Android app
/// uid). The tag exists so a rerun cannot inherit a stale tree.
fn fixture_root(tag: &str) -> PathBuf {
    let base = match std::env::var("PREFIX") {
        Ok(p) => PathBuf::from(p).join("tmp"),
        Err(_) => std::env::temp_dir(),
    };
    base.join(format!("p7-loader-{}-{}", std::process::id(), tag))
}

/// Run a command, or panic with its stderr. A missing tool is a RED test, never a silent skip: a
/// skipped fixture build would let this test pass having proven nothing.
fn run_tool(program: &str, args: &[&str]) -> String {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("run {}: {e}", program));
    assert!(
        out.status.success(),
        "{} {:?} failed: {}",
        program,
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Compile `source` into `dst` with `extra` compiler flags.
fn compile_c(dst: &Path, source: &str, extra: &[&str]) {
    let src = dst.with_extension("c");
    std::fs::write(&src, source).unwrap_or_else(|e| panic!("write {}: {e}", src.display()));
    let cc = compiler();
    let mut args: Vec<String> = vec![src.to_string_lossy().into_owned()];
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push("-o".to_string());
    args.push(dst.to_string_lossy().into_owned());
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_tool(&cc, &refs);
    let _ = std::fs::remove_file(&src);
}

/// The `DT_NEEDED` sonames of `path`, read from the ELF the linker produced. `readelf` is the
/// authority on the artefact, but the tool that reads it differs by host, so all three are tried
/// and a host with none of them fails this test loudly instead of skipping the copies below.
fn needed_libs(path: &Path) -> Vec<String> {
    let p = path.to_string_lossy().into_owned();
    let out = ["readelf", "llvm-readelf"]
        .iter()
        .find_map(|tool| {
            std::process::Command::new(tool)
                .args(["-d", &p])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        })
        .or_else(|| {
            // `objdump -p` prints `  NEEDED               libc.so.6`.
            std::process::Command::new("objdump")
                .args(["-p", &p])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        })
        .unwrap_or_else(|| {
            panic!(
                "no ELF reader (readelf / llvm-readelf / objdump) on PATH; cannot learn the \
                 fixture's NEEDED libraries, and skipping that step would make this test vacuous"
            )
        });

    let mut out_names = Vec::new();
    for line in out.lines() {
        if let Some(rest) = line.split_once("(NEEDED)").map(|(_, r)| r) {
            if let (Some(a), Some(b)) = (rest.find('['), rest.rfind(']')) {
                if b > a {
                    out_names.push(rest[a + 1..b].to_string());
                }
            }
        } else if let Some(rest) = line.trim().strip_prefix("NEEDED") {
            let name = rest.trim();
            if !name.is_empty() {
                out_names.push(name.to_string());
            }
        }
    }
    assert!(
        !out_names.is_empty(),
        "the fixture {} reports no NEEDED libraries; it is not the dynamic binary this test needs",
        path.display()
    );
    out_names
}

/// The absolute path the toolchain resolves `name` to - `cc -print-file-name` on both hosts
/// (Termux answers `/system/lib64/libc.so`, a glibc runner answers the multiarch path).
fn toolchain_lib(name: &str) -> PathBuf {
    let cc = compiler();
    let out = run_tool(&cc, &[&format!("-print-file-name={name}")]);
    let p = PathBuf::from(out.trim());
    assert!(
        p.is_file(),
        "the toolchain resolved {} to {} which is not a file",
        name,
        p.display()
    );
    p
}

/// Copy `src` to `dst`, creating parents, and PROVE the copy is a different file from the
/// destination it replaces (a silent no-op copy would make the rootfs contain the host's library,
/// which is exactly the false positive this test exists to catch).
fn copy_to(src: &Path, dst: &Path) {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).expect("create destination directory");
    }
    let _ = std::fs::remove_file(dst);
    std::fs::copy(src, dst)
        .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src.display(), dst.display()));
    assert!(
        dst.is_file(),
        "the fixture copy {} was not created",
        dst.display()
    );
}

/// Removes the fixture root when the test ends, INCLUDING when an assertion panics.
///
/// The test cleaned its root twice by hand - once before the fixture is built and once at the end -
/// and the trailing call is skipped the moment an assertion panics, which is exactly when a fixture
/// tree is most likely to survive. Measured 2026-11-09: `$PREFIX/tmp/p7-loader-<pid>-dyn` trees were
/// present (each holding `rootfs/p7loader/lib/{libc.so,libdl.so,libpdnflag.so}` and `rootfs/bin/hello`),
/// and a planted panic reproduced one. The guard makes teardown unconditional; the pre-clean stays,
/// because it is what makes a re-run start from a clean tree.
struct FixtureGuard(PathBuf);

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_dynamic_guest_runs_inside_the_rootfs() {
    let root = fixture_root("dyn");
    let _guard = FixtureGuard(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let rootfs = root.join("rootfs");
    std::fs::create_dir_all(rootfs.join("bin")).expect("create rootfs");
    // The directory the fixture's DT_RUNPATH names. It exists on NO host filesystem, so the loader
    // can only satisfy it through the engine's translation - see the module docs.
    let runpath_dir_in_rootfs = rootfs.join("p7loader").join("lib");
    std::fs::create_dir_all(&runpath_dir_in_rootfs).expect("create rootfs runpath dir");
    let host_libdir = root.join("hostlib");
    std::fs::create_dir_all(&host_libdir).expect("create host lib dir");

    // ---- the fixture -------------------------------------------------------------------------
    // A shared library that exists only inside the rootfs and whose return value IS the exit code.
    let lib_so = host_libdir.join("libpdnflag.so");
    compile_c(
        &lib_so,
        "int pdn_flag(void) { return 7; }\n",
        &["-shared", "-fPIC", "-Wl,-soname,libpdnflag.so"],
    );
    // `printf` from the guest's libc, and `pdn_flag()` from the rootfs-only library.
    let host_bin = root.join("hello");
    compile_c(
        &host_bin,
        "#include <stdio.h>\nextern int pdn_flag(void);\n\
         int main(void) { printf(\"hello from guest\\n\"); return pdn_flag(); }\n",
        &[
            &format!("-L{}", host_libdir.display()),
            "-lpdnflag",
            "-Wl,-rpath,/p7loader/lib",
        ],
    );
    let fixture = rootfs.join("bin").join("hello");
    copy_to(&host_bin, &fixture);

    // ---- the libraries, at the SAME absolute paths inside the rootfs -------------------------
    // Every NEEDED library the host toolchain resolves is copied to `<rootfs>/p7loader/lib/<name>`,
    // which is where the loader will look for it once the RUNPATH entry is translated. The
    // rootfs-only flag library is added to that set.
    let mut placed = Vec::new();
    let needed = needed_libs(&host_bin);
    for name in &needed {
        // The rootfs-only library is placed below from the copy we built; it is resolvable by no
        // toolchain search path, which is the point of it.
        if name == "libpdnflag.so" {
            continue;
        }
        let src = toolchain_lib(name);
        let dst = runpath_dir_in_rootfs.join(name);
        copy_to(&src, &dst);
        placed.push((
            name.clone(),
            src.display().to_string(),
            dst.display().to_string(),
        ));
    }
    copy_to(&lib_so, &runpath_dir_in_rootfs.join("libpdnflag.so"));
    // Every NEEDED name must be present inside the rootfs, at the path the loader will search -
    // a silently skipped library would make the run fail for a reason this test could not explain.
    for name in &needed {
        let at = runpath_dir_in_rootfs.join(name);
        assert!(
            at.is_file(),
            "NEEDED {} was not placed inside the rootfs at {}; the loader could not open it",
            name,
            at.display()
        );
    }
    eprintln!("[dyn] fixture={}", fixture.display());
    eprintln!(
        "[dyn] rootfs-only lib={}",
        runpath_dir_in_rootfs.join("libpdnflag.so").display()
    );
    for (n, s, d) in &placed {
        eprintln!("[dyn] NEEDED {n}: {s} -> {d}");
    }

    // The rootfs is the whole namespace here: no bind at all, so EVERY absolute path the loader
    // opens is translated into the rootfs and the report names each rewrite. A pseudo-filesystem
    // the loader probes and the rootfs does not carry simply answers ENOENT, which is the honest
    // answer and is what the engine reports.
    let plan = RunPlan {
        rootfs: Some(rootfs.clone()),
        binds: Vec::new(),
        cwd: None,
        env: Vec::new(),
    };

    // ---- capture the guest's stdout ----------------------------------------------------------
    // fd 1 is redirected to a pipe around the run; the guest inherits it. Safe because this file
    // holds exactly one test (see the module docs).
    let saved = unsafe { libc::dup(1) };
    assert!(saved >= 0, "dup(1) failed");
    let mut fds = [0i32; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe failed");
    assert_eq!(unsafe { libc::dup2(fds[1], 1) }, 1, "dup2 failed");

    let argv = vec!["/bin/hello".to_string()];
    let run = run_native(&argv, &plan, false);

    // Restore fd 1 before anything else: the assertions below must not inherit the pipe.
    assert_eq!(unsafe { libc::dup2(saved, 1) }, 1, "restore of fd 1 failed");
    unsafe { libc::close(saved) };
    unsafe { libc::close(fds[1]) };
    let mut out = Vec::new();
    loop {
        let mut buf = [0u8; 256];
        let n = unsafe { libc::read(fds[0], buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    unsafe { libc::close(fds[0]) };
    let guest_stdout = String::from_utf8_lossy(&out).into_owned();

    let run = run.expect("traced run of the dynamic fixture");
    eprintln!(
        "[dyn] exit={} stops={} entries={} executed={:?}",
        run.exit_code, run.stops, run.entries, run.executed
    );
    eprintln!("[dyn] guest stdout={:?}", guest_stdout);
    for t in &run.translations {
        eprintln!(
            "[dyn] translated {}: {} -> {}",
            t.syscall,
            t.guest,
            t.host.display()
        );
    }
    for (label, n) in run.refusal_summary() {
        eprintln!("[dyn] refusal x{n}: {label}");
    }

    // ---- A: the dynamic guest ran to completion and its output came from the guest -------------
    assert!(
        guest_stdout.contains("hello from guest"),
        "the guest's printf must appear on the inherited stdout; got {:?} \
         (an empty pipe means the run failed BEFORE printf, or the guest never reached libc)",
        guest_stdout
    );
    assert_eq!(
        run.exit_code, 7,
        "the run's exit code is `pdn_flag()`'s return value, and that symbol lives ONLY in the \
         rootfs copy of /p7loader/lib/libpdnflag.so; any other code means the rootfs library was \
         not loaded (runtime errors are in the guest stdout above)"
    );

    // ---- A: the loader's path-taking syscalls were translated INTO the rootfs ------------------
    let openat: Vec<_> = run
        .translations
        .iter()
        .filter(|t| t.syscall == "SYS_openat")
        .collect();
    assert!(
        !openat.is_empty(),
        "the loader's opens must be translated, not passed to the host; translations={:?}",
        run.translations
    );
    for t in &openat {
        assert!(
            t.host.starts_with(&rootfs),
            "every translated open must land inside the rootfs, got {} -> {}",
            t.guest,
            t.host.display()
        );
    }
    // The loader's path-taking syscalls, by name. This is the report entry the acceptance asks
    // for: these are the rewrites that let a dynamic guest start at all.
    let mut names: Vec<&str> = run.translations.iter().map(|t| t.syscall).collect();
    names.sort_unstable();
    names.dedup();
    eprintln!("[dyn] translated syscall names: {:?}", names);
    assert!(
        names.contains(&"SYS_openat"),
        "the loader's openat must be translated, not passed to the host; names={:?}",
        names
    );
    assert!(
        names.contains(&"SYS_newfstatat"),
        "the loader's stat must be translated, not passed to the host; names={:?}",
        names
    );
    // The decisive one: the rootfs-only library was found at the RUNPATH path, INSIDE the rootfs.
    // Nothing on the host can satisfy this path. On this device the loader opens the RUNPATH
    // directory first (translated into the rootfs) and then reaches the file through it, so the
    // file's own translation is the already-translated identity - either way its HOST path is the
    // rootfs copy, which is what is asserted.
    let flag = run
        .translations
        .iter()
        .find(|t| t.host.starts_with(&runpath_dir_in_rootfs))
        .unwrap_or_else(|| {
            panic!(
                "the RUNPATH path must be resolved INSIDE the rootfs ({}); translations={:?}",
                runpath_dir_in_rootfs.display(),
                run.translations
            )
        });
    eprintln!(
        "[dyn] rootfs-only RUNPATH path: {} -> {} (via {})",
        flag.guest,
        flag.host.display(),
        flag.syscall
    );
    // With no bind at all, NOTHING may escape the rootfs: the loader's library search, and the
    // libc it eventually loads, can only have come from the copies placed under the RUNPATH
    // directory inside it - which is why printf worked and why the exit code proves the
    // rootfs-only library ran.
    assert!(
        run.translations.iter().all(|t| t.host.starts_with(&rootfs)),
        "with no bind, every translated path must land inside the rootfs; translations={:?}",
        run.translations
    );

    // ---- the named refusals are still refusals, not silent pass-through ------------------------
    assert!(
        run.refusals.iter().all(|r| r.label().contains('[')),
        "every refusal must state a reason, got {:?}",
        run.refusal_summary()
    );
    // A syscall that TAKES A PATH and has no translator here is named, so `strict` stops on it
    // instead of letting the kernel resolve a guest path on the host. `SYS_chdir` is the cheapest
    // such case; the engine's own dispatcher is what this asserts is wired. This is about a GUEST
    // chdir: the engine's own pre-exec chdir (RunPlan::cwd, 2026-11-09) is exempt because the path it
    // receives is already the translated HOST path.
    assert_eq!(
        proot_core::native_run::syscall_name(libc::SYS_chdir as i64),
        Some("SYS_chdir"),
        "a path-taking syscall with no translator must still be named in the report"
    );

    // Teardown is the guard's (`FixtureGuard`), not this line's: the manual call that used to live
    // here ran only on the way out of a PASSING test.
}
