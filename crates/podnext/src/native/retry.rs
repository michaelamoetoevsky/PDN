//! The transfer policy for the registry client (WORKLIST B2 and B3).
//!
//! Two decisions, each a pure function over plain data so it can be tested without a socket:
//!
//! - **Where a redirect may take the request, and whether the credentials travel with it** (B2;
//!   comparators `docker/transport.py:same_auth_origin` and `download.py:check_redirect`).
//! - **When a failed transfer may be repeated, how long to wait, and whether the body arrived
//!   whole** (B3; comparator `helpers/download.py`).
//!
//! Every socket stays in `image.rs`, which is the only caller. `now` is a parameter rather than a
//! call to the clock, so the date form of `Retry-After` is testable too.

use anyhow::{bail, Result};
use reqwest::Url;
use std::time::{Duration, SystemTime};

/// Attempts allowed for one layer, the first included.
pub const MAX_ATTEMPTS: u32 = 3;

/// Ceiling on a single wait, whatever `Retry-After` asks for.
pub const MAX_DELAY: Duration = Duration::from_secs(60);

/// May this status succeed on a repeat request?
///
/// Only the "try again later" family may. A 4xx outside 408 and 429 describes the request rather
/// than a moment, so it cannot succeed on a repeat; that is what "deterministic" means here (B3).
fn is_transient(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

/// What follows a failed attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Wait `delay` and try again.
    Retry { delay: Duration },
    /// Stop, for `reason`.
    GiveUp { reason: String },
}

/// Decide what follows a failed attempt (B3).
///
/// `status` is `None` for a transport error - a reset connection, a decode error - which is
/// transient by nature. `retry_after` is the raw `Retry-After` header, honoured in both forms it
/// may take; `now` is injected so the decision is a pure function of its arguments.
pub fn next_attempt(
    attempt: u32,
    status: Option<u16>,
    retry_after: Option<&str>,
    now: SystemTime,
) -> Decision {
    if let Some(status) = status {
        if !is_transient(status) {
            return Decision::GiveUp {
                reason: format!(
                    "status {status} is deterministic: a repeat request cannot succeed"
                ),
            };
        }
    }
    if attempt >= MAX_ATTEMPTS {
        return Decision::GiveUp {
            reason: format!("{attempt} attempts exhausted"),
        };
    }
    let delay = retry_after
        .and_then(|value| retry_after_delay(value, now))
        .unwrap_or_else(|| backoff(attempt))
        .min(MAX_DELAY);
    Decision::Retry { delay }
}

/// Exponential backoff for when the server gives no hint: 500 ms, 1 s, ...
fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500) * 2u32.saturating_pow(attempt.saturating_sub(1))
}

/// The `Retry-After` header as a wait. Both forms are accepted: delta-seconds, and an HTTP-date
/// (an IMF-fixdate, which is the RFC 2822 format). A date that has already passed yields no hint
/// and the caller falls back to backoff.
pub fn retry_after_delay(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let when = chrono::DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&chrono::Utc);
    let now: chrono::DateTime<chrono::Utc> = now.into();
    when.signed_duration_since(now).to_std().ok()
}

/// Verify the body is as long as the server said it would be (B3).
///
/// A body cut short is the quiet half of a truncated response: the read succeeds, the layer is
/// short, and the digest check downstream reports a mismatch that hides the real cause. A missing
/// `Content-Length` (a chunked response) declares nothing and is accepted here - reqwest itself
/// errors on a chunked stream that ends early.
pub fn ensure_complete_body(declared: Option<u64>, received: u64) -> Result<()> {
    match declared {
        Some(declared) if declared != received => {
            bail!("truncated body: declared {declared} bytes, received {received}")
        }
        _ => Ok(()),
    }
}

/// Where a redirect may take the request, and whether the credentials travel with it.
#[derive(Debug, PartialEq, Eq)]
pub enum Redirect {
    /// Same scheme, host and port: the request continues with its credentials.
    Follow,
    /// A different origin: follow, but the credentials do not travel. reqwest drops the sensitive
    /// headers when the host or port changes; this variant names that decision.
    FollowWithoutCredentials,
    /// Refuse the redirect outright.
    Refuse(String),
}

/// Decide where a redirect may go, and whether the credentials travel with it (B2).
///
/// The hole this closes: comparing netlocs alone let a same-host redirect that changed only the
/// scheme keep the `Authorization` header, so `https://reg.example -> http://reg.example` put the
/// bearer token on the wire in clear (`docker/transport.py:same_auth_origin`). The scheme is part
/// of the origin here, and an HTTPS-to-HTTP downgrade is refused as well
/// (`download.py:check_redirect`), which is what makes the transfer fail rather than continue
/// uncredentialed.
pub fn plan_redirect(from: &Url, to: &Url) -> Redirect {
    let same_origin = from.scheme() == to.scheme()
        && from.host_str() == to.host_str()
        && from.port_or_known_default() == to.port_or_known_default();
    if same_origin {
        return Redirect::Follow;
    }
    if from.scheme() == "https" && to.scheme() == "http" {
        return Redirect::Refuse(format!(
            "refusing HTTPS -> HTTP downgrade {from} -> {to}: \
             the Authorization header would leave the origin in clear"
        ));
    }
    Redirect::FollowWithoutCredentials
}
