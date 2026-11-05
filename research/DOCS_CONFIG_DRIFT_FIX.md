# #27 — Gateway config silently ignores unknown keys (fix + reconciliation)

**Issue:** [#27] `[BUG-DOC1] Documented gateway fail-safe config (bulkhead, retry_policy) does not exist in code; keys are silently ignored`
**Department:** Configuration & Documentation Engineering (CDE)
**Branch:** `dept/config-drift` (worktree of master `5df9484`)
**Scope:** enforcement fix + regression test + reconciliation of the `[gateway]` documentation surface. The full 227-key drift is listed as a prioritised backlog, not silently half-edited.

---

## TL;DR

1. **Code fix:** `#[serde(deny_unknown_fields)]` added to every `Deserialize` struct in
   `crates/pn-gateway/src/config.rs` (18 structs). An unknown key — e.g. the documented
   `bulkhead` / `retry_policy` — now fails at deserialization instead of being dropped.
2. **Docs reconciled:** the `[gateway.fail_safe]` example and `architecture.md`'s `FailSafeConfig`
   pseudo-struct now mirror the real struct; the unimplemented keys
   (`bulkhead_*`, `retry_base_delay`, `retry_max_delay`) are marked **planned**, not presented as
   working. No feature was invented.
3. **Regression test:** `config::tests::documented_fail_safe_keys_are_real_or_declared_planned`
   reads `docs/src/configuration.md` and fails if a future doc key is neither a real field nor
   explicitly declared planned. A second test proves unknown keys are rejected.
4. **`cargo test -p pn-gateway --locked` is green (5 passed).**

---

## 1. What changed and why

### 1.1 Enforcement — `#[serde(deny_unknown_fields)]`

**Pre-check (before applying):** the attribute is incompatible with `#[serde(flatten)]`. The crate
has none:

```
$ grep -rn "flatten" crates/pn-gateway/src/ || echo "NO flatten"
NO flatten
$ grep -rn "deny_unknown_fields" crates/pn-gateway/src/ || echo "NO deny_unknown_fields"
NO deny_unknown_fields
```

So `deny_unknown_fields` is safe here. Applied to the 18 structs that derive `Deserialize`
(the unit-variant enums were left untouched — the attribute is a struct/field-deserialization
control and has no meaning for externally-tagged unit enums):

`GatewayConfig, ProotConfig, BindMount, DistroConfig, DistroSource, CompatConfig, FailSafeConfig,
CircuitBreakerConfig, GuardrailConfig, GuardrailRule, HealthConfig, HealthCheck, MetricsConfig,
NetworkConfig, PortMapping, StorageConfig, LoggingConfig, FileRotationConfig`.

```
$ grep -c "deny_unknown_fields" crates/pn-gateway/src/config.rs
18
```

No code path deserialized these structs before this change (`grep -rn "from_str\|from_value\|serde_json::from" crates/pn-gateway/` → 0 hits), so the attribute cannot break existing behaviour:
the only construction path is `Default`/builder.

### 1.2 Documentation reconciliation — the `[gateway.fail_safe]` block

`docs/src/configuration.md` previously documented seven keys that do not exist:

```toml
[gateway.fail_safe]
circuit_breaker_threshold = 5
circuit_breaker_timeout = "60s"
retry_max_attempts = 3
retry_base_delay = "1s"
retry_max_delay = "30s"
bulkhead_max_concurrent = 10
bulkhead_queue_size = 100
```

It was replaced with the **real** field names/shape plus an explicit `# planned:` sentinel for the
keys that are genuinely absent, and a note that the global `config.toml` has no loader yet:

```toml
[gateway]
# NOTE: the global config.toml shown in this section is not yet read by any crate
# (issue #27). The field names below mirror `crates/pn-gateway/src/config.rs` so
# they are correct for the day a loader exists.
#
# Fail-safe configuration
[gateway.fail_safe]
enabled = true
max_retries = 3
retry_delay = "5s"
fallback_actions = ["Retry", "FallbackToHost", "RestartContainer"]
auto_recovery = true
circuit_breaker = { enabled = true, failure_threshold = 5, timeout = "60s", half_open_requests = 3 }
# planned: bulkhead_max_concurrent, bulkhead_queue_size, retry_base_delay, retry_max_delay
```

`docs/src/architecture.md` likewise carried a fake struct
(`FailSafeConfig { retry_policy, bulkhead, timeout }`) and the line
*"FailSafeManager: Circuit breakers, retry policies, bulkheads"*; both were corrected to the real
shape with a `#27` note.

### 1.3 Reconciliation table (documented key → present in code? → action taken)

| Documented key | Present in code? | Action taken |
|---|---|---|
| `gateway.fail_safe.circuit_breaker_threshold` | **No** — renamed; real path is `circuit_breaker.failure_threshold` | **Doc fixed** → nested `circuit_breaker = { failure_threshold = 5 }` |
| `gateway.fail_safe.circuit_breaker_timeout` | **No** — renamed; real path is `circuit_breaker.timeout` | **Doc fixed** → nested `timeout = "60s"` |
| `gateway.fail_safe.retry_max_attempts` | **No** — renamed; real field is `max_retries` | **Doc fixed** → `max_retries = 3` |
| `gateway.fail_safe.retry_base_delay` | **No** (no backoff base field) | **Planned** (`# planned:`) — tracked, not invented |
| `gateway.fail_safe.retry_max_delay` | **No** | **Planned** — tracked, not invented |
| `gateway.fail_safe.bulkhead_max_concurrent` | **No** (0 hits for `bulkhead` in `crates/pn-gateway/src/`) | **Planned** — the headline of #27 |
| `gateway.fail_safe.bulkhead_queue_size` | **No** | **Planned** |
| `architecture.md` `retry_policy: RetryPolicyConfig` | **No** | **Doc fixed** — removed from the pseudo-struct |
| `architecture.md` `bulkhead: BulkheadConfig` | **No** | **Doc fixed** + planned note |
| `architecture.md` `timeout: TimeoutConfig` | **No** | **Doc fixed** — removed |
| `architecture.md` "retry policies, bulkheads" prose | **No** | **Doc fixed** — "fixed retries (no backoff/bulkhead yet — see #27)" |

**Decision rule applied.** Where a real field exists under a different name (3 keys), the *code is
right* and the markdown was fixed. Where no field exists anywhere (`bulkhead_*`, retry backoff),
the docs describe an **intended but unimplemented** feature → marked planned and tracked; no feature
was invented.

---

## 2. Regression tests

`crates/pn-gateway/src/config.rs` — `#[cfg(test)] mod tests`:

* `unknown_fields_are_rejected` — rounds-trips `FailSafeConfig`, then injects an unknown key
  (`bulkhead`) and asserts deserialization **errors** with the offending key named.
* `documented_fail_safe_keys_are_real_or_declared_planned` — reads `docs/src/configuration.md`,
  derives the real field set from `FailSafeConfig` (serialised to JSON — not hand-copied), and
  asserts every active `[gateway.fail_safe]` key is either a real field or listed in the
  `# planned:` sentinel, **and** that no planned key has silently become real.

### 2.1 Negative proof — the drift guard bites

Inserting a bogus active key (`drift_probe_unknown_key = 1`) into the documented block:

```
$ cargo test -p pn-gateway --locked
...
test config::tests::documented_fail_safe_keys_are_real_or_declared_planned ... FAILED
---- config::tests::documented_fail_safe_keys_are_real_or_declared_planned stdout ----
thread 'config::tests::documented_fail_safe_keys_are_real_or_declared_planned' (11802) panicked at crates/pn-gateway/src/config.rs:708:13:
documented key `drift_probe_unknown_key` is not a real FailSafeConfig field (issue #27): implement it or move it to the `# planned:` list
...
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

(exit 101; the probe was then reverted — `git diff` shows only the intended changes.)

### 2.2 Green run (final state)

```
$ flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
    'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
     heavy --budget-mb 2200 --min-avail-mb 700 -- \
     env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked'
EXIT=0
```

Heavy status (`20261104T155810Z-env-u-LD.log.status`):

```
outcome=completed
exit_code=0
reason=exited normally
peak_rss_mb=233
min_avail_seen_mb=3019
duration_s=7
cmd=env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked
```

Test output:

```
     Running unittests src/lib.rs (/data/data/com.termux/files/home/workspace/PDN/target/debug/deps/pn_gateway-652469170f01c3bd)

running 5 tests
test config::tests::unknown_fields_are_rejected ... ok
test config::tests::documented_fail_safe_keys_are_real_or_declared_planned ... ok
test tests::test_gateway_creation ... ok
test health::tests::test_health_monitor_creation ... ok
test gateway::tests::test_gateway_creation ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests pn_gateway
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo fmt -p pn-gateway -- --check` → exit 0.

---

## 3. Residual gaps (honest)

* **The global `config.toml` still has no loader.** `pn-gateway` has no `toml` dependency and no
  crate opens a global config file. Even with the corrected field names the documented global
  example is **inert by construction** — nothing reads it. Fixing that (writing a loader or
  deleting the example) is a maintainer decision, not something this increment should invent.
* **The other ~220 drifted keys are untouched.** The audit
  (`research/DOCS_CONFIG_DRIFT_AUDIT.md`) lists 261 documented keys, 227 drifted. This change only
  reconciles the `[gateway]` section that #27 names. The remaining drift spans `[container.*]`,
  `[profile.*]`, `[network.*]`, `[grpc.*]`, `[api.*]`, `[general.*]`, `[plugin.*]`, `[pod.*]` and is
  carried as the backlog below.
* **Backlog (prioritised by blast radius, per audit section 5.6):**
  1. `gateway.guardrails.*` limits (`max_containers`, `max_memory_total`, `max_cpu_total`,
     `blocked_images/syscalls`, `required_security_profile`, `quotas.*`, `rate_limit.*`) — safety
     surface, all absent.
  2. `gateway.health.timeout` / `healthy_threshold`, `gateway.metrics.port` / `path` — absent.
  3. `profile.seccomp.rules.*` — documented as a map, real type is `Vec`; plus wrong enum literals
     (`SCMP_ARCH_AARCH64` vs `AARCH64`).
  4. The persisted surfaces that *are* loaded (`networks.toml`, `volumes.toml`, supervisor service
     TOMLs) should get `deny_unknown_fields` too — those are the only ones where a wrong key
     currently has real effect. Out of CDE's file scope here.
* **Not verified:** an end-to-end `podnext` run against a documented `config.toml` — there is no
  loader to exercise, so the inertness is structural, not observed at runtime.

---

## 4. Exact reproduction

```sh
cd /data/data/com.termux/files/home/workspace/PDN
git worktree add --detach /data/data/com.termux/files/home/.hermes/cache/scratch/wt-dept-cde 5df9484
cd /data/data/com.termux/files/home/.hermes/cache/scratch/wt-dept-cde
git switch -c dept/config-drift

# proof of enforcement + drift guard (via the mandated memory guard + flock):
export PATH="$HOME/.hermes/bin:$PATH"
flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
  'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
   heavy --budget-mb 2200 --min-avail-mb 700 -- \
   env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked'
```

Raw transcripts saved to:

* `$HOME/.hermes/cache/scratch/cde-03c-test.log` (first green run after `touch`)
* `$HOME/.hermes/cache/scratch/cde-03d-NEGATIVE.log` + `cde-03d-heavy.log` (drift-guard failure)
* `$HOME/.hermes/cache/scratch/cde-04-final.log` + `cde-04-heavy.log` (final green run)
* `$HOME/.hermes/logs/heavy/20261104T155810Z-env-u-LD.log{,.status}` (authoritative heavy log)
