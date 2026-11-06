//! B14 - the restart decision, asserted over the policy alone.
//!
//! No supervisor, no subprocess, no clock, no sleep: every row below is plain data
//! in and one verdict out, so an exit code, a manual stop or a backoff duration is
//! checked by equality rather than by watching a process. The table is printed with
//! `-- --nocapture` so the reader can see exactly which inputs map to which verdict.

use pn_next::restart_policy::{
    backoff_secs, condition_matches, decide, recent_restarts, GiveUpReason, RestartDecision,
    RestartInput, ServiceOutcome,
};
use pn_next::{RestartCondition, RestartPolicy};

fn policy(condition: RestartCondition) -> RestartPolicy {
    RestartPolicy {
        condition,
        ..RestartPolicy::default()
    }
}

fn verdict(
    policy: &RestartPolicy,
    outcome: ServiceOutcome,
    attempts_used: u32,
    elapsed_in_window: bool,
    circuit_open: bool,
) -> RestartDecision {
    decide(RestartInput {
        policy,
        outcome,
        attempts_used,
        elapsed_in_window,
        circuit_open,
    })
}

fn show(d: RestartDecision) -> String {
    match d {
        RestartDecision::RestartNow => "restart-now".to_string(),
        RestartDecision::RestartAfter { delay_secs } => format!("restart-after({delay_secs}s)"),
        RestartDecision::GiveUp { reason } => match reason {
            GiveUpReason::ConditionNotMet => "give-up(condition-not-met)".to_string(),
            GiveUpReason::CircuitOpen => "give-up(circuit-open)".to_string(),
            GiveUpReason::ExceededMaxRestarts => "give-up(exceeded-max-restarts)".to_string(),
        },
    }
}

fn show_outcome(o: ServiceOutcome) -> String {
    match o {
        ServiceOutcome::Exited(Some(c)) => format!("exit({c})"),
        ServiceOutcome::Exited(None) => "exit(signal,no-code)".to_string(),
        ServiceOutcome::ManualStop => "manual-stop".to_string(),
    }
}

/// Exit codes: the condition accepts or refuses by the code, and nothing else.
#[test]
fn exit_codes_pick_the_condition() {
    let cases: &[(RestartCondition, ServiceOutcome, RestartDecision)] = &[
        (
            RestartCondition::OnFailure,
            ServiceOutcome::Exited(Some(0)),
            RestartDecision::GiveUp {
                reason: GiveUpReason::ConditionNotMet,
            },
        ),
        (
            RestartCondition::OnFailure,
            ServiceOutcome::Exited(Some(1)),
            RestartDecision::RestartAfter { delay_secs: 2 },
        ),
        (
            RestartCondition::OnFailure,
            ServiceOutcome::Exited(Some(137)),
            RestartDecision::RestartAfter { delay_secs: 2 },
        ),
        (
            RestartCondition::OnFailure,
            ServiceOutcome::Exited(None),
            RestartDecision::GiveUp {
                reason: GiveUpReason::ConditionNotMet,
            },
        ),
        (
            RestartCondition::Always,
            ServiceOutcome::Exited(Some(0)),
            RestartDecision::RestartAfter { delay_secs: 2 },
        ),
        (
            RestartCondition::Always,
            ServiceOutcome::Exited(None),
            RestartDecision::RestartAfter { delay_secs: 2 },
        ),
        (
            RestartCondition::Never,
            ServiceOutcome::Exited(Some(1)),
            RestartDecision::GiveUp {
                reason: GiveUpReason::ConditionNotMet,
            },
        ),
        (
            RestartCondition::OnAbnormal,
            ServiceOutcome::Exited(None),
            RestartDecision::RestartAfter { delay_secs: 2 },
        ),
        (
            RestartCondition::OnAbnormal,
            ServiceOutcome::Exited(Some(137)),
            RestartDecision::RestartAfter { delay_secs: 2 },
        ),
        (
            RestartCondition::OnAbnormal,
            ServiceOutcome::Exited(Some(1)),
            RestartDecision::GiveUp {
                reason: GiveUpReason::ConditionNotMet,
            },
        ),
        (
            RestartCondition::OnAbnormal,
            ServiceOutcome::Exited(Some(0)),
            RestartDecision::GiveUp {
                reason: GiveUpReason::ConditionNotMet,
            },
        ),
    ];

    println!("condition       outcome                 attempts  elapsed  -> verdict");
    for (condition, outcome, expected) in cases {
        let p = policy(*condition);
        let got = verdict(&p, *outcome, 0, false, false);
        println!(
            "{:<15} {:<23} {:<9} {:<8} -> {}",
            format!("{condition:?}"),
            show_outcome(*outcome),
            0,
            false,
            show(got)
        );
        assert_eq!(
            got,
            *expected,
            "condition={condition:?} outcome={}",
            show_outcome(*outcome)
        );
    }
}

