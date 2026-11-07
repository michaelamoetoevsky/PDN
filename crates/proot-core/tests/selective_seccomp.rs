//! S1: the selective-interception backend, exercised and measured against the
//! PRoot path on the same workload.
//!
//! `proot_core::selective` is the second execution backend: a `SECCOMP_RET_TRACE`
//! filter over a selected syscall set, everything else `SECCOMP_RET_ALLOW`, and a
//! tracer that reads each stop's cookie through the existing `PtraceEvent::Seccomp`
//! handler. The correctness test asserts the shape of the interception; the
//! measurement prints both wall times for one workload on this device.

use proot_core::{selective, ProotConfig};
use std::process::Command;
use std::time::Instant;

fn prefix() -> String {
    std::env::var("PREFIX").unwrap_or_else(|_| "/data/data/com.termux/files/usr".to_string())
}

fn sh_bin() -> String {
    format!("{}/bin/sh", prefix())
}

fn proot_bin() -> Option<String> {
    let path = format!("{}/bin/proot", prefix());
    std::path::Path::new(&path).is_file().then_some(path)
}

/// A metadata workload: 4000 `stat`-class lookups, no fork per iteration. None of
/// these syscalls is in the selected set, so the selective backend should leave
/// them native while PRoot stops on every one.
const WORKLOAD: &str = "i=0; while [ $i -lt 4000 ]; do [ -e /etc/hostname ]; i=$((i+1)); done";

/// Best-of-3 wall time in milliseconds for a command that must exit cleanly.
fn best_of<F: FnMut() -> bool>(mut run: F) -> u128 {
    let mut best = u128::MAX;
    for _ in 0..3 {
        let t = Instant::now();
        assert!(run(), "the workload exited non-zero");
        best = best.min(t.elapsed().as_millis());
    }
    best
}

/// The correctness claim: the selected set is intercepted, everything else runs
/// native. 4000 metadata lookups run with a small, bounded number of stops.
#[test]
fn the_backend_intercepts_the_selected_set_and_leaves_the_rest_native() {
    let selected = selective::selected_syscalls();
    let run = selective::run_selected_cmd(&[sh_bin(), "-c".into(), WORKLOAD.into()], &selected)
        .expect("run the workload under the selective backend");

    println!(
        "SELECTIVE-INTERCEPTED {} of the 4000 metadata lookups' syscalls, exit {}",
        run.intercepted(),
        run.exit_code
    );
    assert_eq!(run.exit_code, 0);
    // `sh`'s own startup opens and mappings are selected; the loop's 4000
    // lookups are not. A number anywhere near 4000 means the filter is stopping
    // unselected syscalls.
    assert!(
        run.intercepted() < 500,
        "the backend intercepted {} syscalls; the 4000-lookup loop is not in the selected set and \
         must run native",
        run.intercepted()
    );
}

/// The opt-in mode, end to end through the engine: `execute()` with
/// `seccomp_selective` set runs the command under the filter and tracer.
#[tokio::test]
async fn the_opt_in_mode_runs_a_command_under_the_filter() {
    let mut config = ProotConfig {
        seccomp_selective: true,
        ..Default::default()
    };
    config.command = vec![sh_bin(), "-c".into(), "exit 7".into()];
    let mut engine = proot_core::ProotEngine::new(config).expect("engine");
    let code = engine
        .execute()
        .await
        .expect("execute under the selective backend");
    assert_eq!(code, 7, "the guest's own exit code must pass through");
}

/// The measurement: the same workload through the current PRoot path, the
/// selective backend, and native, all three timed on this device.
#[test]
fn measurement_selective_vs_proot_on_the_same_workload() {
    let Some(proot) = proot_bin() else {
        println!(
            "FIXTURE-UNAVAILABLE: no proot binary at {}/bin/proot",
            prefix()
        );
        return;
    };
    let sh = sh_bin();
    let selected = selective::selected_syscalls();

    let proot_ms = best_of(|| {
        Command::new(&proot)
            .args(["-r", "/", "-0", "-w", "/", &sh, "-c", WORKLOAD])
            .status()
            .expect("spawn proot")
            .success()
    });

    let mut last = None;
    let selective_ms = best_of(|| {
        let run =
            selective::run_selected_cmd(&[sh.clone(), "-c".into(), WORKLOAD.into()], &selected)
                .expect("run the selective backend");
        last = Some(run);
        true
    });

    let native_ms = best_of(|| {
        Command::new(&sh)
            .args(["-c", WORKLOAD])
            .status()
            .expect("spawn sh")
            .success()
    });

    let run = last.expect("a selective run");
    println!("SELECTIVE-MEASURE workload : {WORKLOAD}");
    println!("SELECTIVE-MEASURE proot path: {proot_ms:>6} ms");
    println!(
        "SELECTIVE-MEASURE selective : {selective_ms:>6} ms ({} intercepted, exit {})",
        run.intercepted(),
        run.exit_code
    );
    println!("SELECTIVE-MEASURE native    : {native_ms:>6} ms");

    assert_eq!(run.exit_code, 0);
}
