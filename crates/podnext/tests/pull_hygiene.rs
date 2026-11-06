//! Pull hygiene: a digest is validated before it becomes a path, the credentials stay bound to
//! their origin, and a failed transfer is repeated only when a repeat can help (WORKLIST B1-B3).
//!
//! All three are pure functions over plain data in `src/native/digest.rs` and `src/native/retry.rs`;
//! every socket stays in `src/native/image.rs`. Nothing in this file opens one: this device's link
//! to Docker Hub is intermittent (a 30 MB pull failed with `Error: error decoding response body`
//! while a 3 MB pull succeeded), so no gate may depend on a transfer.
//!
//! The modules are compiled from their real source rather than re-declared, so these exercise the
//! shipped code and not a copy of it.

#[path = "../src/native/digest.rs"]
mod digest;
#[path = "../src/native/retry.rs"]
mod retry;

use reqwest::Url;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn url(s: &str) -> Url {
    Url::parse(s).expect("test url")
}

/// The digest form the registry actually serves, for the cases that must pass.
const VALID: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

// ===== B1 - digest validation before path composition =====

/// B1 acceptance: a crafted digest containing `..` is refused with the value quoted.
#[test]
fn b1_a_digest_with_parent_components_is_refused_with_the_value_quoted() {
    let crafted = "sha256:../../../etc/shadow";
    let error = digest::validate_digest(crafted)
        .expect_err("a digest that walks out of its directory must be refused")
        .to_string();
    assert!(
        error.contains("\"sha256:../../../etc/shadow\""),
        "the refusal must quote the offending value: {error}"
    );
}

#[test]
fn b1_a_well_formed_sha256_digest_is_accepted() {
    assert!(digest::validate_digest(VALID).is_ok());
}

/// Everything that is not exactly `<sha256>:<64 lowercase hex>` is refused, and each refusal names
/// the value - the manifest, not the caller, chooses these, so none of them may reach a path.
#[test]
fn b1_anything_that_is_not_sha256_hex_is_refused() {
    for bad in [
        "",
        "sha256:",
        "sha256:abc",                 // too short
        "sha256:../../../etc/shadow", // traversal
        "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08/../../x", // separator
        "sha256:9F86D081884C7D659A2FEAA0C55AD015A3BF4F1B2B0B822CD15D6C15B0F00A08", // uppercase
        "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a0g", // not hex
        "sha512:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08", // other alg
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",        // no algorithm
    ] {
        let error = digest::validate_digest(bad)
            .expect_err(&format!("{bad:?} must be refused"))
            .to_string();
        assert!(
            error.contains(&format!("{bad:?}")),
            "the refusal must quote {bad:?}: {error}"
        );
    }
}

// ===== B2 - origin-bound credentials, refused downgrade =====

/// B2 acceptance: a downgrade drops the header and the transfer fails.
#[test]
fn b2_a_same_host_downgrade_drops_the_credentials_and_fails_the_transfer() {
    let plan = retry::plan_redirect(
        &url("https://registry-1.docker.io/v2/library/alpine/blobs/sha256:abc"),
        &url("http://registry-1.docker.io/v2/library/alpine/blobs/sha256:abc"),
    );
    match &plan {
        retry::Redirect::Refuse(reason) => {
            assert!(
                reason.contains("clear"),
                "the refusal must say the token would travel in clear: {reason}"
            );
            assert!(
                reason.contains("https://") && reason.contains("http://"),
                "the refusal must name the addresses: {reason}"
            );
        }
        other => {
            panic!("a downgrade must fail the transfer, not continue uncredentialed: {other:?}")
        }
    }
    assert_ne!(
        plan,
        retry::Redirect::Follow,
        "following would put the bearer token on a cleartext connection"
    );
}

#[test]
fn b2_a_same_origin_redirect_keeps_the_credentials() {
    assert_eq!(
        retry::plan_redirect(
            &url("https://reg.example/v2/a"),
            &url("https://reg.example/v2/b")
        ),
        retry::Redirect::Follow
    );
}

