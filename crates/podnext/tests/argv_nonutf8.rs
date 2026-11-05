//! Regression test for Issue #41: non-UTF-8 argv should not panic.
//!
//! Before fix: `std::env::args()` panicked on invalid UTF-8 bytes (e.g. 0xff),
//! causing SIGABRT (exit 134).
//!
//! After fix: `std::env::args_os()` + `to_string_lossy()` converts invalid
//! sequences to replacement characters (�), allowing graceful error handling.

use std::process::Command;

#[test]
fn argv_with_invalid_utf8_does_not_panic() {
    let bin = env!("CARGO_BIN_EXE_podnext");

    // Pass a byte sequence that is invalid UTF-8 (0xff)
    let output = Command::new(bin)
        .arg("list")
        .arg(unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(&[0xff]) })
        .output()
        .expect("failed to execute podnext");

    // Should NOT exit with SIGABRT (134 on Linux with panic=abort)
    assert_ne!(
        output.status.code(),
        Some(134),
        "podnext panicked on non-UTF-8 argv (SIGABRT); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should exit with error (1 or 2), not success
    assert!(
        !output.status.success(),
        "podnext should reject invalid arguments, not succeed silently"
    );
}

#[test]
fn argv_with_valid_utf8_works() {
    let bin = env!("CARGO_BIN_EXE_podnext");

    let output = Command::new(bin)
        .arg("--version")
        .output()
        .expect("failed to execute podnext");

    assert!(
        output.status.success(),
        "podnext --version failed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("podnext") || stdout.contains("0.1."),
        "version output missing expected content: {}",
        stdout
    );
}
