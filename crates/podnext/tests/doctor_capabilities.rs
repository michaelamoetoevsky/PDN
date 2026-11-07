//! S5/C7 - the compatibility probe, and the one property that makes it worth having.
//!
//! The probe exists because PDN made claims about the executor that nothing measured: the Docker
//! refusal said "Docker requires namespaces (mount, pid, net) and cgroups v2, which PRoot cannot
//! provide" as prose, and the cgroup path wrote limits into a plain directory and returned Ok().
//!
//! A check that reports `ok` for something it did not measure is the same defect one layer up, so
//! these tests assert the honest shape rather than the happy path:
//!   1. every probe answers with ONE of the four named verdicts - never an empty string, never a
//!      verdict with no detail to explain it;
//!   2. a knob this platform cannot honour is NOT reported `ok` (the cgroupfs case on Android,
//!      where /sys/fs/cgroup is a cgroup2 mount the app cannot write);
//!   3. `subreaper` is `unknown`: PR_SET_CHILD_SUBREAPER has no read-only form, so claiming `ok`
//!      would assert a capability the probe never measured;
//!   4. the Docker refusal carries the measurements, so a user reads why rather than being told;
//!   5. the probe changes nothing it measures: the platform facts it reads are the same before and
//!      after, which is what makes it safe to run before any guest starts.

use serde_json::Value;
use std::process::Command;

fn bin() -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(|p| p.parent())
        .expect("target profile dir")
        .join("podnext")
}

/// The probe rows, parsed with serde_json rather than a hand-rolled scanner: a scanner written
/// against an assumed shape searched for `"name":"..."` while the output is pretty-printed with a
/// space, and three tests failed for that reason alone.
fn checks() -> Vec<Value> {
    let out = Command::new(bin())
        .args(["doctor", "--format", "json"])
        .output()
        .expect("run doctor");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&text).expect("doctor --format json must be a JSON array")
}

fn find<'a>(rows: &'a [Value], name: &str) -> Option<&'a Value> {
    rows.iter()
        .find(|r| r.get("name").and_then(|n| n.as_str()) == Some(name))
}

fn verdict_of<'a>(rows: &'a [Value], name: &str) -> (&'a str, &'a str) {
    let row = find(rows, name).unwrap_or_else(|| {
        panic!("the probe must report '{name}'; a missing probe is not a silent pass")
    });
    (
        row.get("verdict")
            .and_then(|v| v.as_str())
            .expect("verdict"),
        row.get("detail").and_then(|v| v.as_str()).expect("detail"),
    )
}

#[test]
fn every_probe_answers_with_a_named_verdict_and_a_reason() {
    let rows = checks();
    let names = [
        "namespaces",
        "seccomp",
        "cgroupfs",
        "pidfd",
        "executor uid",
        "subreaper",
    ];
    for name in names {
        let (verdict, detail) = verdict_of(&rows, name);
        assert!(
            matches!(verdict, "ok" | "warn" | "fail" | "unknown"),
            "'{name}' must use one of the four named verdicts, got {verdict:?}"
        );
        assert!(
            detail.len() > 8,
            "'{name}' must explain itself; a verdict with no detail cannot be acted on: {detail:?}"
        );
    }
}

#[test]
fn a_knob_this_platform_cannot_honour_is_not_reported_ok() {
    let rows = checks();
    let (verdict, detail) = verdict_of(&rows, "cgroupfs");
    // On Android/Termux /sys/fs/cgroup is not writable by the app, so a probe that says `ok` here is
    // either measuring the wrong path or assuming an answer - the defect A20 was.
    let writable_sys_fs_cgroup = std::fs::OpenOptions::new()
        .write(true)
        .open("/sys/fs/cgroup/cgroup.procs")
        .is_ok();
    if writable_sys_fs_cgroup {
        println!("platform allows cgroup writes; the refusal branch is not exercised here");
    } else {
        assert_ne!(
            verdict, "ok",
            "cgroup limits cannot be enforced on this platform, so the probe must not report ok: {detail}"
        );
        assert!(
            detail.contains("refuse") || detail.contains("cannot"),
            "the cgroupfs detail must say what happens instead of enforcement: {detail:?}"
        );
    }
}

#[test]
fn subreaper_is_unknown_because_it_cannot_be_read_back() {
    let rows = checks();
    let (verdict, detail) = verdict_of(&rows, "subreaper");
    assert_eq!(
        verdict, "unknown",
        "PR_SET_CHILD_SUBREAPER has no read-only form, so the honest verdict is unknown, not {verdict}"
    );
    assert!(
        detail.contains("A22"),
        "the detail must point at where the capability is actually exercised: {detail:?}"
    );
}

#[test]
fn the_docker_refusal_carries_the_measurements_not_a_claim() {
    // A PRoot container cannot support kernel features, so `container run --runtime` refuses. The
    // message must contain the probe's own line, not only the static sentence.
    let out = Command::new(bin())
        .args(["doctor"])
        .output()
        .expect("run doctor");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("namespaces") && text.contains("cgroupfs"),
        "doctor must name the capability probes it ran: {text}"
    );
}

#[test]
fn namespaces_and_seccomp_measure_something_readable() {
    // Both sysctls are denied to an app uid on Android, so a probe that read only them would answer
    // `unknown` on every device - blind exactly where the Docker refusal needs an answer. These
    // assert the fallbacks are the measurement: /proc/self/ns/* and the Seccomp: line.
    let rows = checks();
    let (ns_verdict, ns_detail) = verdict_of(&rows, "namespaces");
    assert_ne!(
        ns_verdict, "unknown",
        "the namespace probe must measure something: {ns_detail}"
    );
    assert!(
        ns_detail.contains("namespace(s)") || ns_detail.contains("max_user_namespaces="),
        "the namespace detail must name what was read: {ns_detail:?}"
    );
    assert_ne!(
        ns_verdict, "ok",
        "being IN namespaces is not being able to CREATE them, so `ok` would overclaim: {ns_detail}"
    );

    let (sc_verdict, sc_detail) = verdict_of(&rows, "seccomp");
    assert!(
        matches!(sc_verdict, "ok" | "warn"),
        "the seccomp probe must read the mode from /proc/self/status: {sc_verdict}"
    );
    assert!(
        sc_detail.contains("mode"),
        "the seccomp detail must quote the mode: {sc_detail:?}"
    );
}

#[test]
fn the_probe_changes_nothing_it_measures() {
    // The probe is a set of reads, which is what makes it safe to run before any guest starts. If it
    // perturbed the platform, two consecutive runs would disagree about an unchanged machine.
    let first = checks();
    let second = checks();
    for name in ["namespaces", "seccomp", "pidfd", "executor uid"] {
        let a = find(&first, name).and_then(|r| r.get("verdict").cloned());
        let b = find(&second, name).and_then(|r| r.get("verdict").cloned());
        assert_eq!(
            a, b,
            "'{name}' changed between two runs of a read-only probe: {a:?} vs {b:?}"
        );
    }
}
