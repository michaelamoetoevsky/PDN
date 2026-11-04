# Gateway configuration TOML loader

Department: Platform Configuration Engineering (PCE)
Branch: `dept/gateway-loader` (base `72ef9d6`)
Owner files: `crates/pn-gateway/src/**`, `crates/pn-gateway/Cargo.toml`,
`crates/pn-gateway/tests/config_loader*.rs`.

## 1. The gap (proof, before any change)

`crates/pn-gateway/src/config.rs` carries `#[serde(deny_unknown_fields)]` on its
structs and the documented key names were reconciled (issue #27), but nothing
reads a config file. The corrected key names are inert: they exist only as a
struct shape, never as a loaded document.

```
$ grep -rn "toml" crates/pn-gateway/Cargo.toml
(no output, exit 1)

$ grep -rn "from_str\|read_to_string" crates/pn-gateway/src/config.rs
674:        serde_json::from_str::<FailSafeConfig>(&ok).expect("valid FailSafeConfig must parse");
677:        let mut v: serde_json::Value = serde_json::from_str(&ok).unwrap();
679:        let err = serde_json::from_str::<FailSafeConfig>(&v.to_string())
693:        let doc = std::fs::read_to_string(
```

Every hit is inside `#[cfg(test)] mod tests` and every one is `serde_json` or a
test-only `read_to_string` of the *docs*. No production code path turns bytes on
disk into a `GatewayConfig`.

```
$ grep -rn "GatewayConfig\|env::var\|std::env\|config.toml" crates/pn-gateway/src/gateway.rs
10:    config: crate::config::GatewayConfig,
22:    pub async fn new(config: crate::config::GatewayConfig) -> anyhow::Result<Self> {
174:        let config = crate::config::GatewayConfig::default();
```

`PnGateway::new` receives an already-built `GatewayConfig`. No caller, and no
binary in this crate, ever loads one from disk. The crate has no `[[bin]]`; it is
a library only.

## 2. Design choice and reason

Two entry points on `GatewayConfig` (in the new `src/config_loader.rs`):

- `GatewayConfig::from_toml_str(&str) -> Result<Self, ConfigError>`
- `GatewayConfig::from_toml_file(path) -> Result<Self, ConfigError>`

**The document is expected to carry a top-level `[gateway]` table.** This matches
the file shape in `docs/src/configuration.md` (`[gateway.fail_safe]`,
`[gateway.guardrails]`, ...). The internal wrapper `GatewaySection` is
deliberately **not** `deny_unknown_fields`: the same documented global file also
holds `[general]`, `[grpc]`, `[api]`, `[tracing]`, `[plugin]`, which the gateway
does not own, so a stricter top level would refuse a valid file. Strictness is
kept where issue #27 put it — *inside* `[gateway]`, where every struct is
`deny_unknown_fields`.

There is no implicit search path (`~/.config/pn-next/...`): the caller passes the
path explicitly, so a wrong location fails as `NotFound` rather than silently
loading nothing.

Error type (single `ConfigError`, `thiserror`):

| Variant | Meaning | Message |
|---|---|---|
| `NotFound(PathBuf)` | file absent | `gateway configuration file not found: <path>` |
| `Read { path, source }` | unreadable (perms/IO) | `failed to read gateway configuration file <path>: <source>` |
| `Parse(toml::de::Error)` | malformed TOML **or** schema violation | `cannot parse gateway configuration: <toml error>` |

`NotFound` and `Parse` are distinct variants, so "missing" and "malformed" are
distinguishable both by `match` and by message. `deny_unknown_fields` errors are
**surfaced, not swallowed**: `Parse` wraps the raw `toml::de::Error`, which names
the offending key and its line/column.

Dependency: `toml = "0.8"` added to `crates/pn-gateway/Cargo.toml` (0.8 already
resolved in `Cargo.lock`); `tempfile` added as a dev-dependency for the file
tests (already a workspace dependency).

## 3. Raw evidence

```
$ env -u LD_LIBRARY_PATH cargo metadata --format-version 1   # lock refreshed
$ git diff --stat Cargo.lock
 Cargo.lock | 2 ++
```

Test run (shared `CARGO_TARGET_DIR`, serialised via the cargo mutex, under
`heavy`; `--locked`):

```
$ flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
    'CARGO_TARGET_DIR=.../PDN/target heavy --budget-mb 2000 --min-avail-mb 700 \
     --label gateway-loader -- env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked'

     Running tests/config_loader.rs
running 5 tests
test missing_file_differs_from_malformed_file ... ok
test documented_fail_safe_keys_populate_the_struct ... ok
test unknown_key_is_rejected_and_named ... ok
test loads_from_an_explicit_path ... ok
test parses_realistic_config_into_struct ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
# heavy status: outcome=completed exit_code=0 peak_rss_mb=905 duration_s=35
```

Raw error strings (one-off `--nocapture` probe, removed before commit):

```
PROBE unknown-key => cannot parse gateway configuration: TOML parse error at line 38, column 1
   |
38 | bulkhead = 10
   | ^^^^^^^^
unknown field `bulkhead`, expected one of `enabled`, `max_retries`, `retry_delay`, `fallback_actions`, `circuit_breaker`, `auto_recovery`

PROBE missing     => gateway configuration file not found: /nonexistent/x.toml
PROBE malformed   => cannot parse gateway configuration: TOML parse error at line 1, column 6
  |
1 | this is = = not toml
  |      ^
expected `.`, `=`

PROBE max_retries=7 failure_threshold=9 cb_timeout=90s
```

The unknown key `bulkhead` is named, with line/column and the list of expected
keys. Missing and malformed produce different variants and different messages.
The documented fail-safe fields populate the struct:
`max_retries = 7`, `circuit_breaker.failure_threshold = 9`,
`circuit_breaker.timeout = 90s`.

## 4. What a maintainer must decide (NOT done here)

1. **Startup wiring.** There is no obvious load site to wire into:
   `PnGateway::new(config)` takes an already-built config, the crate has no
   binary, and inventing a `main`/env lookup would be a search-path decision the
   task forbids. The loader is therefore **unused but fully tested**. A
   maintainer must decide *who* calls `from_toml_file` (a new `pn-gateway` bin?
   `podnext`/`pn-next` bootstrap? `--config` CLI flag?) and what path policy
   applies. Recommendation: a `--config <path>` flag on the eventual binary,
   passing straight into `from_toml_file`.
2. **Duration encoding drift.** `docs/src/configuration.md` writes durations as
   strings (`retry_delay = "5s"`, `timeout = "60s"`), but the structs use
   `std::time::Duration`, whose serde encoding is a table (`{ secs = 5, nanos = 0 }`).
   The loader therefore parses the table form, and the *documented* string form
   would not load as written. This is a docs-vs-code mismatch for Documentation
   Engineering + a decision: either add a `#[serde(with = "...")]` duration-string
   deserializer on every `Duration` field, or change the docs to the table form.
   Docs are outside this department's file ownership, so nothing was changed there.
3. **Env-var entry point.** The task allowed "explicit path argument/env var".
   Only the explicit path argument was implemented (no env-var name is defined
   anywhere in the tree, and inventing one risks colliding with
   `PN_CONFIG_DIR`). An env-var convenience can be added once a name is agreed.

### Resolved (follow-up commit)

The duration question was decided in favour of the documentation: `crates/pn-gateway/src/duration_fmt.rs`
parses `"30s"` / `"500ms"` / `"2m"` / `"1h"` / `"1d"`, still accepts bare integer seconds and the
native `{ secs, nanos }` table, and rejects an unknown unit with the offending value named.
`GatewayConfig::to_toml_str` writes the same form back, so `docs/src/configuration.md` values now
load, round-trip, and are asserted by four integration tests in `crates/pn-gateway/tests/config_loader.rs`.

## 5. Residual gaps

- No serializer (`to_toml_str`); the "round-trip" covered is file→struct. Adding
  serialization would require exposing the `[gateway]` wrapper publicly.
- Top-level keys outside `[gateway]` are not validated (by design, see §2).
- `Cargo.lock` changed (pn-gateway added to `toml` and `tempfile` edges). The
  integrator must commit it.

## 6. Reproduction

```sh
cd crates/pn-gateway
env -u LD_LIBRARY_PATH cargo test -p pn-gateway            # from the workspace root
# or, under the device memory guard:
export PATH="$HOME/.hermes/bin:$PATH"
flock $HOME/.hermes/cache/scratch/cargo-mutex.lock -c \
  'CARGO_TARGET_DIR=/data/data/com.termux/files/home/workspace/PDN/target \
   heavy --budget-mb 2000 --min-avail-mb 700 --label gw \
   -- env -u LD_LIBRARY_PATH cargo test -p pn-gateway --locked'
```
