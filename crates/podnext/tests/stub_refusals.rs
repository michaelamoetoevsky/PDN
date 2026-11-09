//! Roadmap section 9.1: a command that cannot do its job must refuse, name the reason, and exit
//! non-zero. `build submit|status|cancel|list|artifacts|logs` and `watchdog events` used to print
//! "(not yet implemented)" on stdout and return `Ok(())` — exit 0 for something that did nothing,
//! which a script cannot distinguish from success.

use std::process::Command;

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_podnext"))
        .args(args)
        .output()
        .expect("spawn podnext");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn build_service_subcommands_refuse_with_a_reason() {
    let cases: [&[&str]; 6] = [
        &["build", "submit", "--spec", "spec.yaml"],
        &["build", "status", "b1"],
        &["build", "cancel", "b1"],
        &["build", "list"],
        &["build", "artifacts", "b1"],
        &["build", "logs", "b1"],
    ];
    for args in cases {
        let (code, stdout, stderr) = run(args);
        assert_eq!(
            code, 1,
            "{args:?} must exit 1 (refusal), got {code}; stdout={stdout:?} stderr={stderr:?}"
        );
        assert!(
            stderr.contains("is not implemented"),
            "{args:?} must name the reason on stderr: {stderr:?}"
        );
        assert!(
            !stdout.contains("not yet implemented") && !stdout.contains("not implemented"),
            "{args:?} must not report the non-action on stdout: {stdout:?}"
        );
    }
}

#[test]
fn watchdog_events_refuses_with_a_reason() {
    let (code, stdout, stderr) = run(&["watchdog", "events"]);
    assert_eq!(code, 1, "stdout={stdout:?} stderr={stderr:?}");
    assert!(stderr.contains("is not implemented"), "stderr={stderr:?}");
    assert!(
        !stdout.contains("Event streaming"),
        "the stub's stdout line is gone: {stdout:?}"
    );
}

#[test]
fn attach_without_a_running_session_refuses_with_a_reason() {
    // PDN#57: `attach` reconnects to a running detached session and never starts one, so with
    // nothing running it must say so and exit 1 - not silently open a fresh shell.
    let (code, stdout, stderr) = run(&["attach", "no-such-container-xyz"]);
    assert_eq!(code, 1, "stdout={stdout:?} stderr={stderr:?}");
    assert!(
        stderr.contains("No running session"),
        "the refusal must name the state it found: {stderr:?}"
    );
    assert!(
        !stderr.contains("not fully implemented"),
        "the old warning must be gone: {stderr:?}"
    );
}

/// `run` accepted nine options and dropped every one of them, so the command that ran was not
/// the one the caller asked for. They are refused by name now, and the refusal names the flag.
#[test]
fn run_refuses_the_options_it_cannot_honour() {
    let cases: [&[&str]; 8] = [
        &["run", "no-such-container-xyz", "--rm", "--", "true"],
        &[
            "run",
            "no-such-container-xyz",
            "--name",
            "other",
            "--",
            "true",
        ],
        &[
            "run",
            "no-such-container-xyz",
            "--network",
            "net1",
            "--",
            "true",
        ],
        &[
            "run",
            "no-such-container-xyz",
            "-p",
            "8080:80",
            "--",
            "true",
        ],
        &[
            "run",
            "no-such-container-xyz",
            "--entrypoint",
            "/bin/sh",
            "--",
            "true",
        ],
        &["run", "no-such-container-xyz", "-u", "1000", "--", "true"],
        &[
            "run",
            "no-such-container-xyz",
            "--workdir",
            "/tmp",
            "--",
            "true",
        ],
        // Issue #62: this one was parsed and dropped, so "accepted" and "enforced" were the same
        // word. It refuses like the rest now.
        &["run", "no-such-container-xyz", "--seccomp", "--", "true"],
    ];
    for args in cases {
        let (code, stdout, stderr) = run(args);
        assert_eq!(
            code, 1,
            "{args:?} must be refused: stdout={stdout:?} stderr={stderr:?}"
        );
        let flag = args.iter().find(|a| a.starts_with('-')).expect("a flag");
        assert!(
            stderr.contains(*flag),
            "{args:?} must name the option it refuses: {stderr:?}"
        );
        assert!(
            stdout.is_empty(),
            "{args:?} must not print a run header before refusing: {stdout:?}"
        );
    }
}

