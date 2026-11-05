//! Roadmap section 9.1 contract test: every command and every documented option in
//! `docs/src/cli-reference.md` must exist in the real CLI, and the documentation must not
//! describe exit codes or environment variables that no code path implements.
//!
//! This test exists because the reverse happened: a conflict resolution re-added a phantom
//! exit-code table (3–12), eight inert `PN_*` variables and a `--short`/`--json` option table
//! for `podnext version`, and nothing failed. It runs the built binary, so it measures the
//! product rather than restating the source.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn cli_reference() -> String {
    std::fs::read_to_string(repo_root().join("docs/src/cli-reference.md"))
        .expect("docs/src/cli-reference.md must exist")
}

fn help(args: &[String]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_podnext"))
        .args(args)
        .arg("--help")
        .output()
        .expect("run podnext --help");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// Every `### \`podnext …\`` heading, paired with the section body that follows it.
fn documented_sections(doc: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for line in doc.lines() {
        if let Some(rest) = line.strip_prefix("### `podnext ") {
            if let Some(c) = cur.take() {
                out.push(c);
            }
            let spec = rest.trim_end_matches('`').trim().to_string();
            cur = Some((spec, String::new()));
        } else if line.starts_with("## ") {
            if let Some(c) = cur.take() {
                out.push(c);
            }
        } else if let Some((_, body)) = cur.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

/// Long flags named in the first cell of an option table row: `| \`-u, --user <USER>\` | … |`.
fn documented_long_flags(body: &str) -> Vec<String> {
    let mut flags: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("| `") {
            continue;
        }
        let cell = trimmed.split('|').nth(1).unwrap_or("");
        for token in cell.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
            if token.starts_with("--") && token.len() > 3 && !flags.iter().any(|f| f == token) {
                flags.push(token.to_string());
            }
        }
    }
    flags
}

fn rust_sources(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
        }
    }
}

#[test]
fn every_documented_command_answers_help() {
    let doc = cli_reference();
    let mut broken = Vec::new();
    for (spec, _) in documented_sections(&doc) {
        let args: Vec<String> = spec.split_whitespace().map(str::to_string).collect();
        let (ok, _) = help(&args);
        if !ok {
            broken.push(spec);
        }
    }
    assert!(
        broken.is_empty(),
        "documented commands that do not answer --help: {broken:#?}"
    );
}

#[test]
fn every_documented_option_exists_in_the_real_help() {
    let doc = cli_reference();
    let mut broken = Vec::new();
    for (spec, body) in documented_sections(&doc) {
        let args: Vec<String> = spec.split_whitespace().map(str::to_string).collect();
        let (ok, text) = help(&args);
        if !ok {
            continue; // reported by the command test
        }
        for flag in documented_long_flags(&body) {
            if !text.contains(&flag) {
                broken.push(format!("{spec} documents `{flag}`"));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "documented options that --help does not list: {broken:#?}"
    );
}

#[test]
fn no_phantom_exit_codes_or_environment_variables_in_the_reference() {
    let doc = cli_reference();
    // Exit codes 3–12 were documented and are produced by no code path. Only rows inside the
    // Exit Codes section count: another table may legitimately use the same shape.
    let exit_section = doc
        .split("## Exit Codes")
        .nth(1)
        .expect("an Exit Codes section")
        .split("\n## ")
        .next()
        .unwrap();
    for code in 3..=12 {
        assert!(
            !exit_section.contains(&format!("| {code} | ")),
            "cli-reference.md documents exit code {code}, which no code path returns"
        );
    }
    // For variables, only a table row is a claim. The prose in this file names the removed
    // variables on purpose, to record that they were removed.
    for var in [
        "PN_CONFIG_DIR",
        "PN_DATA_DIR",
        "PN_CACHE_DIR",
        "PN_LOG_LEVEL",
        "PN_GATEWAY_ADDR",
        "PN_SECURITY_PROFILE",
        "PN_NO_COLOR",
    ] {
        assert!(
            !doc.contains(&format!("| `{var}`")),
            "cli-reference.md documents {var}, which nothing in crates/ reads"
        );
    }
}

/// (command, label) pairs the binary itself prints in its `Commands:` listing.
fn help_stability_labels() -> Vec<(String, String)> {
    let (ok, text) = help(&[]);
    assert!(ok, "podnext --help must succeed");
    let mut out = Vec::new();
    let mut in_commands = false;
    for line in text.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if !line.trim().is_empty() && !line.starts_with(' ') {
                break; // left the section
            }
            if let Some(c) = stability_label(line) {
                out.push(c);
            }
        }
    }
    out
}

