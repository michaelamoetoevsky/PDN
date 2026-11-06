//! WORKLIST B9 and B10: stdout is the data channel, and a page belongs to the
//! command that owns it.
//!
//! B9 - the comparator is `login/message.py:207`: the one human write in that
//! package goes to `stderr`, so `stdout` stays parseable. This file gates the
//! same rule over every sub-command that publishes both renderings (one that
//! declares its own `--format`): the JSON path's `stdout` parses as JSON, and the
//! human path's `stdout` carries data only - no `Error:`, no `Note:`, no progress
//! line. Both are measured against the built binary, not restated from the source.
//!
//! B10 - `podnext help <cmd>` must answer with that command's page while the
//! configuration is unreadable, and a bad flag must print that command's page
//! rather than a bare `Usage:` blob.
//!
//! Every process runs in its own sandbox under `$TMPDIR`/`$PREFIX/tmp`: `HOME`,
//! `PN_TEST_ROOT`, `TERMUX__HOME` and `TMPDIR` all point there, `XDG_RUNTIME_DIR`
//! is removed (it outranks `HOME` for the daemon socket), and the operator store
//! is never touched. `PN_TEST_ROOT` is a HOME-like base - the product composes
//! `<PN_TEST_ROOT>/.podnext/containers` under it - never a store root.
//!
//! Ceiling of the human-path assertion: it is a denylist of the message prefixes
//! the product uses (`Error:`, `Note:`, `Installing`, ...) plus non-emptiness, not
//! a parser for "data". A command that invents a new diagnostic prefix would slip
//! through until the prefix is added here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

/// The message prefixes a human rendering may not put on `stdout`: these belong on
/// `stderr`, where a caller reading the data channel does not see them.
const MESSAGE_PREFIXES: &[&str] = &[
    "Error:",
    "error:",
    "Warning:",
    "warning:",
    "Warn:",
    "Note:",
    "note:",
    "Hint:",
    "hint:",
    "DEBUG",
    "INFO",
    "WARN",
    "TRACE",
    "Installing",
    "Pulling",
    "Downloading",
    "Extracting",
    "Starting",
    "Stopping",
    "Removing",
];

/// Every sub-command that declares its own `--format` and answers in a fresh sandbox,
/// with no guest, image, session or network to find.
///
/// `daemon status` is here with no daemon running: it renders both a table and JSON
/// (its exit code is a separate contract, so this gate asserts streams only). Each
/// remaining `--format` sub-command needs an argument that must exist first
/// (`quota get/usage`, `service status`, `image inspect/history`, `network/volume
/// inspect`, `backup create/status`) or is a refusing stub (`build submit/status/list/
/// artifacts/logs`), or needs a configuration file as input (`config validate`), so
/// none of them renders a pair in a sandbox. `dashboard status`, `log tail`,
/// `service logs` and `gitops reconcile` declare no `--format` of their own; the
/// global flag is accepted there and ignored, which is a separate finding, not a pair.
const PAIRS: &[&[&str]] = &[
    &["list"],
    &["ps"],
    &["sessions"],
    &["info"],
    &["version"],
    &["doctor"],
    &["search"],
    &["image", "ls"],
    &["volume", "ls"],
    &["network", "ls"],
    &["backup", "list"],
    &["service", "list"],
    &["daemon", "status"],
    &["watchdog", "status"],
    &["boot", "status"],
    &["wakelock", "status"],
    &["policy", "list"],
    &["policy", "eval", "pdn-no-such-service"],
    &["quota", "list"],
    &["dbus", "list"],
    &["socket", "list"],
    &["log", "query"],
    &["log", "stats"],
    &["gitops", "status"],
    &["gitops", "resources"],
    &["gitops", "drift"],
];

/// A private store and home for one test.
struct Sandbox {
    dir: PathBuf,
    home: PathBuf,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        let base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("pdn-stdout-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("home");
        std::fs::create_dir_all(&home).expect("sandbox home");
        Self { dir, home }
    }

