//! `podnext doctor` - a host and guest diagnostic that reports what it measured.
//!
//! Every check runs and reports one of three verdicts, and each carries the value or the
//! command it came from. Nothing here is inferred from the absence of an error: a check
//! that cannot run says so (`Unknown`) instead of passing.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Ok,
    Warn,
    Fail,
    /// The check could not be run at all; the detail carries the reason.
    Unknown,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Verdict::Ok => "ok",
            Verdict::Warn => "warn",
            Verdict::Fail => "fail",
            Verdict::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub verdict: Verdict,
    /// What was measured, or the reason the check could not run.
    pub detail: String,
}

impl Check {
    fn new(name: &str, verdict: Verdict, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            verdict,
            detail: detail.into(),
        }
    }
}

/// Run every host check. `container` adds the guest checks, which need a real container.
pub async fn collect(container: Option<&str>) -> Vec<Check> {
    let mut checks = Vec::new();
    checks.push(proot_binary_check());
    checks.push(temp_dir_check());
    checks.push(dev_shm_check());
    checks.push(dns_check());
    checks.push(kernel_check());
    checks.push(cgroups_check());
    // S5/C7: the capabilities PDN's own claims rest on. Each is a READ - none changes the state it
    // measures - and each answers `Unknown` when it cannot measure, never `Ok`.
    checks.push(namespace_check());
    checks.push(seccomp_actions_check());
    checks.push(writable_cgroupfs_check());
    checks.push(pidfd_check());
    checks.push(executor_uid_check());
    // S2: the identity/credential syscalls PDN names, and what it does about each, read from the
    // table itself rather than retyped here.
    checks.push(identity_check());
    checks.push(subreaper_check());
    checks.extend(container_checks(container).await);
    checks
}

/// The first `d.d.d` token in `text`, if any. Used because `proot --version` on Termux
/// prints a banner whose last line ends in the version number.
fn version_token(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.'))
        .find(|token| {
            token.matches('.').count() >= 2
                && token.starts_with(|c: char| c.is_ascii_digit())
                && token.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
        .map(str::to_string)
}

fn proot_binary_check() -> Check {
    // OPTIONAL SINCE 2026-11-09 (p7 step 8). The default engine is podnext's own in-tree ptrace
    // engine, so a host without `proot` runs containers; only `--engine proot` needs the binary.
    // This check therefore WARNS when it is missing instead of failing. The old `Fail` was correct
    // while proot was the default and is now a false alarm - it sent operators looking for a
    // package they no longer need, which is exactly the confusion reported from real use.
    // Requirement, verbatim from workbench/WORKLIST.md section Z step 8: "doctor reports the proot
    // binary as optional (not a `fail` row)".
    // The same resolver the rest of PDN uses, so a relocated prefix is reported as the path
    // the run path would actually use rather than a second guess at it.
    let binary = std::env::var("PROOT_BIN").unwrap_or_else(|_| {
        proot_distro_core::paths::host::prefix()
            .join("bin/proot")
            .display()
            .to_string()
    });
    match std::process::Command::new(&binary)
        .arg("--version")
        .output()
    {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            // This proot prints ASCII art with the version embedded in it and no plain
            // version line, so the version is extracted as a token (d.d.d) rather than
            // taken from the first line, which is part of the drawing.
            let first = match version_token(&text) {
                Some(version) => format!("proot {version}"),
                None => text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("")
                    .to_string(),
            };
            if out.status.success() && !first.is_empty() {
                Check::new("proot binary", Verdict::Ok, format!("{binary}: {first}"))
            } else {
                Check::new(
                    "proot binary",
                    Verdict::Warn,
                    format!(
                        "{binary} exited {:?} without a version line - optional: the default \
                         engine is podnext's own, so only `--engine proot` is affected",
                        out.status.code()
                    ),
                )
            }
        }
        // Absent, not broken: the default engine never spawns it, so this is a warning that names
        // the one thing it does affect rather than a failure implying containers cannot run.
        Err(err) => Check::new(
            "proot binary",
            Verdict::Warn,
            format!(
                "{binary}: {err} - optional: the default engine is podnext's own, so only \
                 `--engine proot` is affected"
            ),
        ),
    }
}

fn temp_dir_check() -> Check {
    // `std::env::temp_dir()` respects TMPDIR; on Android it lands somewhere an app uid
    // cannot write when TMPDIR is unset, which is why the project mandates $PREFIX/tmp.
    let dir = std::env::temp_dir();
    let probe = dir.join(format!("podnext-doctor-{}", std::process::id()));
    match std::fs::write(&probe, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Check::new(
                "temporary directory",
                Verdict::Ok,
                format!(
                    "{} is writable (TMPDIR={:?})",
                    dir.display(),
                    std::env::var("TMPDIR").ok()
                ),
            )
        }
        Err(err) => Check::new(
            "temporary directory",
            Verdict::Fail,
            format!(
                "{} is not writable: {err}. Set TMPDIR to $PREFIX/tmp",
                dir.display()
            ),
        ),
    }
}