/// A manual stop is final for every condition: no verdict of this table restarts a
/// service an operator has stopped.
#[test]
fn manual_stops_never_restart() {
    let conditions = [
        RestartCondition::Never,
        RestartCondition::OnFailure,
        RestartCondition::Always,
        RestartCondition::UnlessStopped,
        RestartCondition::OnAbnormal,
    ];
    println!("\nmanual stop:");
    for condition in conditions {
        let p = policy(condition);
        let got = verdict(&p, ServiceOutcome::ManualStop, 0, false, false);
        println!("{:<15} -> {}", format!("{condition:?}"), show(got));
        assert_eq!(
            got,
            RestartDecision::GiveUp {
                reason: GiveUpReason::ConditionNotMet
            },
            "a manual stop under {condition:?} must not restart"
        );
        assert!(!condition_matches(condition, ServiceOutcome::ManualStop));
    }
}

/// Backoff durations: `base * multiplier^attempt`, capped at `max_delay`, with the
/// attempt number derived from the window. `RestartNow` is exactly a zero delay.
#[test]
fn durations_come_from_the_policy() {
    let mut p = policy(RestartCondition::Always);
    p.base_delay_secs = 1;
    p.backoff_multiplier = 2.0;
    p.max_delay_secs = 60;
    p.max_restarts = 100;

    let rows: &[(u32, bool, u32, u64)] = &[
        (0, false, 1, 2), // first of a window: attempt 1 -> 1*2^1
        (0, true, 1, 2),  // restart_count 0, in window: attempt 1
        (1, true, 2, 4),  // attempt 2 -> 1*2^2
        (2, true, 3, 8),  // attempt 3 -> 1*2^3
        (5, true, 6, 60), // attempt 6 -> 64, capped to 60
        (9, false, 1, 2), // window rolled over: back to attempt 1
    ];
    println!("\ndurations (base=1s multiplier=2 max=60s):");
    for (attempts_used, elapsed, attempt, delay) in rows {
        assert_eq!(
            recent_restarts(*attempts_used, *elapsed),
            *attempt,
            "attempt number for attempts_used={attempts_used} elapsed_in_window={elapsed}"
        );
        assert_eq!(
            backoff_secs(&p, *attempt),
            *delay,
            "delay for attempt {attempt}"
        );
        let got = verdict(
            &p,
            ServiceOutcome::Exited(Some(1)),
            *attempts_used,
            *elapsed,
            false,
        );
        println!(
            "attempts_used={attempts_used} elapsed_in_window={elapsed} -> attempt {attempt} -> {}",
            show(got)
        );
        assert_eq!(got, RestartDecision::RestartAfter { delay_secs: *delay });
    }

    let mut immediate = policy(RestartCondition::Always);
    immediate.base_delay_secs = 0;
    println!(
        "base_delay=0 -> {}",
        show(verdict(
            &immediate,
            ServiceOutcome::Exited(Some(1)),
            0,
            false,
            false
        ))
    );
    assert_eq!(
        verdict(&immediate, ServiceOutcome::Exited(Some(1)), 0, false, false),
        RestartDecision::RestartNow
    );
}

/// The restart budget: once the attempt number exceeds `max_restarts` the verdict is
/// refused, and a window that has rolled over grants a fresh attempt.
#[test]
fn budget_and_window() {
    let mut p = policy(RestartCondition::Always);
    p.max_restarts = 2;
    p.base_delay_secs = 1;
    p.max_delay_secs = 60;
    p.backoff_multiplier = 2.0;

    println!("\nbudget (max_restarts=2):");
    let cases: &[(u32, bool, RestartDecision)] = &[
        (0, false, RestartDecision::RestartAfter { delay_secs: 2 }),
        (1, true, RestartDecision::RestartAfter { delay_secs: 4 }),
        (
            2,
            true,
            RestartDecision::GiveUp {
                reason: GiveUpReason::ExceededMaxRestarts,
            },
        ),
        (5, false, RestartDecision::RestartAfter { delay_secs: 2 }),
    ];
    for (attempts_used, elapsed, expected) in cases {
        let got = verdict(
            &p,
            ServiceOutcome::Exited(Some(1)),
            *attempts_used,
            *elapsed,
            false,
        );
        println!(
            "attempts_used={attempts_used} elapsed_in_window={elapsed} -> {}",
            show(got)
        );
        assert_eq!(got, *expected);
    }

    // An open circuit outranks the condition and the budget.
    let got = verdict(&p, ServiceOutcome::Exited(Some(1)), 0, false, true);
    println!("circuit_open=true -> {}", show(got));
    assert_eq!(
        got,
        RestartDecision::GiveUp {
            reason: GiveUpReason::CircuitOpen
        }
    );
}
