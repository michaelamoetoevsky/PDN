//! The argv quoting table (roadmap section 9.3).
//!
//! What the caller writes must arrive at the guest as the caller meant. The interesting cases are
//! the ones where a shell is usually in the way: `$HOME` must not be expanded, `$(id)` must not be
//! run, `*` must not glob, a quote must stay a quote, and a newline must survive inside a single
//! argument.
//!
//! A measurement note worth keeping, because it cost a wrong conclusion: when this was first
//! probed from a shell, `$(id)` arrived at the guest already expanded - by the *host shell* in the
//! probe, not by podnext. Any test of this property that goes through a shell proves nothing about
//! the tool. Here every argument is handed over as a literal `OsStr` by `Command::arg`, with no
//! shell anywhere in the path, and the guest reports what it received NUL-separated so an argument
//! containing a newline cannot be confused with two arguments.
//!
//! Skips with a printed reason when no container is installed: an unrunnable check is not a pass.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

/// The first installed container, preferring `alpine` because it starts fastest. Parsed out of
/// `list --format json` with a substring scan rather than by adding a JSON dependency to a test.
fn pick_container() -> Option<String> {
    let out = Command::new(bin())
        .args(["list", "--format", "json"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut names = Vec::new();
    for chunk in text.split("\"name\": \"").skip(1) {
        if let Some(end) = chunk.find('"') {
            names.push(chunk[..end].to_string());
        }
    }
    names
        .iter()
        .find(|n| n.as_str() == "alpine")
        .cloned()
        .or_else(|| names.into_iter().next())
}

#[test]
fn arguments_arrive_at_the_guest_exactly_as_written() {
    let container = match pick_container() {
        Some(c) => c,
        None => {
            eprintln!(
                "skipped: no container is installed on this host, so the guest cannot report its \
                 argv; install one (podnext install alpine) to run this table"
            );
            return;
        }
    };

    let cases: Vec<&str> = vec![
        "plain",
        "two words",
        "single'quote",
        "double\"quote",
        "semi;colon",
        "$HOME and ${PATH}",
        "$(id) and `id`",
        "-leading-dash",
        "--looks-like-a-flag",
        "*",
        "",
        "line one\nline two",
        "tab\there",
        "back\\slash",
        "unicode: héllo — 世界",
    ];

    // `sh` is $0 for the -c script, so "$@" is exactly the arguments after it.
    let mut cmd = Command::new(bin());
    cmd.args(["run", &container, "--", "/bin/sh", "-c"])
        .arg("printf '%s\\0' \"$@\"")
        .arg("sh");
    for case in &cases {
        cmd.arg(case); // literal: no shell is involved on this side
    }
    let out = cmd.output().expect("failed to execute podnext");

    let code = out.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        0,
        "run exited {code}; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut got: Vec<String> = out
        .stdout
        .split(|b| *b == 0)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    if got.last().map(|s| s.is_empty()).unwrap_or(false) {
        got.pop(); // the separator after the final argument
    }

    let expected: Vec<String> = cases.iter().map(|c| (*c).to_string()).collect();
    assert_eq!(
        got.len(),
        expected.len(),
        "the guest saw {} arguments for {} sent\nsent:     {expected:?}\nreceived: {got:?}",
        got.len(),
        expected.len()
    );
    for (i, (want, have)) in expected.iter().zip(got.iter()).enumerate() {
        assert_eq!(
            want, have,
            "argument {i} changed in transit\n  sent:     {want:?}\n  received: {have:?}"
        );
    }
}

/// The other half of the property: an argument that is *not* expanded must not be, and a shell in
/// the middle of the run path would show up here. Kept separate so a failure names the mechanism.
#[test]
fn the_host_does_not_expand_what_the_caller_wrote() {
    let container = match pick_container() {
        Some(c) => c,
        None => {
            eprintln!("skipped: no container is installed on this host");
            return;
        }
    };
    let out = Command::new(bin())
        .args(["run", &container, "--", "/bin/sh", "-c"])
        .arg("printf '%s\\0' \"$@\"")
        .arg("sh")
        .arg("$HOME") // would expand to the host home if a shell intervened
        .arg("$(whoami)") // would run if a shell intervened
        .arg("`id -u`")
        .output()
        .expect("failed to execute podnext");

    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout).replace('\0', "|");
    for needle in ["$HOME", "$(whoami)", "`id -u`"] {
        assert!(
            text.contains(needle),
            "{needle} was expanded somewhere in the run path; guest saw: {text}"
        );
    }
    assert!(
        !text.contains("u0_a"),
        "an expansion produced a host identity; guest saw: {text}"
    );
}