    fn home(&self) -> &Path {
        &self.home
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_env(args, &[])
    }

    /// `podnext <args>` with the store levers pointed at the sandbox. `TERMUX__HOME`
    /// is set and `XDG_RUNTIME_DIR` removed because both outrank `HOME` in the
    /// product's own resolution, and either would otherwise name the operator's paths.
    fn run_env(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut c = Command::new(bin());
        c.args(args)
            .env("HOME", &self.home)
            .env("PN_TEST_ROOT", &self.home)
            .env("TERMUX__HOME", &self.home)
            .env("TMPDIR", &self.dir)
            .env("PN_NEXT_RUNTIME_DIR", self.home.join(".local/run/pn-next"))
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("PN_DEBUG")
            .env_remove("PN_GATEWAY_CONFIG");
        for (k, v) in env {
            c.env(k, v);
        }
        c.output()
            .unwrap_or_else(|e| panic!("spawn podnext {args:?}: {e}"))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(
            &self.home,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        );
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// B9, first assertion: the JSON path's `stdout` is data a caller can parse.
#[test]
fn json_path_stdout_parses() {
    let sandbox = Sandbox::new("json");
    let mut broken = Vec::new();
    for cmd in PAIRS {
        let mut args = cmd.to_vec();
        args.extend(["--format", "json"]);
        let out = sandbox.run(&args);
        let text = stdout(&out);
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&text) {
            let first = text.lines().next().unwrap_or("<no output>");
            broken.push(format!(
                "podnext {} --format json: stdout is not JSON ({e}); first line: {first:?}",
                cmd.join(" ")
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "the JSON path put something other than data on stdout:\n{}",
        broken.join("\n")
    );
}

/// B9, second assertion: the human path's `stdout` carries data only, and every
/// prompt, note or progress line went to `stderr`.
#[test]
fn human_path_stdout_carries_data_only() {
    let sandbox = Sandbox::new("human");
    let mut broken = Vec::new();
    for cmd in PAIRS {
        let out = sandbox.run(cmd);
        let text = stdout(&out);
        if text.trim().is_empty() {
            broken.push(format!(
                "podnext {}: the human path rendered nothing on stdout (stderr: {:?})",
                cmd.join(" "),
                stderr(&out).lines().next().unwrap_or("")
            ));
            continue;
        }
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(prefix) = MESSAGE_PREFIXES.iter().find(|p| t.starts_with(**p)) {
                broken.push(format!(
                    "podnext {}: a message line reached stdout: {t:?} (prefix {prefix:?})",
                    cmd.join(" ")
                ));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "the human path mixed messages into the data channel:\n{}",
        broken.join("\n")
    );
}

/// B10, first case: help does not need the store or the configuration.
///
/// The sandbox is made unreadable (the home directory and the file behind
/// `PN_GATEWAY_CONFIG` are both `chmod 000`) because a help page that had to load
/// configuration first would fail exactly there. The page is compared against the
/// same command's `--help`, so the assertion does not restate the text.
#[test]
fn help_stands_alone_with_an_unreadable_config() {
    let sandbox = Sandbox::new("help");
    let config = sandbox.home().join("gateway.toml");
    std::fs::write(&config, "not a configuration\n").expect("write config");

    let cmds: &[&[&str]] = &[
        &["list"],
        &["image", "ls"],
        &["quota", "list"],
        &["gitops", "status"],
        &["doctor"],
    ];

    let mut broken = Vec::new();
    for cmd in cmds {
        // `--help` first, while the store is still readable, to get the page.
        let mut help_args = cmd.to_vec();
        help_args.push("--help");
        let page = stdout(&sandbox.run(&help_args));

        // Now make every store and configuration read impossible. The file first:
        // a `chmod 000` on the directory would stop the file's own chmod too.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000))
            .expect("chmod config");
        std::fs::set_permissions(sandbox.home(), std::fs::Permissions::from_mode(0o000))
            .expect("chmod home");
        let mut args = vec!["help"];
        args.extend_from_slice(cmd);
        let out = sandbox.run_env(&args, &[("PN_GATEWAY_CONFIG", config.to_str().unwrap())]);
        std::fs::set_permissions(sandbox.home(), std::fs::Permissions::from_mode(0o700))
            .expect("restore home");

        let text = stdout(&out);
        let mut cmdline = cmd.to_vec();
        cmdline.push("--help");
        let expected_usage = format!("Usage: podnext {}", cmd.join(" "));
        if !out.status.success() {
            broken.push(format!(
                "podnext help {}: exit {:?} with an unreadable config (stderr: {:?})",
                cmd.join(" "),
                out.status.code(),
                stderr(&out).lines().next().unwrap_or("")
            ));
            continue;
        }
        if !text.contains(&expected_usage) {
            broken.push(format!(
                "podnext help {}: no {expected_usage:?} in stdout: {text:?}",
                cmd.join(" ")
            ));
        }
        // Its own page, line for line, not a stub or the top-level screen.
        for line in page.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
            if !text.lines().map(str::trim_end).any(|l| l == line) {
                broken.push(format!(
                    "podnext help {}: the page differs from `podnext {}` - missing {line:?}",
                    cmd.join(" "),
                    cmdline.join(" ")
                ));
                break;
            }
        }
        if !stderr(&out).is_empty() {
            broken.push(format!(
                "podnext help {}: wrote to stderr with an unreadable config: {:?}",
                cmd.join(" "),
                stderr(&out)
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "help must stand alone (no store, no configuration):\n{}",
        broken.join("\n")
    );
}

/// B10, second case: a rejected invocation is answered by the page of the command
/// that owns it, not by a `Usage:` one-liner.
///
/// Measured on this tree: the error names the command its own usage line, and the
/// assertion is that the whole `--help` page is there too. `--help` itself must
/// still write to stdout and exit 0.
#[test]
fn a_bad_flag_prints_that_command_s_own_page() {
    let sandbox = Sandbox::new("badflag");
    let cmds: &[&[&str]] = &[
        &["list"],
        &["image", "ls"],
        &["quota", "list"],
        &["gitops", "status"],
        &["doctor"],
    ];

    let mut broken = Vec::new();
    for cmd in cmds {
        let mut help_args = cmd.to_vec();
        help_args.push("--help");
        let help = sandbox.run(&help_args);
        if !help.status.success() || !stderr(&help).is_empty() {
            broken.push(format!(
                "podnext {} --help: exit {:?} with stderr {:?} - help is not a failure",
                cmd.join(" "),
                help.status.code(),
                stderr(&help).lines().next().unwrap_or("")
            ));
            continue;
        }
        let page = stdout(&help);
        let page_lines: Vec<&str> = page
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.is_empty())
            .collect();

        let mut args = cmd.to_vec();
        args.push("--no-such-flag");
        let out = sandbox.run(&args);
        let text = stderr(&out);
        let text_lines: Vec<&str> = text.lines().map(str::trim_end).collect();

        if out.status.code() != Some(2) {
            broken.push(format!(
                "podnext {}: a bad flag must exit 2, got {:?}",
                args.join(" "),
                out.status.code()
            ));
        }
        if !text.contains("--no-such-flag") {
            broken.push(format!(
                "podnext {}: the refusal does not name the flag: {text:?}",
                args.join(" ")
            ));
        }
        let missing: Vec<&&str> = page_lines
            .iter()
            .filter(|l| !text_lines.iter().any(|t| t == *l))
            .collect();
        if !missing.is_empty() {
            broken.push(format!(
                "podnext {}: the refusal is not `podnext {} --help` - {} of {} page lines missing, first {:?}",
                args.join(" "),
                cmd.join(" "),
                missing.len(),
                page_lines.len(),
                missing[0]
            ));
        }
        if !stdout(&out).is_empty() {
            broken.push(format!(
                "podnext {}: a refusal wrote to stdout: {:?}",
                args.join(" "),
                stdout(&out)
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "a bad flag must print the owning command's page:\n{}",
        broken.join("\n")
    );
}
