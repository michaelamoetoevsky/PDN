## Summary

`docs/src/configuration.md` documents a `[gateway.fail_safe]` section with bulkhead concurrency limiting and a retry policy. **None of it exists in the code.** `pn-gateway` defines a different set of fields. It has no `deny_unknown_fields`, and it does not depend on `toml`. The documented section therefore cannot take effect.

A user who follows the documentation configures a concurrency limit. The limit is never applied, and nothing tells the user.

## Evidence

The documentation shows this example at `docs/src/configuration.md:70-85`:

```toml
[gateway]
# Fail-safe configuration
[gateway.fail_safe]
circuit_breaker_threshold = 5
circuit_breaker_timeout = "60s"
retry_max_attempts = 3
retry_base_delay = "1s"
retry_max_delay = "30s"
bulkhead_max_concurrent = 10
bulkhead_queue_size = 100
```

The real struct is at `crates/pn-gateway/src/config.rs:190-197`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailSafeConfig {
    pub enabled: bool,
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub fallback_actions: Vec<FailSafeAction>,
    pub circuit_breaker: CircuitBreakerConfig,
    pub auto_recovery: bool,
}
```

Key-to-field intersection:

```
kunci di docs/src/configuration.md : ['bulkhead_max_concurrent', 'circuit_breaker_threshold', 'retry_max_attempts']
field aktual FailSafeConfig        : ['auto_recovery', 'circuit_breaker', 'enabled', 'fallback_actions', 'max_retries', 'retry_delay']
irisan (yang benar-benar terbaca)  : KOSONG
diabaikan diam-diam                : ['bulkhead_max_concurrent', 'circuit_breaker_threshold', 'retry_max_attempts']
```

`grep -c deny_unknown_fields crates/pn-gateway/src/config.rs` → **0**. Serde ignores unknown fields by default. The documented example therefore produces no error, even if the parser reads it.

`grep -rn "toml::" crates/pn-gateway/src/` → **nothing**. `toml` is not a dependency of that crate. See `crates/pn-gateway/Cargo.toml`. The crate that documents this config surface does not read TOML at all. `FailSafeConfig` is built programmatically by the builder at `config.rs:600`.

## The related drift this uncovered

Four capabilities that the documentation and the benchmark suite refer to are absent from `crates/pn-gateway/src/`:

| Symbol / key | In docs | In `src/` |
|---|---|---|
| `bulkhead`, `BulkheadConfig`, `bulkhead_max_concurrent`, `bulkhead_queue_size` | `architecture.md:138,179`, `configuration.md:78-79` | 0 hits (`grep -rni bulkhead crates/pn-gateway/src/` is empty) |
| `retry_policy: RetryPolicyConfig`, `backoff_multiplier`, `max_delay`, `retryable_errors` | `architecture.md` (`pub retry_policy: RetryPolicyConfig,`) | 0 hits each |
| `ResourceRequest` (per-request guardrail evaluation) | — | 0 hits anywhere in the repo |
| `HealthCheckType::HTTP` | — | variant removed; the enum now has `Process`, `MemoryUsage`, `CpuUsage`, … |

`docs/src/architecture.md:138` still states: *"**FailSafeManager**: Circuit breakers, retry policies, bulkheads"*.

`grep -rn "bulkhead_max_concurrent" --include=*.rs .` → **no hits**. No test ever exercised the documented example, which is why the drift went unnoticed.

## Why it went unnoticed for so long

`crates/pn-gateway/benches/benchmarks.rs` was the **last code artifact that still referred to the old API**. It called `BulkheadConfig`, `RetryPolicyConfig`, `GuardrailsEngine::new(..).unwrap()`, `health.register_check(..)`, and `metrics.increment_counter(name, &[..])`. It stopped compiling with 26 errors. That failure was invisible because the `Benchmark` workflow piped `cargo bench` through `tee` without `pipefail`. The check was green for a run that failed. That is fixed in #25 and `e2d2b0a`.

The benchmark suite therefore acted as an accidental canary for API drift. It broke first, and its brokenness was hidden. Nothing else in the repo asserts that documented configuration keys match real fields.

## Impact

- **A safety control documented as available is absent.** Concurrency limiting, or bulkhead, is the type of control someone relies on to protect a host. A user who sets `bulkhead_max_concurrent = 10` gets no limit and no warning.
- Retry behaviour differs from the documentation. The implementation has a single fixed `retry_delay` with `max_retries`, and it retries on any error. It has no backoff, no `max_delay`, no `retryable_errors`, and no `retry_base_delay`.
- Documentation cannot be trusted as a description of the gateway's fail-safe surface.

## Suggested resolution

The honest options, in the order I will try them:

1. **Decide the intent first.** Either these features are planned but unimplemented, or the code removed them and the docs never followed. The answer determines everything else, and it is a maintainer call rather than something to infer from the code.
2. **If unimplemented:** move the `[gateway.fail_safe]` section out of `configuration.md` into a clearly-labelled "Planned" section, and correct `architecture.md:138`. Documentation that describes absent safety features is worse than no documentation.
3. **If intended to exist:** implement `bulkhead` and `retry_policy` with backoff on `FailSafeConfig`, or change the docs to match the real field names, `max_retries` and `retry_delay`.
4. **Regardless of the above, make the surface verifiable.** Two cheap guards:
   - Give the gateway config structs `#[serde(deny_unknown_fields)]`. Then a documented-but-absent key fails loudly. It is not ignored.
   - Add a test that parses every configuration example in `docs/src/` against the real structs, so the docs cannot drift away from the code again. That test is the guard that was absent.

## Certainty boundary

Verified directly: the field and key mismatch, the absence of `deny_unknown_fields`, the absence of a `toml` dependency, and the zero-hit greps above. **Not** verified: an end-to-end run of `podnext` or the gateway against the documented `config.toml`. The loader lives outside `pn-gateway`. The gateway has no TOML path of its own, so the section is inert by construction, not by a loader decision I observed.

Labels: bug, documentation
