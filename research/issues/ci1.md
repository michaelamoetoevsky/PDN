## Summary

The `PDN CI/CD` workflow **fails on `master`**. After an explicit `stop_service` call,
`test_restart_policy_unless_stopped` leaves the service in `Failed` and not in `Stopped`. The
restart policy `UnlessStopped` races the explicit stop and wins.

## Reproduction

```sh
env -u LD_LIBRARY_PATH cargo test --workspace --no-fail-fast
# test test_restart_policy_unless_stopped ... FAILED
# tests/supervisor_e2e.rs:302:
#   assertion `left == right` failed
#     left: Failed
#    right: Stopped
```

CI run for `master` HEAD:

```
41744808292  failure  PDN CI/CD  master  push  2026-11-04T00:10:21Z  9m45s
  Rust Host Build: test_restart_policy_unless_stopped ... FAILED
  test result: FAILED. 18 passed; 1 failed
```

## Evidence from the CI log, verbatim

```
test test_restart_policy_unless_stopped ... FAILED
---- test_restart_policy_unless_stopped stdout ----
thread 'test_restart_policy_unless_stopped' (4095) panicked at tests/supervisor_e2e.rs:302:5:
assertion `left == right` failed
  left: Failed
 right: Stopped
error: test failed, to rerun pass `-p pdn-integration-tests --test supervisor_e2e`
```

## Root cause

The test body is at `tests/supervisor_e2e.rs:275`. The service command is `/bin/false`, which
always exits 1. The settings are `RestartCondition::UnlessStopped`, `max_restarts = 3` and
`base_delay_secs = 1`. After `stop_service(name, false)`, the test sleeps 3 s and expects
`Stopped`. By then the restart loop has already marked the service `Failed`. Thus the supervisor
does not let an explicit user stop pre-empt a restart decision that is in flight. Also, the
exit-code classifier treats an intentional stop as an abnormal exit.

## Impact

- CI is red on `master`. Every push reports a failing workflow.
- This is a real defect and not only a flaky test. An explicit stop can lose to the restart
  policy. `UnlessStopped` must prevent exactly that result.

## Proposed fix

Set and check the `user_stopped` flag *before* the evaluation of the restart decision. An
explicit stop then always wins the race. Make sure the exit-code classifier does not classify a
stop that the user initiates as abnormal. Such a stop has `exit_code > 128` or a signal. Set the
state to `Stopped` and not to `Failed`. Keep the test. If a sleep is genuinely necessary, poll
for the expected state and do not sleep for a fixed interval.

Labels: bug, ci