fn dev_shm_check() -> Check {
    let path = std::env::var("PN_SHM_DIR").unwrap_or_else(|_| "/dev/shm".to_string());
    match std::fs::metadata(&path) {
        Ok(meta) => {
            let probe =
                std::path::Path::new(&path).join(format!("podnext-doctor-{}", std::process::id()));
            match std::fs::write(&probe, b"probe") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&probe);
                    Check::new(
                        "shared memory",
                        Verdict::Ok,
                        format!("{path} exists and is writable"),
                    )
                }
                Err(err) => Check::new(
                    "shared memory",
                    Verdict::Warn,
                    format!(
                        "{path} exists ({:?}) but is not writable by this uid: {err}",
                        meta.permissions()
                    ),
                ),
            }
        }
        Err(err) => Check::new(
            "shared memory",
            Verdict::Ok,
            format!(
                "{path} is not present ({err}), which is expected on Android: PDN binds a \
per-container directory in its place, so this is informational rather than a fault. \
Run `podnext doctor --container <name>` to see the guest side"
            ),
        ),
    }
}

fn dns_check() -> Check {
    let mut found = Vec::new();
    // Termux's own prefix carries resolv.conf; /etc and /system/etc are what the guest
    // sees on other layouts, so all three are tried rather than assumed.
    let prefix_conf = std::env::var("PREFIX")
        .map(|p| format!("{p}/etc/resolv.conf"))
        .unwrap_or_default();
    let candidates = [
        "/etc/resolv.conf",
        "/system/etc/resolv.conf",
        prefix_conf.as_str(),
    ];
    for candidate in candidates.into_iter().filter(|c| !c.is_empty()) {
        if let Ok(text) = std::fs::read_to_string(candidate) {
            let servers: Vec<String> = text
                .lines()
                .filter_map(|l| l.trim().strip_prefix("nameserver "))
                .map(|s| s.trim().to_string())
                .collect();
            if !servers.is_empty() {
                found.push(format!("{candidate} -> {}", servers.join(", ")));
            }
        }
    }
    if found.is_empty() {
        Check::new(
            "dns",
            Verdict::Warn,
            "no nameserver found in /etc/resolv.conf, /system/etc/resolv.conf or $PREFIX/etc/resolv.conf; \
             a container gets its own resolv.conf written at start, so this affects the host only",
        )
    } else {
        Check::new("dns", Verdict::Ok, found.join("; "))
    }
}

/// `/proc/version` is not readable by an app uid on Android, so the kernel is read with
/// `uname`; a check that cannot run reports `unknown`, never `ok` with an error in the detail.
fn kernel_check() -> Check {
    match std::process::Command::new("uname").arg("-a").output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                Check::new("host kernel", Verdict::Unknown, "uname -a produced nothing")
            } else {
                Check::new("host kernel", Verdict::Ok, text)
            }
        }
        Ok(out) => Check::new(
            "host kernel",
            Verdict::Unknown,
            format!("uname -a exited {:?}", out.status.code()),
        ),
        Err(err) => Check::new(
            "host kernel",
            Verdict::Unknown,
            format!("uname unavailable: {err}"),
        ),
    }
}

fn cgroups_check() -> Check {
    match std::fs::read_to_string("/proc/self/cgroup") {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            if lines.is_empty() {
                Check::new(
                    "cgroups",
                    Verdict::Warn,
                    "/proc/self/cgroup is empty: resource limits are not applied",
                )
            } else {
                Check::new(
                    "cgroups",
                    Verdict::Ok,
                    format!("{} entry/entries; first: {}", lines.len(), lines[0]),
                )
            }
        }
        Err(err) => Check::new(
            "cgroups",
            Verdict::Unknown,
            format!("/proc/self/cgroup unreadable: {err}"),
        ),
    }
}

