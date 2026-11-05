use std::process::Command;

fn podnext() -> Command {
    Command::new(env!("CARGO_BIN_EXE_podnext"))
}

#[test]
fn test_sessions_json_output_clean() {
    let output = podnext()
        .args(["sessions", "-f", "json"])
        .output()
        .expect("failed to run podnext sessions");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout must be strictly parseable JSON
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout was not valid json: {}\nerror: {}", stdout, e));
    assert!(parsed.is_array());

    // The human note must be on stderr, not stdout, and it must describe what `attach` actually
    // does: reconnect to a session started with `run --detach`. It never creates one, so the note
    // must not claim otherwise.
    assert!(!stdout.contains("podnext attach"));
    assert!(
        stderr.contains("podnext attach") && stderr.contains("reconnect"),
        "the sessions note belongs on stderr and must say attach reconnects; stderr was: {stderr:?}"
    );
    assert!(
        !stderr.contains("create a new session"),
        "the sessions note must not tell the caller attach creates a session; stderr was: {stderr:?}"
    );
}

#[test]
fn test_gitops_status_json_output_clean() {
    let output = podnext()
        .args(["gitops", "status", "-f", "json"])
        .output()
        .expect("failed to run podnext gitops status");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout must be strictly parseable JSON
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout was not valid json: {}\nerror: {}", stdout, e));
    assert!(parsed.is_array());
    assert!(!stdout.contains("No applications found"));
}

#[test]
fn test_non_tty_stdin_refused_without_hang() {
    use std::process::Stdio;

    let mut child = podnext()
        .args(["remove", "nonexistent_target_12345"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn podnext");

    let status = child.wait().expect("failed to wait on child");
    assert_eq!(status.code(), Some(1));
}

#[test]
fn test_unknown_targets_exit_1() {
    for args in [
        vec!["kill", "nosuch_container_99999"],
        vec!["service", "logs", "nosuch_service_99999"],
        vec!["quota", "set", "nosuch_service_99999", "memory", "512M"],
        vec!["boot", "enable", "nosuch_service_99999"],
        vec!["remove", "nosuch_container_99999"],
        vec!["volume", "rm", "nosuch_volume_99999"],
        vec!["image", "rm", "nosuch_image_99999"],
    ] {
        let output = podnext()
            .args(&args)
            .output()
            .unwrap_or_else(|e| panic!("failed to run {:?}: {}", args, e));

        assert_eq!(
            output.status.code(),
            Some(1),
            "expected exit 1 for args {:?}, got {:?}",
            args,
            output.status.code()
        );
    }
}

#[test]
fn test_exit_code_unification() {
    // 0: success
    let ok = podnext().arg("--version").output().unwrap();
    assert_eq!(ok.status.code(), Some(0));

    // 1: application / entity not found
    let not_found = podnext()
        .args(["kill", "nonexistent_container"])
        .output()
        .unwrap();
    assert_eq!(not_found.status.code(), Some(1));

    // 2: clap usage error
    let clap_err = podnext()
        .args(["list", "--bogus-flag-12345"])
        .output()
        .unwrap();
    assert_eq!(clap_err.status.code(), Some(2));

    // 2: an unloadable configuration file is the same usage-error class as a missing argument.
    // There is no separate "config error" code: the taxonomy (roadmap section 9.1, asserted in
    // tests/exit_codes.rs) is 0 success / 1 documented refusal or domain error / 2 usage error,
    // plus the guest's own status. cli-reference.md documents 2 for an invalid file and
    // tests/cli_reference_contract.rs forbids documenting any code above 2.
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "invalid_toml = = =").unwrap();
    let config_err = podnext()
        .args(["config", "validate", &tmp.path().to_string_lossy()])
        .output()
        .unwrap();
    assert_eq!(config_err.status.code(), Some(2));
}

#[test]
fn test_yes_flag_on_volume_rm() {
    // Create volume
    let vol_name = "test_yes_vol_auto";
    let _ = podnext().args(["volume", "create", vol_name]).output();

    // Remove with --yes
    let rm = podnext()
        .args(["volume", "rm", vol_name, "--yes"])
        .output()
        .unwrap();
    assert_eq!(rm.status.code(), Some(0));
}
