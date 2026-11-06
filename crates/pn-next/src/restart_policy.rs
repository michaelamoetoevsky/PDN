//! B14 - the restart decision, as a pure module.
//!
//! `supervisor::RestartPolicy` says what the operator wants. This module says what
//! the supervisor must do next, and it says it over plain values only: no process,
//! no clock, no lock, no `async`. The whole decision table therefore runs in a unit
//! test (`tests/restart_policy.rs`) with no subprocess and no supervisor.
//!
//! The runtime in `supervisor.rs` calls these functions, so the table and the loop
//! cannot drift: one exit code, one manual stop, one window have exactly one verdict.

use crate::supervisor::{RestartCondition, RestartPolicy};

/// What ended the service, as the supervisor sees it.
///
/// `Exited(None)` is the Unix signal kill that leaves no exit code; `ManualStop` is
/// an operator stop, which the reference always treats as final for the *policy*
/// (the OnAbnormal force-kill path is modelled as `Exited`, not as this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceOutcome {
    /// The process ended on its own with this status (`None` = signal, no code).
    Exited(Option<i32>),
    /// An operator stopped it deliberately.
    ManualStop,
}

/// Why a restart was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveUpReason {
    /// The policy's `condition` does not match this outcome.
    ConditionNotMet,
    /// The circuit breaker is open.
    CircuitOpen,
    /// More attempts than `max_restarts` already fall inside the window.
    ExceededMaxRestarts,
}

/// What the supervisor must do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartDecision {
    /// Restart with no wait (the computed backoff is zero).
    RestartNow,
    /// Restart after this many seconds of backoff.
    RestartAfter { delay_secs: u64 },
    /// Do not restart, and this is the reason.
    GiveUp { reason: GiveUpReason },
}

/// The plain inputs the decision reads. Every field is a value the caller already
/// holds; the circuit breaker's own state is reduced to `circuit_open` because it
/// mutates per failure and so cannot live inside a pure function.
#[derive(Debug, Clone, Copy)]
pub struct RestartInput<'a> {
    pub policy: &'a RestartPolicy,
    pub outcome: ServiceOutcome,
    /// Restarts already performed inside the current window.
    pub attempts_used: u32,
    /// Whether `attempts_used` fall inside the current window; when `false` the
    /// window is empty and this attempt is the first of a new one.
    pub elapsed_in_window: bool,
    /// The circuit breaker's verdict, already computed by the caller.
    pub circuit_open: bool,
}

/// Does the policy's `condition` accept this outcome?
///
/// A manual stop is final for every condition: the reference loop re-checks
/// `user_stopped` after the backoff and cancels, so folding the stop in here is the
/// same verdict one step earlier. (`OnAbnormal` on a force kill is `Exited`, which
/// is what makes the force-kill restart path work.)
pub fn condition_matches(condition: RestartCondition, outcome: ServiceOutcome) -> bool {
    match (condition, outcome) {
        (_, ServiceOutcome::ManualStop) => false,
        (RestartCondition::Never, _) => false,
        (RestartCondition::Always, _) => true,
        (RestartCondition::OnFailure, ServiceOutcome::Exited(code)) => {
            code.map_or(false, |c| c != 0)
        }
        (RestartCondition::OnAbnormal, ServiceOutcome::Exited(code)) => {
            // No code (signal), or the shell's 128+signal encoding.
            code.map_or(true, |c| c < 0 || c > 128)
        }
        (RestartCondition::UnlessStopped, ServiceOutcome::Exited(_)) => true,
    }
}

/// The attempt number this exit is: 1 for the first of a window, else one more than
/// the count already inside it. Mirrors `restart_count + 1` when the last restart is
/// recent, and `1` when the window has rolled over.
pub fn recent_restarts(attempts_used: u32, elapsed_in_window: bool) -> u32 {
    if elapsed_in_window {
        attempts_used + 1
    } else {
        1
    }
}

/// Exponential backoff capped at `max_delay_secs`: `base * multiplier^attempt`.
pub fn backoff_secs(policy: &RestartPolicy, attempt: u32) -> u64 {
    std::cmp::min(
        (policy.base_delay_secs as f64 * policy.backoff_multiplier.powi(attempt as i32)) as u64,
        policy.max_delay_secs,
    )
}

/// The whole decision: circuit, condition, budget, then delay.
///
/// The runtime composes it from [`condition_matches`], [`recent_restarts`] and
/// [`backoff_secs`] because it must record a circuit failure between the condition
/// and the budget; this entry point is the table the tests assert against.
pub fn decide(input: RestartInput<'_>) -> RestartDecision {
    if input.circuit_open {
        return RestartDecision::GiveUp {
            reason: GiveUpReason::CircuitOpen,
        };
    }
    if !condition_matches(input.policy.condition, input.outcome) {
        return RestartDecision::GiveUp {
            reason: GiveUpReason::ConditionNotMet,
        };
    }
    let attempt = recent_restarts(input.attempts_used, input.elapsed_in_window);
    if attempt > input.policy.max_restarts {
        return RestartDecision::GiveUp {
            reason: GiveUpReason::ExceededMaxRestarts,
        };
    }
    let delay_secs = backoff_secs(input.policy, attempt);
    if delay_secs == 0 {
        RestartDecision::RestartNow
    } else {
        RestartDecision::RestartAfter { delay_secs }
    }
}