/// The `proot` command line PDN ran, read out of the probe's debug output.
///
/// The line is long (every synthetic bind is one argument), so it is reported whole but the
/// caller decides how much to print; the point is that the report carries the command that
/// ran rather than a reconstruction of it.
fn proot_argv(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find_map(|line| line.split_once("Executing proot command: "))
        .map(|(_, argv)| argv.trim().to_string())
        .filter(|argv| !argv.is_empty())
}

/// Guest checks. They run the container, so they are only attempted when one is named; a
/// container that cannot be opened is a `Fail` that says which container and why.
async fn container_checks(container: Option<&str>) -> Vec<Check> {
    let Some(name) = container else {
        return vec![Check::new(
            "guest",
            Verdict::Unknown,
            "no container named; pass one (podnext doctor --container <name>) to check the guest side",
        )];
    };
    let command = concat!(
        "echo OS=$(sed -n 's/^PRETTY_NAME=\"\\(.*\\)\"/\\1/p' /etc/os-release 2>/dev/null); ",
        "echo UNAME=$(uname -m); ",
        "echo SHM=$(df -P /dev/shm 2>/dev/null | tail -n 1 | awk '{print $2\"K \"$4\"K free\"}'); ",
        "echo MOUNTS=$(grep -c ' ' /proc/mounts 2>/dev/null); ",
        "echo RESOLV=$(grep -c nameserver /etc/resolv.conf 2>/dev/null)"
    );
    let executable = std::env::current_exe().ok();
    let Some(binary) = executable else {
        return vec![Check::new(
            "guest",
            Verdict::Unknown,
            "cannot locate this binary to re-enter the guest",
        )];
    };
    let output = tokio::process::Command::new(binary)
        .args(["run", name, "--", "/bin/sh", "-c", command])
        // The argv is the thing to look at when a mount or a stub is missing, and it is only
        // printed when this is set (see the diagnostic in `proot-core`). Captured here rather
        // than re-derived, so the report shows the command that actually ran.
        .env("PN_DEBUG", "1")
        .output()
        .await;
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() {
                return vec![Check::new(
                    "guest",
                    Verdict::Fail,
                    format!(
                        "running a probe in container '{name}' exited {:?}: {}",
                        out.status.code(),
                        stderr.lines().last().unwrap_or("").trim()
                    ),
                )];
            }
            let reported: Vec<Check> = stdout
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(key, value)| {
                    let detail = if value.is_empty() {
                        "not reported".to_string()
                    } else {
                        value.to_string()
                    };
                    let verdict = if value.is_empty() {
                        Verdict::Unknown
                    } else {
                        Verdict::Ok
                    };
                    Check::new(&format!("guest {key}"), verdict, detail)
                })
                .collect();
            if let Some(argv) = proot_argv(&stderr) {
                let mut checks: Vec<Check> = reported.clone();
                checks.push(Check::new("guest proot argv", Verdict::Ok, argv));
                return checks;
            }
            if reported.is_empty() {
                // The probe ran and reported nothing: that is not a pass. Without this the
                // guest section simply vanished, and an empty section reads as "no problems".
                return vec![Check::new(
                    "guest",
                    Verdict::Unknown,
                    format!(
                        "the probe in container '{name}' produced no readable values; \
stdout={:?} stderr={:?}",
                        stdout.trim(),
                        stderr.trim()
                    ),
                )];
            }
            reported
        }
        Err(err) => vec![Check::new(
            "guest",
            Verdict::Unknown,
            format!("could not run the guest probe: {err}"),
        )],
    }
}

/// How much of a detail the text table shows before it is elided. The proot argv is hundreds of
/// arguments long (one per synthetic bind), so the table would stop being a table; the JSON and
/// YAML output carry it whole.
const TEXT_DETAIL_LIMIT: usize = 200;

fn elide(detail: &str) -> String {
    if detail.chars().count() <= TEXT_DETAIL_LIMIT {
        return detail.to_string();
    }
    let mut out: String = detail.chars().take(TEXT_DETAIL_LIMIT - 1).collect();
    out.push('\u{2026}');
    out
}

pub fn render_text(checks: &[Check]) {
    for check in checks {
        println!(
            "{:<10} {:<18} {}",
            check.verdict.label(),
            check.name,
            elide(&check.detail)
        );
    }
    if checks
        .iter()
        .any(|c| c.detail.chars().count() > TEXT_DETAIL_LIMIT)
    {
        println!();
        println!("(long details are elided here; `--format json` keeps them whole)");
    }
    let failed = checks.iter().filter(|c| c.verdict == Verdict::Fail).count();
    let warned = checks.iter().filter(|c| c.verdict == Verdict::Warn).count();
    let unknown = checks
        .iter()
        .filter(|c| c.verdict == Verdict::Unknown)
        .count();
    println!();
    println!(
        "{} checks: {failed} failed, {warned} warnings, {unknown} unknown",
        checks.len()
    );
}

