# PDN Phase 1–4 Evidence Record

**Repository**: https://github.com/michaelamoetoevsky/PDN
**Workspace version**: `0.1.7`
**Branch**: `master`
**Evidence date**: `2026-11-01`

This document records verified repository state. A CLI entry, Rust compilation, or unit test does not by itself prove the corresponding runtime feature.

## Evidence rules

- **Runtime-proven**: exercised locally and output captured.
- **Test-proven**: test target executed; scope stated.
- **Compile-proven**: compiled or help/metadata inspected; runtime behavior not established.
- **Declared/stub**: source exposes the interface, but implementation returns a placeholder or is not exercised.
- **Unverified**: no current evidence.

## Baseline evidence

| Check | Result | Scope |
|---|---:|---|
| `cargo build --package podnext --release` | PASS, exit `0` | Host release build |
| `cargo test --workspace` | `93 passed`, `1 ignored` | Workspace tests |
| `cargo test --package pn-next --features cri` | `49 passed`, `0 failed`; doctest PASS | `pn-next` with `cri` |
| `cargo fmt --all -- --check` | PASS, exit `0` | All Rust targets |
| `git diff --check` | PASS, exit `0` | Current worktree |
| `target/release/podnext --version` | `podnext 0.1.7` | Local release binary |
| `target/release/podnext-daemon --version` | `podnext-daemon 0.1.7` | Local release binary |
| Daemon IPC smoke | PASS | Unix socket: `ping` → `{"type":"pong"}`; `list` → `{"type":"ok","data":[]}` |
| Active build/runtime processes after verification | None found | `cargo`, `rustc`, `podnext`, `podnext-daemon` |

Remote GitHub release status is deliberately separate: API verification found releases through `v0.1.5`; `v0.1.7` is not established as a remote release. The local tag `v0.1.7` and local binaries do not prove publication.

## Phase 1 — Core stabilization

### Verified

- Workspace metadata is centralized in root `Cargo.toml` with version `0.1.7`.
- `podnext` and `podnext-daemon` use `#[command(version)]`; both local release binaries report `0.1.7`.
- Repository metadata points to `https://github.com/michaelamoetoevsky/PDN`.
- Debug output changed to opt-in through `PN_NEXT_DEBUG`; release CLI help scan found no `[DEBUG main]` leakage.
- Release workflow contains Android targets `aarch64-linux-android` and `x86_64-linux-android`.
- The daemon creates a Unix socket and answered `ping` and `list` during the recorded foreground smoke test.
- Workspace and CRI-scoped test commands passed as listed in the baseline table.

### Not established by current evidence

- Interactive container login, direct `-r` execution, image installation, and cross-compiled Android binaries were not re-exercised in this verification pass.
- GitHub release `v0.1.7` and its assets are not proven to exist remotely.
- The release workflow itself was not executed in this local pass.

## Phase 2 — Feature surface and implementation depth

| Area | Evidence status | Exact finding |
|---|---|---|
| Direct execution flags | Compile-proven | CLI exposes rootfs, bind, pwd, qemu, UID/GID, environment, tracing, and related flags; end-to-end behavior not re-exercised here. |
| `ps` session detail | Test-proven only | Workspace tests pass; no dedicated runtime command output recorded in this pass. |
| Image commands | Mixed | `ls`, `pull`, and `rm` have handlers; `push`, `tag`, `history`, and `inspect` are explicitly stub/not-implemented paths in source. |
| Network commands | Declared/stub | CLI/help surface exists; handlers are stub implementations; multi-container networking is unverified. |
| Volume commands | Declared/stub | CLI/help surface exists; handlers are stub implementations; persistent storage workflow is unverified. |
| Podman-compatible run flags | Unverified | No evidence sufficient to claim parity. |
| Daemon service control | Compile/runtime partial | Daemon IPC `ping`/`list` passed; service lifecycle commands were not established by a runtime evidence set. |

Phase 2 is therefore **partially implemented**, not feature-complete.

## Phase 3 — Platform maturity

| Area | Evidence status | Exact finding |
|---|---|---|
| CRI plugin | Test-proven, runtime unverified | `pn-next` CRI feature: `49` unit tests passed, `0` failed, doctest passed. Kubelet-compatible gRPC runtime was not demonstrated. |
| Web dashboard | Compile-proven/unverified runtime | Module exists; live HTTP/WebSocket dashboard behavior was not demonstrated. |
| Supervisor | Test/compile scope only | Supervisor code is present; daemon startup plus basic IPC passed; service restart/health/dependency behavior was not fully exercised. |
| Multi-node clustering | Unverified | No current implementation/runtime evidence establishing Raft or multi-node operation. |
| GitOps sync | Unverified | No current implementation/runtime evidence establishing reconciliation from Git. |

Phase 3 is **not production-ready on the available evidence**.

## Phase 4 — Ecosystem and adoption

| Area | Evidence status | Exact finding |
|---|---|---|
| crates.io publication | Unverified | No publication evidence collected. |
| termux-packages acceptance | Unverified | Recipe exists; acceptance/repository availability is not proven. Recipe version is aligned to `0.1.7`. |
| VS Code DevContainer | Unverified | No current end-to-end validation recorded. |
| Community metrics | Unverified | No current GitHub metric evidence recorded. |
| Remote release `v0.1.7` | Not proven | GitHub API evidence stops at `v0.1.5`; local tag is not equivalent to a release. |

## Open work

1. Classify and remove or implement remaining `TODO`/stub paths before claiming feature completion.
2. Add real daemon IPC integration coverage instead of treating the existing client example as a test; that example currently runs `0 tests`.
3. Register or separately invoke `tests/integration/v2_criteria.rs`; its execution by Cargo is not established.
4. Exercise Android cross-build and artifact verification in CI.
5. Publish and independently verify `v0.1.7` only after the intended commit and assets are available remotely.

## Reproduction commands

```sh
cd ~/workspace/PDN
env -u LD_LIBRARY_PATH cargo fmt --all -- --check
git diff --check
env -u LD_LIBRARY_PATH cargo build --package podnext --release
env -u LD_LIBRARY_PATH cargo test --workspace
env -u LD_LIBRARY_PATH cargo test --package pn-next --features cri
./target/release/podnext --version
./target/release/podnext-daemon --version
```

Evidence logs remain under `$HOME/.hermes/cache/scratch/`, including `pdn-format-fix-20261101-185220.log`, the workspace/CRI test logs, and the daemon IPC smoke log.