/// Credentials are bound to the origin: another host, or the same host on another port, is another
/// origin and the header does not go there.
#[test]
fn b2_a_redirect_to_another_origin_drops_the_credentials() {
    assert_eq!(
        retry::plan_redirect(
            &url("https://reg.example/v2/a"),
            &url("https://cdn.example/v2/a")
        ),
        retry::Redirect::FollowWithoutCredentials
    );
    assert_eq!(
        retry::plan_redirect(
            &url("https://reg.example/v2/a"),
            &url("https://reg.example:8443/v2/a")
        ),
        retry::Redirect::FollowWithoutCredentials
    );
}

// ===== B3 - a real retry policy =====

/// B3 acceptance: a 404 is not retried.
#[test]
fn b3_a_404_is_not_retried() {
    match retry::next_attempt(1, Some(404), None, at(0)) {
        retry::Decision::GiveUp { reason } => {
            assert!(
                reason.contains("404"),
                "the reason names the status: {reason}"
            )
        }
        other => panic!("a 404 describes the request, not the moment: {other:?}"),
    }
    assert!(matches!(
        retry::next_attempt(1, Some(401), None, at(0)),
        retry::Decision::GiveUp { .. }
    ));
}

#[test]
fn b3_a_transient_status_is_retried_until_the_budget_runs_out() {
    for status in [408, 429, 500, 502, 503, 504] {
        assert!(
            matches!(
                retry::next_attempt(1, Some(status), None, at(0)),
                retry::Decision::Retry { .. }
            ),
            "status {status} is transient"
        );
    }
    // No response at all (a transport error) is transient by nature.
    assert!(matches!(
        retry::next_attempt(1, None, None, at(0)),
        retry::Decision::Retry { .. }
    ));
    assert!(matches!(
        retry::next_attempt(retry::MAX_ATTEMPTS, Some(503), None, at(0)),
        retry::Decision::GiveUp { .. }
    ));
}

#[test]
fn b3_retry_after_in_seconds_is_honoured() {
    match retry::next_attempt(1, Some(503), Some("7"), at(0)) {
        retry::Decision::Retry { delay } => assert_eq!(delay, Duration::from_secs(7)),
        other => panic!("Retry-After must set the wait: {other:?}"),
    }
}

#[test]
fn b3_retry_after_as_an_http_date_is_honoured() {
    // 1994-11-06T08:49:37Z is 784111777 in Unix seconds; ask from 30 s before it.
    let when = 784_111_777;
    match retry::next_attempt(
        1,
        Some(503),
        Some("Sun, 06 Nov 1994 08:49:37 GMT"),
        at(when - 30),
    ) {
        retry::Decision::Retry { delay } => assert_eq!(delay, Duration::from_secs(30)),
        other => panic!("the HTTP-date form of Retry-After must set the wait: {other:?}"),
    }
}

#[test]
fn b3_a_retry_after_beyond_the_ceiling_is_clamped() {
    match retry::next_attempt(1, Some(503), Some("3600"), at(0)) {
        retry::Decision::Retry { delay } => assert_eq!(delay, retry::MAX_DELAY),
        other => panic!("an hour-long wait must be clamped: {other:?}"),
    }
}

/// B3 acceptance: a truncated body fails reporting both lengths.
#[test]
fn b3_a_truncated_body_fails_reporting_both_lengths() {
    let error = retry::ensure_complete_body(Some(4096), 1024)
        .expect_err("a body cut short of its declared length is a failed transfer")
        .to_string();
    assert!(
        error.contains("4096"),
        "the declared length must be reported: {error}"
    );
    assert!(
        error.contains("1024"),
        "the received length must be reported: {error}"
    );
}

#[test]
fn b3_a_complete_body_and_an_undeclared_length_are_accepted() {
    assert!(retry::ensure_complete_body(Some(1024), 1024).is_ok());
    assert!(
        retry::ensure_complete_body(None, 1024).is_ok(),
        "a chunked response declares no length; reqwest errors on one that ends early"
    );
}
