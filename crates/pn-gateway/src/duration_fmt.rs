//! Human-readable durations in configuration files.
//!
//! `std::time::Duration`'s own serde representation is a struct -
//! `{ secs = 60, nanos = 0 }` - but `docs/src/configuration.md` documents the
//! form every operator expects:
//!
//! ```toml
//! gateway_timeout = "30s"
//! retry_delay = "500ms"
//! ```
//!
//! Until this module existed those documented values could not be loaded at
//! all: `Duration` failed to deserialise a string, and because every config
//! struct is `#[serde(deny_unknown_fields)]` the failure was loud but the
//! documented configuration was still unusable. See issue #34.
//!
//! Accepted inputs, deliberately chosen to match the documentation and common
//! practice rather than to be clever:
//!
//! | input | meaning |
//! |---|---|
//! | `"30s"`, `"30sec"`, `"30secs"`, `"30second"`, `"30seconds"` | seconds |
//! | `"500ms"`, `"500millis"` | milliseconds |
//! | `"2m"`, `"2min"`, `"2mins"`, `"2minute"`, `"2minutes"` | minutes |
//! | `"1h"`, `"1hr"`, `"1hour"`, `"1hours"` | hours |
//! | `"1d"`, `"1day"`, `"1days"` | days |
//! | `30` (bare integer) | seconds, for TOML files written before this existed |
//! | `{ secs = 30, nanos = 0 }` | the native `Duration` form, still accepted so no existing file breaks |
//!
//! Whitespace around the value and between the number and the unit is ignored
//! (`" 30 s "` works). Anything else is an error that names the offending value
//! and the accepted forms - a silent fallback to zero would turn a typo into a
//! disabled timeout, which is the failure mode this whole area is trying to
//! stop.

use serde::{Deserialize, Deserializer, Serializer};
use std::time::Duration;

/// Parse a duration written the way the docs write it.
///
/// Split out from the serde glue so it can be tested directly and reused.
pub fn parse_duration(input: &str) -> Result<Duration, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty duration".to_string());
    }

    // Split into digits and a unit. A leading sign is not meaningful here and is
    // rejected rather than silently accepted, because `-5s` is a typo, not a wait.
    let split = trimmed
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != ' ')
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let number = number.trim();
    let unit = unit.trim().to_ascii_lowercase();

    if number.is_empty() {
        return Err(format!(
            "duration {input:?} has no number; expected forms like \"30s\", \"500ms\", \"2m\", \"1h\""
        ));
    }

    let magnitude: f64 = number.parse().map_err(|_| {
        format!("duration {input:?} has an unparseable number {number:?}; expected forms like \"30s\", \"500ms\"")
    })?;
    if !magnitude.is_finite() || magnitude < 0.0 {
        return Err(format!(
            "duration {input:?} must be a finite, non-negative number"
        ));
    }

    let multiplier = match unit.as_str() {
        // Bare numbers are seconds: TOML files predating this module wrote
        // `timeout = 30`, and refusing them would break working deployments for
        // no gain.
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
        "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => 0.001,
        "us" | "micro" | "micros" | "microsecond" | "microseconds" => 0.000_001,
        "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600.0,
        "d" | "day" | "days" => 86_400.0,
        other => {
            return Err(format!(
                "unknown duration unit {other:?} in {input:?}; expected one of ms, s, m, h, d"
            ))
        }
    };

    let seconds = magnitude * multiplier;
    if seconds > u64::MAX as f64 {
        return Err(format!("duration {input:?} overflows"));
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// Render a duration the way the docs write it, so a config round-trips.
pub fn format_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos % 86_400_000_000_000 == 0 {
        format!("{}d", nanos / 86_400_000_000_000)
    } else if nanos % 3_600_000_000_000 == 0 {
        format!("{}h", nanos / 3_600_000_000_000)
    } else if nanos % 60_000_000_000 == 0 {
        format!("{}m", nanos / 60_000_000_000)
    } else if nanos % 1_000_000_000 == 0 {
        format!("{}s", nanos / 1_000_000_000)
    } else if nanos % 1_000_000 == 0 {
        format!("{}ms", nanos / 1_000_000)
    } else {
        format!("{}s", d.as_secs_f64())
    }
}

/// Native `Duration` shape, kept so files written before this module still load.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeDuration {
    secs: u64,
    #[serde(default)]
    nanos: u32,
}

/// `#[serde(with = "crate::duration_fmt")]` for `Duration` fields.
pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Text(String),
        Seconds(u64),
        Native(NativeDuration),
    }

    match Repr::deserialize(deserializer)? {
        Repr::Text(s) => parse_duration(&s).map_err(serde::de::Error::custom),
        Repr::Seconds(secs) => Ok(Duration::from_secs(secs)),
        Repr::Native(n) => Ok(Duration::new(n.secs, n.nanos)),
    }
}

/// `#[serde(with = "crate::duration_fmt")]` for `Duration` fields.
pub fn serialize<S>(d: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format_duration(*d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_forms_parse() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration(" 30 s ").unwrap(), Duration::from_secs(30));
        assert_eq!(
            parse_duration("1500ms").unwrap(),
            Duration::from_millis(1500)
        );
    }

    #[test]
    fn bare_number_and_native_form_still_load() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
        let native: NativeDuration = toml::from_str("secs = 90\nnanos = 0").unwrap();
        assert_eq!(native.secs, 90);
    }

    #[test]
    fn garbage_is_rejected_with_the_value_named() {
        for bad in ["", "abc", "s30", "-5s", "30 lightyears", "1e3s"] {
            let err = parse_duration(bad).expect_err(bad);
            assert!(
                err.contains(bad.trim().is_empty().then_some("x").unwrap_or(bad.trim()))
                    || err.contains("empty")
                    || err.contains("no number"),
                "error for {bad:?} should name the value: {err}"
            );
        }
    }

    #[test]
    fn round_trips_in_the_documented_form() {
        for (d, expected) in [
            (Duration::from_secs(30), "30s"),
            (Duration::from_millis(500), "500ms"),
            (Duration::from_secs(120), "2m"),
            (Duration::from_secs(3600), "1h"),
            (Duration::from_secs(300), "5m"),
            (Duration::from_secs(604800), "7d"),
        ] {
            assert_eq!(format_duration(d), expected);
            assert_eq!(parse_duration(expected).unwrap(), d);
        }
    }
}
