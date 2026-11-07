//! Base-tree negative control for S2 (adoption of `shroot`'s identity/credential syscall table).
//!
//! This file deliberately uses only the surface that exists on the base tree (f2f7a5f): it spawns
//! the real `podnext doctor` binary and parses the JSON, exactly as `doctor_capabilities.rs` does,
//! so the same file compiles and runs there. On the base tree `doctor` reports no row naming the
//! guest's identity: the identity syscalls are implicit in the runtime's `--change-id=0:0:0` argv
//! and named nowhere a caller can read, so the row lookup below fails there. With the table named
//! in `proot_core::identity` and reported by the probe, it passes.

use std::process::Command;

fn bin() -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(|p| p.parent())
        .expect("target profile dir")
        .join("podnext")
}

#[test]
fn doctor_names_the_guest_identity_policy_and_the_size_of_the_table() {
    let out = Command::new(bin())
        .args(["doctor", "--format", "json"])
        .output()
        .expect("run doctor");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out.stdout))
        .expect("doctor --format json must be a JSON array");

    let row = rows
        .iter()
        .find(|r| r.get("name").and_then(|n| n.as_str()) == Some("guest identity"))
        .unwrap_or_else(|| {
            panic!(
                "doctor must report the guest identity policy; a policy named only in the \
                 runtime's argv is the S2 defect - there is no row a caller can read to see \
                 which identity syscalls PDN handles"
            )
        });

    let detail = row.get("detail").and_then(|d| d.as_str()).expect("detail");
    assert!(
        detail.contains("18"),
        "the row must carry the size of the named table, not leave it implied: {detail:?}"
    );
    assert!(
        detail.contains("0:0:0"),
        "the row must name the mapping that makes the guest identity: {detail:?}"
    );
}