pub fn worst_verdict(checks: &[Check]) -> Verdict {
    if checks.iter().any(|c| c.verdict == Verdict::Fail) {
        Verdict::Fail
    } else if checks.iter().any(|c| c.verdict == Verdict::Warn) {
        Verdict::Warn
    } else {
        Verdict::Ok
    }
}

#[cfg(test)]
mod tests {
    /// The Docker refusal cites this line, so a probe missing from it is a measurement the message
    /// quietly drops. Asserted as a count as well as by name: the count is what catches an entry
    /// that was added to `collect` and forgotten here.
    #[test]
    fn capability_line_names_every_probe_it_renders() {
        let line = super::capability_line();
        let names = [
            "namespaces",
            "seccomp",
            "cgroupfs",
            "pidfd",
            "executor uid",
            "subreaper",
        ];
        for name in names {
            assert!(
                line.contains(name),
                "capability_line must name '{name}': {line}"
            );
        }
        assert_eq!(
            line.matches(',').count(),
            names.len() - 1,
            "one entry per probe and no more: {line}"
        );
        assert!(
            line.contains(":ok")
                || line.contains(":warn")
                || line.contains(":fail")
                || line.contains(":unknown"),
            "every entry must carry its verdict, not just its name: {line}"
        );
    }

    use super::*;

    #[test]
    fn the_version_is_read_out_of_the_banner() {
        let banner = " _____ _____              ___\n|  __ \\  __ \\_____  _____|   |_\n|   __/     /  _  \\/  _  \\    _|\n|__|  |__|__\\_____/\\_____/\\____| 5.1.107.101\n\nbuilt-in accelerators: process_vm = yes\n";
        assert_eq!(version_token(banner).as_deref(), Some("5.1.107.101"));
        assert_eq!(version_token("no version here"), None);
        assert_eq!(version_token(""), None);
    }

    #[tokio::test]
    async fn every_check_reports_a_name_and_what_it_measured() {
        let checks = collect(None).await;
        assert!(!checks.is_empty(), "the host checks must produce results");
        for check in &checks {
            assert!(
                !check.name.trim().is_empty(),
                "a check without a name is unreadable"
            );
            assert!(
                !check.detail.trim().is_empty(),
                "{} reported no detail",
                check.name
            );
        }
    }

    #[test]
    fn short_details_are_whole_and_long_ones_are_elided_with_a_pointer() {
        let short = "a".repeat(TEXT_DETAIL_LIMIT);
        assert_eq!(elide(&short), short);
        let long = "a".repeat(TEXT_DETAIL_LIMIT + 50);
        let out = elide(&long);
        assert_eq!(
            out.chars().count(),
            TEXT_DETAIL_LIMIT,
            "the elided form keeps the limit: {out}"
        );
        assert!(out.ends_with('\u{2026}'), "the elision is visible: {out}");
    }

    #[test]
    fn the_argv_is_read_out_of_the_debug_line() {
        let stderr = "[DEBUG] Executing proot command: Command { std: cd \"/rootfs\" && \"/usr/bin/proot\" \"--bind=/a:/b\" \"/bin/sh\", kill_on_drop: false }\n";
        let argv = proot_argv(stderr).expect("the line is present");
        assert!(argv.contains("--bind=/a:/b"), "{argv}");
        assert!(argv.starts_with("Command {"), "{argv}");
        assert_eq!(proot_argv("no debug output here"), None);
    }

    /// A guest probe that reports nothing must never produce an `ok` row. In a unit test
    /// `current_exe()` is this harness rather than the CLI, so the probe yields no `KEY=`
    /// lines - the same shape as a guest that answers nothing, which is the invariant
    /// asserted here. The end-to-end case (a container that does not exist exits 1 and
    /// names itself) lives in `crates/podnext/tests/doctor_contract.rs`.
    #[tokio::test]
    async fn a_probe_that_reports_nothing_is_not_a_pass() {
        let checks = container_checks(Some("a-container-that-does-not-exist")).await;
        assert!(
            !checks.is_empty(),
            "silence must not produce an empty guest section"
        );
        assert_ne!(checks[0].verdict, Verdict::Ok, "{}", checks[0].detail);
        assert!(!checks[0].detail.trim().is_empty());
    }