/// `name [label]` from one Commands-listing line; no regex crate is available to these tests.
fn stability_label(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let (name, rest) = t.split_once(' ')?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
        return None;
    }
    let rest = rest.trim_start();
    for label in ["stable", "experimental"] {
        if rest.starts_with(&format!("[{label}]")) {
            return Some((name.to_string(), label.to_string()));
        }
    }
    None
}

/// (command, label) pairs from the "## Command stability" table in the reference.
fn documented_stability_labels() -> Vec<(String, String)> {
    let doc = cli_reference();
    let section = doc
        .split("## Command stability")
        .nth(1)
        .expect("a Command stability section")
        .split("\n## ")
        .next()
        .unwrap();
    let mut out = Vec::new();
    for line in section.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("| `podnext ") else {
            continue;
        };
        let Some((cmd, tail)) = rest.split_once('`') else {
            continue;
        };
        for label in ["stable", "experimental"] {
            if tail.contains(&format!("`[{label}]`")) {
                out.push((cmd.to_string(), label.to_string()));
            }
        }
    }
    out
}

#[test]
fn every_command_carries_a_stability_label_in_help() {
    let labels = help_stability_labels();
    assert!(
        labels.len() >= 30,
        "expected the Commands listing to be parsed, got {} entries: {labels:?}",
        labels.len()
    );
    let unlabelled: Vec<String> = {
        let (_, text) = help(&[]);
        let mut found = Vec::new();
        let mut in_commands = false;
        for line in text.lines() {
            if line.starts_with("Commands:") {
                in_commands = true;
                continue;
            }
            if in_commands {
                if !line.trim().is_empty() && !line.starts_with(' ') {
                    break;
                }
                let t = line.trim_start();
                if let Some((name, _)) = t.split_once(' ') {
                    if name == "help" {
                        continue;
                    }
                    if !name.is_empty()
                        && name.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                        && stability_label(line).is_none()
                    {
                        found.push(name.to_string());
                    }
                }
            }
        }
        found
    };
    assert!(
        unlabelled.is_empty(),
        "commands without [stable]/[experimental] in --help: {unlabelled:?}"
    );
}

#[test]
fn the_documented_stability_table_matches_the_binary() {
    let mut from_help = help_stability_labels();
    let mut from_doc = documented_stability_labels();
    from_help.sort();
    from_doc.sort();
    assert_eq!(
        from_help, from_doc,
        "cli-reference.md's Command stability table and `podnext --help` disagree"
    );
}

#[test]
fn every_documented_environment_variable_is_read_by_the_code() {
    let root = repo_root();
    let doc = cli_reference();
    let section = doc
        .split("## Environment Variables")
        .nth(1)
        .expect("an Environment Variables section")
        .split("\n## ")
        .next()
        .unwrap()
        .to_string();

    let mut sources = String::new();
    rust_sources(&root.join("crates"), &mut sources);

    let mut unread = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("| `") {
            continue;
        }
        let name = trimmed.split('`').nth(1).unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        if !sources.contains(&name) {
            unread.push(name);
        }
    }
    assert!(
        unread.is_empty(),
        "documented environment variables that no crate reads: {unread:?}"
    );
}
