## Summary

The `Benchmark` workflow reported **success for a run that failed**. `cargo bench` did not
compile the benchmark targets. The run had 26 errors from API drift. The failure never reached
the job. Every command in the step was piped, and the GitHub Actions default `bash -e` does not
set `pipefail`.

```
      - name: Run benchmarks
        run: |
          cargo bench --workspace --locked --no-run 2>&1 | tail -20
          cargo bench --workspace --locked 2>&1 | tee benchmark-output.txt
```

The exit status of a pipeline is the status of the **last** command. `tail` and `tee` exit 0,
so `##[error]Process completed with exit code 101` never occurred. The workflow has been 15/15
green and it measured nothing.

This defect is in the same class as the `|| true` gates of D5. It is a check that cannot fail.
`research/CI_CD_AUDIT.md` D12 had already flagged the workflow as misleading. But D12 attributed
the problem to the wrong mechanism. The quoted reason was "no `[[bench]]` targets exist". The
opposite is true.

## Evidence

The targets exist and are real:

```
$ grep -rn '\[\[bench\]\]' crates/ Cargo.toml
crates/pn-gateway/Cargo.toml:32:[[bench]]
crates/pn-next/Cargo.toml:73:[[bench]]
# both: name = "benchmarks", path = "benches/benchmarks.rs", harness = false
# both files use criterion_group!/criterion_main!/bench_function
```

The targets do not compile. The two targets have 26 errors: 8× E0422, 6× E0560, 6× E0599,
5× E0432, 1× E0433.

```
$ cargo bench --workspace --locked --no-run
error[E0432]: unresolved imports `pn_gateway::GuardrailsConfig`, `pn_gateway::GuardrailsEngine`, `pn_gateway::ResourceRequest`
error[E0422]: cannot find struct, variant or union type `BulkheadConfig` in crate `pn_gateway`
error[E0560]: struct `FailSafeConfig` has no field named `bulkhead`
error[E0599]: no method named `unwrap` found for struct `FailSafeManager` in the current scope
error: could not compile `pn-gateway` (bench "benchmarks") due to 12 previous errors
error: could not compile `pn-next` (bench "benchmarks") due to 14 previous errors
```

Yet every `Benchmark` run on record is green. The audit window shows 15/15.

## Root cause

Two independent defects exist. Either defect alone can hide the other.

1. **A pipe without `pipefail`** in the benchmark step discarded the exit status.
2. **The bench sources drifted from the crate APIs.** Constructors that used to return
   `Result` now return the value: `FailSafeManager`, `HealthMonitor` and `MetricsCollector`.
   Config types were renamed or moved: `BulkheadConfig`, `RetryPolicyConfig`, `GuardrailsConfig`,
   `GuardrailsEngine` and `ResourceRequest`. Struct fields changed: `FailSafeConfig.bulkhead` and
   `.retry_policy`, plus `ContainerConfig.config` and `.sandbox_id`. Several imports no longer
   resolve: `pn_next::{Resources, RuntimeManager, SecurityProfile}` and
   `proot_compat::path_translation::*`.

The newly added guard step made this worse. It greps for the `[[bench]]` marker and passes.
That pass is a green tick over a target that cannot build.

## Impact

- The benchmark suite has not run or received maintenance for as long as the API has drifted.
- A green `Benchmark` check carried no information. It did not cover performance regressions.
- A reader who trusts it concludes that the benches are healthy.

## Fix

- Add `set -euo pipefail` to every benchmark step. A piped failure then fails the job.
- The guard now **builds** the targets with `cargo bench --workspace --locked --no-run`.
  It no longer only greps for the marker.
- The change repairs the 26 compile errors in `crates/pn-gateway/benches/benchmarks.rs` and
  `crates/pn-next/benches/benchmarks.rs`. Each reference now matches the current API. Every
  benchmark is preserved. No benchmark was deleted to make the file compile.

Labels: bug, ci, performance