    #[tokio::test]
    async fn without_a_container_the_guest_side_is_unknown_not_ok() {
        let checks = container_checks(None).await;
        assert_eq!(checks[0].verdict, Verdict::Unknown);
    }

    #[test]
    fn the_worst_verdict_drives_the_exit_status() {
        let ok = vec![Check::new("a", Verdict::Ok, "fine")];
        assert_eq!(worst_verdict(&ok), Verdict::Ok);
        let warned = vec![
            Check::new("a", Verdict::Ok, "fine"),
            Check::new("b", Verdict::Warn, "meh"),
        ];
        assert_eq!(worst_verdict(&warned), Verdict::Warn);
        let failed = vec![
            Check::new("a", Verdict::Warn, "meh"),
            Check::new("b", Verdict::Fail, "no"),
        ];
        assert_eq!(worst_verdict(&failed), Verdict::Fail);
    }
}
/// Namespaces: which ones this process is actually in, read from the per-process links, plus the
/// sysctl when the sandbox exposes it. `/proc/sys/user/max_user_namespaces` is denied to an app uid
/// on Android, so it is a detail rather than the measurement - a probe that answered from it alone
/// would report `unknown` on every Android device and be blind exactly where it matters.
fn namespace_check() -> Check {
    let mut seen: Vec<String> = Vec::new();
    for (label, path) in [
        ("mnt", "/proc/self/ns/mnt"),
        ("pid", "/proc/self/ns/pid"),
        ("net", "/proc/self/ns/net"),
        ("user", "/proc/self/ns/user"),
    ] {
        if let Ok(target) = std::fs::read_link(path) {
            seen.push(format!("{label}={}", target.display()));
        }
    }
    let sysctl = match std::fs::read_to_string("/proc/sys/user/max_user_namespaces") {
        Ok(raw) => format!("max_user_namespaces={}", raw.trim()),
        Err(err) => format!("max_user_namespaces unreadable ({err})"),
    };
    if seen.is_empty() {
        return Check::new(
            "namespaces",
            Verdict::Unknown,
            format!("no /proc/self/ns/* link could be read; {sysctl}"),
        );
    }
    // Being IN namespaces is not being able to CREATE them: the executor's uid decides that, which
    // is why the fact is reported with the sysctl beside it instead of as a bare `ok`.
    Check::new(
        "namespaces",
        Verdict::Warn,
        format!(
            "this process is in {} namespace(s) [{}]; {sysctl} - creating mount/pid/net namespaces additionally needs privileges this executor's uid may not have",
            seen.len(),
            seen.join(" ")
        ),
    )
}

/// Seccomp: the mode this process is in, read from `/proc/self/status` (which the sandbox does
/// expose, unlike `actions_avail`). Mode 0 means no filter is installed - the state PDN was in when
/// `seccomp.rs` promised a boundary it never built (A21).
fn seccomp_actions_check() -> Check {
    let status = match std::fs::read_to_string("/proc/self/status") {
        Ok(text) => text,
        Err(err) => {
            return Check::new(
                "seccomp",
                Verdict::Unknown,
                format!("cannot read /proc/self/status: {err}"),
            )
        }
    };
    let mode = status
        .lines()
        .find_map(|l| l.strip_prefix("Seccomp:"))
        .map(|rest| rest.trim().to_string());
    let avail = std::fs::read_to_string("/proc/sys/kernel/seccomp/actions_avail")
        .map(|raw| format!("actions_avail: {}", raw.trim()))
        .unwrap_or_else(|err| format!("actions_avail unreadable ({err})"));
    match mode.as_deref() {
        Some("0") => Check::new(
            "seccomp",
            Verdict::Warn,
            format!("no seccomp filter is installed on this process (mode 0); {avail}"),
        ),
        Some(mode) if !mode.is_empty() => Check::new(
            "seccomp",
            Verdict::Ok,
            format!("a seccomp filter is installed (mode {mode}); {avail}"),
        ),
        _ => Check::new(
            "seccomp",
            Verdict::Unknown,
            format!("no Seccomp: line in /proc/self/status; {avail}"),
        ),
    }
}

/// cgroupfs writability - the difference between a limit that is enforced and a number that is only
/// written down. Reuses the supervisor's own helper, so the check and the start path cannot
/// disagree about which mount is in play.
fn writable_cgroupfs_check() -> Check {
    match pn_next::writable_cgroupfs() {
        Some(path) => Check::new(
            "cgroupfs",
            Verdict::Ok,
            format!(
                "{} is writable: cgroup limits can be enforced",
                path.display()
            ),
        ),
        None => Check::new(
            "cgroupfs",
            Verdict::Warn,
            "no writable cgroupfs: CPU/memory/pids limits cannot be enforced here, and the honest view refuses them by name instead of writing them down",
        ),
    }
}