/// The two compatibility flags stay accepted, because stdio is inherited and they describe it.
#[test]
fn run_still_accepts_tty_and_interactive() {
    // No container needed: reaching the container lookup proves the flags were not refused.
    let (code, _stdout, stderr) = run(&["run", "no-such-container-xyz", "-t", "-i", "--", "true"]);
    assert_eq!(code, 1, "an unknown container still fails");
    assert!(
        !stderr.contains("not implemented"),
        "-t/-i must not be refused: {stderr:?}"
    );
    assert!(
        stderr.contains("not found") || stderr.contains("no-such-container-xyz"),
        "the failure must be the container lookup: {stderr:?}"
    );
}

/// Real use reported `podnext login <ctr> --shared-home` answering clap's
/// `error: unexpected argument '--shared-home' found` — a parse error that reads as a typo and says
/// nothing about what to do instead. The proot-distro login polygon PDN does not implement is now
/// accepted by the parser and refused by name, so the caller gets the flag, the reason and a command
/// that works. A genuine typo must still fail at parse time, which is asserted too, so accepting these
/// flags did not swallow mistakes.
#[test]
fn login_refuses_the_unimplemented_proot_flags_by_name_with_a_remedy() {
    // (argument to pass, name the message must carry, remedy to look for). The first two differ for a
    // flag that takes a value: the caller types `--change-id=0:0:0` and the refusal names `--change-id`.
    // Only flags that were a PARSE error before this change: `--kill-on-exit`, `--no-kill-on-exit` and
    // `--no-sysvipc` are global options already refused by name elsewhere, so they are deliberately not
    // duplicated here (one source of truth per flag).
    let cases: [(&str, &str, &str); 5] = [
        ("--shared-home", "--shared-home", "-b"),
        ("--shared-tmp", "--shared-tmp", "-b"),
        ("--termux-home", "--termux-home", "-b"),
        ("--detach", "--detach", "podnext run --detach"),
        ("--change-id=0:0:0", "--change-id", "--engine proot"),
    ];
    for (arg, flag, remedy) in cases {
        let (code, _stdout, stderr) = run(&["login", "no-such-container", arg]);
        assert_ne!(code, 2, "{arg} must not read as a parse error: {stderr}");
        assert_eq!(code, 1, "{arg} must refuse with exit 1: {stderr}");
        assert!(
            stderr.contains(flag),
            "the message must name {flag}: {stderr}"
        );
        assert!(
            stderr.contains(remedy),
            "the message for {flag} must carry its remedy ({remedy}): {stderr}"
        );
    }

    // The typo control: one character off a real flag is still a parse error, so the change above
    // explains the flags PDN knows about without inventing behaviour for the ones it does not.
    let (code, _stdout, stderr) = run(&["login", "no-such-container", "--shared-hom"]);
    assert_eq!(code, 2, "a typo must still be a parse error: {stderr}");
    assert!(stderr.contains("unexpected argument"), "{stderr}");
}

/// The installed `proot --help` gives `-0` to `--root-id` ("make current user appear as root and fake
/// its privileges") and `-l` to `--link2symlink`. PDN had `-0` on `--link2symlink`, so a reader of
/// the proot documentation asked for root identity and silently got hardlink emulation. PDN's guest
/// is already root - `--change-id=0:0:0` is passed unconditionally, measured with a decoy PROOT_BIN:
/// 139 argv entries, byte-identical with and without the flag, and `proot -0` on this device reports
/// uid 0 where plain proot reports uid 10396 - so `-0` and `--root-id` now refuse by name, under the
/// reference's own spelling.
#[test]
fn root_id_refuses_by_name_because_the_guest_is_already_root() {
    for arg in ["-0", "--root-id"] {
        let (code, _stdout, stderr) = run(&["run", "no-such-container", arg, "--", "/bin/sh"]);
        assert_ne!(
            code, 2,
            "{arg} must be parsed, not read as a typo: {stderr}"
        );
        assert_eq!(code, 1, "{arg} must refuse with exit 1: {stderr}");
        assert!(
            stderr.contains("--root-id"),
            "the message must name the flag in the reference's spelling: {stderr}"
        );
        assert!(
            stderr.contains("--change-id=0:0:0"),
            "and the measured reason: {stderr}"
        );
    }

    // The short flag that moved must not collide with a subcommand's own: `daemon logs` owns `-l`,
    // and making `-l` global made clap panic for that one command only ("Short option names must be
    // unique for each argument, but '-l' is in use by both 'lines' and 'link2symlink'"). The
    // help-dump gate saw it as a screen printing <<exit 101>>; this is the same check at unit speed.
    let (code, _stdout, stderr) = run(&["daemon", "logs", "--help"]);
    assert_eq!(code, 0, "every subcommand's help must render: {stderr}");
}
