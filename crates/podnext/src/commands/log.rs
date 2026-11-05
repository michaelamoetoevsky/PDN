//! Structured log commands: `podnext log query`, `tail`, `stats` and `flush`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use log::*;` in `commands/mod.rs`.

use anyhow::Result;
use serde::Serialize;
use tabled::Tabled;

use pn_next::{LogLevel, LogQuery, StructuredLogging, StructuredLoggingConfig};

use super::{output_json, output_table, output_yaml};
use crate::utils::format_size;
pub async fn execute_log_command(cmd: crate::LogCommands) -> Result<()> {
    match cmd {
        crate::LogCommands::Query {
            service,
            min_level,
            max_level,
            grep,
            limit,
            format,
        } => cmd_log_query(service, min_level, max_level, grep, limit, format).await,
        crate::LogCommands::Tail { service, lines } => cmd_log_tail(service, lines).await,
        crate::LogCommands::Stats { format } => cmd_log_stats(format).await,
        crate::LogCommands::Flush => cmd_log_flush().await,
    }
}

pub async fn cmd_log_query(
    service: Option<String>,
    min_level: Option<crate::LogLevelArg>,
    max_level: Option<crate::LogLevelArg>,
    grep: Option<String>,
    limit: usize,
    format: crate::OutputFormat,
) -> Result<()> {
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;

    let query = LogQuery {
        service,
        min_level: min_level.map(|l| match l {
            crate::LogLevelArg::Emergency => LogLevel::Emergency,
            crate::LogLevelArg::Alert => LogLevel::Alert,
            crate::LogLevelArg::Critical => LogLevel::Critical,
            crate::LogLevelArg::Error => LogLevel::Error,
            crate::LogLevelArg::Warning => LogLevel::Warning,
            crate::LogLevelArg::Notice => LogLevel::Notice,
            crate::LogLevelArg::Info => LogLevel::Info,
            crate::LogLevelArg::Debug => LogLevel::Debug,
        }),
        max_level: max_level.map(|l| match l {
            crate::LogLevelArg::Emergency => LogLevel::Emergency,
            crate::LogLevelArg::Alert => LogLevel::Alert,
            crate::LogLevelArg::Critical => LogLevel::Critical,
            crate::LogLevelArg::Error => LogLevel::Error,
            crate::LogLevelArg::Warning => LogLevel::Warning,
            crate::LogLevelArg::Notice => LogLevel::Notice,
            crate::LogLevelArg::Info => LogLevel::Info,
            crate::LogLevelArg::Debug => LogLevel::Debug,
        }),
        grep,
        limit: Some(limit),
        ..Default::default()
    };

    let results = logging.query(query).await?;

    #[derive(Tabled, Serialize)]
    struct LogRow {
        #[tabled(rename = "ID")]
        id: u64,
        #[tabled(rename = "LEVEL")]
        level: String,
        #[tabled(rename = "SERVICE")]
        service: String,
        #[tabled(rename = "MESSAGE")]
        message: String,
    }

    let rows: Vec<LogRow> = results
        .iter()
        .map(|e| LogRow {
            id: e.id,
            level: format!("{:?}", e.level),
            service: e.service.clone(),
            message: e.message.clone(),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&results)?,
        crate::OutputFormat::Yaml => output_yaml(&results)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!("{} [{}] {}: {}", r.id, r.level, r.service, r.message);
            }
        }
    }
    Ok(())
}

pub async fn cmd_log_tail(service: Option<String>, lines: usize) -> Result<()> {
    // The log store already backs `log query`; tailing is the same query with
    // the newest entries and no filters beyond the service.
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;

    let mut entries = logging
        .query(LogQuery {
            service: service.clone(),
            limit: Some(lines),
            ..Default::default()
        })
        .await?;

    entries.sort_by_key(|e| e.id);
    let start = entries.len().saturating_sub(lines);

    for e in &entries[start..] {
        println!("{:>6} [{:?}] {}: {}", e.id, e.level, e.service, e.message);
    }

    // One-shot: `tail -f` semantics would need a subscription, and saying so is
    // better than appearing to follow.
    match service {
        Some(s) => println!(
            "--- {} entr{} for service '{}' (snapshot, not following) ---",
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" },
            s
        ),
        None => println!(
            "--- {} entr{} (snapshot, not following) ---",
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" }
        ),
    }
    Ok(())
}

pub async fn cmd_log_stats(format: crate::OutputFormat) -> Result<()> {
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;
    let stats = logging.get_stats().await?;

    #[derive(Tabled, Serialize)]
    struct StatsRow {
        #[tabled(rename = "METRIC")]
        metric: String,
        #[tabled(rename = "VALUE")]
        value: String,
    }

    let rows = vec![
        StatsRow {
            metric: "Total Entries".to_string(),
            value: stats.total_entries.to_string(),
        },
        StatsRow {
            metric: "Total Size".to_string(),
            value: format_size(stats.total_size_bytes),
        },
        StatsRow {
            metric: "File Count".to_string(),
            value: stats.file_count.to_string(),
        },
    ];

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&stats)?,
        crate::OutputFormat::Yaml => output_yaml(&stats)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!("{}: {}", r.metric, r.value);
            }
        }
    }
    Ok(())
}

pub async fn cmd_log_flush() -> Result<()> {
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;
    logging.flush().await?;
    println!("Log buffer flushed");
    Ok(())
}