/// pidfd needs kernel 5.3. The release comes from `uname -r`, because `/proc/version` is not
/// readable by an app uid on Android (the same reason `kernel_check` uses `uname`).
fn pidfd_check() -> Check {
    let out = match std::process::Command::new("uname").arg("-r").output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Ok(out) => {
            return Check::new(
                "pidfd",
                Verdict::Unknown,
                format!("uname -r exited {:?}", out.status.code()),
            )
        }
        Err(err) => {
            return Check::new(
                "pidfd",
                Verdict::Unknown,
                format!("uname unavailable: {err}"),
            )
        }
    };
    match parse_kernel_release(&out) {
        Some((major, minor)) if (major, minor) >= (5, 3) => Check::new(
            "pidfd",
            Verdict::Ok,
            format!("kernel {out} supports pidfd_open (>= 5.3)"),
        ),
        Some((major, minor)) => Check::new(
            "pidfd",
            Verdict::Fail,
            format!("kernel {out} ({major}.{minor}) is older than 5.3: pidfd_open is unavailable"),
        ),
        None => Check::new(
            "pidfd",
            Verdict::Unknown,
            format!("cannot parse a kernel version out of {out:?}"),
        ),
    }
}

/// `6.6.30-android15-8-gabc` -> `(6, 6)`; anything else is `None`, which the caller reports as
/// `Unknown` rather than guessing a capability from an unparsable string.
fn parse_kernel_release(release: &str) -> Option<(u32, u32)> {
    let mut parts = release
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Effective uid: which namespace and cgroup paths can exist at all depends on it, so it is
/// reported as a measurement rather than left implicit.
fn executor_uid_check() -> Check {
    match std::fs::read_to_string("/proc/self/status") {
        Ok(status) => match status
            .lines()
            .find_map(|l| l.strip_prefix("Uid:"))
            .and_then(|rest| rest.split_whitespace().next())
        {
            Some(uid) => Check::new(
                "executor uid",
                Verdict::Ok,
                format!("effective uid {uid}; namespace and cgroup writes depend on it"),
            ),
            None => Check::new(
                "executor uid",
                Verdict::Unknown,
                "Uid: line absent from /proc/self/status",
            ),
        },
        Err(err) => Check::new(
            "executor uid",
            Verdict::Unknown,
            format!("cannot read /proc/self/status: {err}"),
        ),
    }
}

/// The identity/credential syscalls PDN names, and what it does about each (adoption S2,
/// `proot_core::identity`). The count is read from the table rather than retyped, so the row and
/// the table cannot disagree; the mapping the table encodes is named too, so a caller sees WHY a
/// guest reports an identity rather than being told only that it does.
fn identity_check() -> Check {
    let count = proot_core::identity::IDENTITY_SYSCALL_TABLE.len();
    Check::new(
        "guest identity",
        Verdict::Ok,
        format!(
            "the runtime maps a guest to one host identity via `--change-id=0:0:0` (a guest is \
             root); {count} identity/credential syscalls are named in proot_core::identity with \
             their disposition, and none switches the executor's real uid"
        ),
    )
}

/// Child subreaper: A22 sets it on the exec path and there is no side-effect-free way to read the
/// flag back from here. `Unknown` with the reason is the honest answer - `Ok` would assert a
/// capability this check never measured.
fn subreaper_check() -> Check {
    Check::new(
        "subreaper",
        Verdict::Unknown,
        "not measurable without changing process state (PR_SET_CHILD_SUBREAPER has no read-only form); the exec path sets it (A22) and orphan reaping has its own tests",
    )
}
/// The capability probes rendered as one line, for callers that must name WHY a capability is
/// missing rather than asserting that it is. Reads the same functions `collect` pushes, so a probe
/// and every message that cites it cannot disagree.
pub(crate) fn capability_line() -> String {
    let probes = [
        namespace_check(),
        seccomp_actions_check(),
        writable_cgroupfs_check(),
        pidfd_check(),
        executor_uid_check(),
        subreaper_check(),
    ];
    probes
        .iter()
        .map(|c| format!("{}:{}", c.name, c.verdict.label()))
        .collect::<Vec<_>>()
        .join(", ")
}
