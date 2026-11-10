## Summary

The `Benchmark` job is not predictable, and the system kills one step of it. The workflow
invokes the suite without bounds. Thus the criterion defaults apply to every case: 100 samples,
3 s warm-up and 5 s measurement. The defaults also apply to cases that build real sandboxes.

## Evidence

The same suite, two revisions, job limit of 60 minutes:

| Run | Revision | `Run benchmarks` | Job total |
|---|---|---|---|
| [41812140227](https://github.com/michaelamoetoevsky/PDN/actions/runs/41812140227) | `6c58b080` | success | 25.7 min |
| [41818454751](https://github.com/michaelamoetoevsky/PDN/actions/runs/41818454751) | `e3f9964` | still running at **43 min** | — |

The step that died, on the baseline comparison of PR #26:

```
2026-11-04T11:16:48  Running benches/benchmarks.rs (target/release/deps/benchmarks-72286bfad1e22bd3)
2026-11-04T11:16:48  Gnuplot not found, using plotters backend
2026-11-04T11:16:48  Benchmarking sandbox_creation/create_sandbox
2026-11-04T11:16:52  Benchmarking sandbox_creation/create_sandbox: Collecting 10 samples in estimated 30.000 s (16M iterations)
2026-11-04T11:17:39  ##[error]Process completed with exit code 143.
```

Exit 143 is SIGTERM, 51 seconds into that one case. It is **not** the job timeout. The job
started at 10:56:50 and died 20m49s into a 60-minute budget. No cancellation appears anywhere in
the log. `timeout-minutes: 60` is the same in the old workflow and the current workflow. This
fact was checked. The hypothesis that a shorter old limit caused the failure is wrong.

## Why this matters

1. **It blocks contributions.** The `Benchmark` check of PR #26 is red for a reason that is
   unrelated to the change of the contributor. Their branch carries an older workflow. The step
   that fails builds the **base** branch.
2. **A green benchmark job is not a signal when it takes 25 min at one time and more than 60
   at another time.** It is a coin flip on the job limit.
3. **The same lack of bounds causes problems locally.** `cargo bench` on the device peaks
   over 1 GB only to compile the bench profile, before any benchmark runs. `sandbox_creation`
   then adds more memory. That peak makes the Android low-memory killer select the Termux app.

## Fix applied

The fix bounds the suite in both benchmark steps:

```yaml
          cargo bench --workspace --bench benchmarks --locked -- \
            --warm-up-time 1 --measurement-time 3 --sample-size 10 2>&1 \
            | tee benchmark-output.txt
```

`--bench benchmarks` is necessary for the criterion flags to reach the criterion harness.
Without it, `cargo bench -- <args>` also sends the flags to the libtest harness of `lib`/`lib
test`. That harness rejects the flags with `error: Unrecognized option: 'warm-up-time'`. The fix
was verified on the device against the compiled bench binary. The run had zero "Unrecognized
option" output and emitted a real measurement:
`guardrails_evaluation/evaluate_request  time: [216.60 ns 219.34 ns 222.02 ns]`. The peak was
6 MB.

A CI benchmark job must be a smoke signal and a regression tripwire, not a measurement
laboratory. Full-suite numbers belong in a deliberate run.

## Still open

- **`sandbox_creation/create_sandbox` needs its own investigation.** It is the case that
  received SIGTERM and it is the most expensive case in the suite. It allocates a real sandbox
  inside a benchmark. The project has not yet established if the case is only slow, or if it
  starts a process that signals its own process group. Bounding hides the symptom, but it does
  not explain the symptom.
- **The 25.7 min -> 43 min growth between `6c58b080` and `e3f9964` is unexplained.** Something
  became materially slower in those commits. The bounded run will also make that growth
  invisible. That effect is the reason for this record, and not a memory of a slow CI day.

Labels: ci, performance
