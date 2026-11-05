//! The commands that print rather than orchestrate: host diagnostics, the
//! version table, shell completions, the gateway-configuration check and the
//! top-level help screen.

use super::*;

/// `podnext config validate`: the load site for the gateway configuration.
///
/// Before this existed the loader was reachable only from tests, so the only way
/// to discover that a documented value could not be loaded was to read the code.
/// The command uses the same entry points the gateway will, and exits non-zero on
/// a configuration the gateway would refuse to start with.
pub(crate) fn execute_config_command(cmd: ConfigCommands) -> Result<()> {
    use pn_gateway::config::GatewayConfig;

    match cmd {
        ConfigCommands::Validate { path, format } => {
            let path = match path.or_else(|| {
                // Same variable the gateway reads, so the check and the start-up
                // path cannot disagree about which file is in play.
                std::env::var(GatewayConfig::CONFIG_ENV)
                    .ok()
                    .filter(|p| !p.trim().is_empty())
                    .map(PathBuf::from)
            }) {
                Some(path) => path,
                None => {
                    eprintln!(
                        "no configuration file given: pass a path or set {}",
                        GatewayConfig::CONFIG_ENV
                    );
                    // A missing required argument is the usage-error class the taxonomy names
                    // for exit 2 (cli-reference.md "Exit Codes": 2 = `Invalid arguments`). It is
                    // not a code of its own: exit 3 was documented nowhere and is asserted absent
                    // by `cli_reference_contract`, so it was the code that was wrong, not the doc.
                    std::process::exit(2);
                }
            };

            let config = match GatewayConfig::from_toml_file(&path) {
                Ok(config) => config,
                Err(err) => {
                    // A path that cannot be loaded is an invalid argument, which is exit 2
                    // (cli-reference.md: "an invalid one names the offending key with its line
                    // and column and exits 2"). Exit 3 was a phantom: the taxonomy is 0/1/2 plus
                    // the child's code, and no doc may name a code above 2.
                    eprintln!("invalid gateway configuration: {err}");
                    std::process::exit(2);
                }
            };

            match format {
                OutputFormat::Json => {
                    output_json(&config)?;
                    return Ok(());
                }
                OutputFormat::Yaml => {
                    output_yaml(&config)?;
                    return Ok(());
                }
                _ => {}
            }

            use pn_gateway::duration_fmt::format_duration;
            println!("{} is valid", path.display());
            println!("  name                {}", config.name);
            println!("  id                  {}", config.gateway_id);
            println!(
                "  proot timeout       {}",
                format_duration(config.proot_config.timeout)
            );
            println!(
                "  retries             {} (delay {})",
                config.fail_safe.max_retries,
                format_duration(config.fail_safe.retry_delay)
            );
            println!(
                "  circuit breaker     {} (threshold {}, timeout {})",
                config.fail_safe.circuit_breaker.enabled,
                config.fail_safe.circuit_breaker.failure_threshold,
                format_duration(config.fail_safe.circuit_breaker.timeout)
            );
            println!(
                "  health check        {} every {}",
                config.health.enabled,
                format_duration(config.health.check_interval)
            );
            println!(
                "  metrics             {} every {}, retained {}",
                config.metrics.enabled,
                format_duration(config.metrics.interval),
                format_duration(config.metrics.retention)
            );
            println!(
                "  storage cleanup     {} / backup {}",
                format_duration(config.storage.cleanup_interval),
                format_duration(config.storage.backup_interval)
            );
            Ok(())
        }
    }
}

/// `podnext doctor`: report what the host and the guest actually look like.
///
/// Exit status is driven by the worst verdict: a failing check exits 1. Warnings and
/// unknowns are printed without failing the command, because "not measured" is not the
/// same as "broken".
pub(crate) async fn cmd_doctor(container: Option<String>, format: OutputFormat) -> Result<()> {
    let checks = doctor::collect(container.as_deref()).await;
    match format {
        OutputFormat::Table | OutputFormat::Text => doctor::render_text(&checks),
        OutputFormat::Json => output_json(&checks)?,
        OutputFormat::Yaml => output_yaml(&checks)?,
    }
    if doctor::worst_verdict(&checks) == doctor::Verdict::Fail {
        anyhow::bail!("one or more checks failed; the rows marked `fail` name what and why");
    }
    Ok(())
}

pub(crate) async fn cmd_version(format: OutputFormat) -> Result<()> {
    let version_info = VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_commit: option_env!("GIT_COMMIT").unwrap_or("unknown").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("unknown").to_string(),
        rust_version: "1.80+".to_string(),
    };

    match format {
        OutputFormat::Table => output_table(&[version_info]),
        OutputFormat::Json => output_json(&version_info)?,
        OutputFormat::Yaml => output_yaml(&version_info)?,
        OutputFormat::Text => println!("podnext {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}

pub(crate) async fn cmd_completion(shell: Shell) -> Result<()> {
    let mut cmd = Cli::command();
    let name = "podnext".to_string();

    match shell {
        Shell::Bash => clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Zsh => clap_complete::generate(
            clap_complete::Shell::Zsh,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Fish => clap_complete::generate(
            clap_complete::Shell::Fish,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::PowerShell => clap_complete::generate(
            clap_complete::Shell::PowerShell,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Elvish => clap_complete::generate(
            clap_complete::Shell::Elvish,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
    }
    Ok(())
}

pub(crate) fn print_help() {
    let mut cmd = Cli::command();
    cmd.print_help().unwrap();
    println!();
}

#[derive(Tabled)]
pub(crate) struct SystemInfo {
    #[tabled(rename = "Version")]
    pub(crate) version: String,
    #[tabled(rename = "Platform")]
    pub(crate) platform: String,
    #[tabled(rename = "Rust Version")]
    pub(crate) rust_version: String,
    #[tabled(rename = "Backend")]
    pub(crate) backend: String,
    #[tabled(rename = "Isolation")]
    pub(crate) isolation: String,
}

#[derive(Tabled, serde::Serialize)]
pub(crate) struct VersionInfo {
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Git Commit")]
    git_commit: String,
    #[tabled(rename = "Build Date")]
    build_date: String,
    #[tabled(rename = "Rust Version")]
    rust_version: String,
}
