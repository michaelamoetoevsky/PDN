# Per-profile delegation on this host: measured state and the three ways forward

Condition 2 of the working brief requires delegated work to run under a *different* profile per role.
This note records what was measured on 8 Nov 2026, why the obvious route does not work here, and the
three paths that do — so the next session does not repeat the investigation.

## What was measured

| attempt | result |
|---|---|
| `hermes -p verifier chat -q "Reply with exactly: PROFILE_VERIFIER_READY"` | fails after ~66 s: *"Hermes couldn't start the model connection: Gateway starting: deadline."* |
| `hermes -p verifier -z "…"` (one-shot) | identical failure — so this is not a `chat`-only path |
| the same command under `env -i` with a clean environment | identical failure — so it is **not** leaked `HERMES_*` variables from the TUI |
| `hermes -p verifier doctor` | **green** — "API key or custom endpoint configured"; the profile's own config is fine |
| `curl http://localhost:20128/v1/models` | `HTTP 200` — the router the profile points at is alive |
| `hermes gateway list` | reports **every** profile as `not running` |
| `~/.hermes/gateway_state.json` | `served_profiles: [default, architect, builder, docs, release, researcher, security, triage, verifier]`, pid 5382, launched by profile `default` |
| `hermes gateway ensure` | `{"reason_code":"profile_mismatch","state":"inaccessible"}` |
| `hermes gateway run -p verifier` | prints *"already serves profile 'verifier' — nothing to start"* and exits 0 |
| `hermes api --help` | `'api' is not a hermes command. Did you mean: acp?` |
| `hermes -p verifier gateway run` (foreground, 25 s) | the host gateway already serves the profile; nothing starts |

Interpretation, stated as inference rather than fact: the host gateway is running and claims the profile,
but the CLI's client side cannot obtain a model connection for a *named* profile from it, and it never
falls back to an in-process run. Both sides therefore agree the profile is served, while the connection
that matters is never established. The header line `[MULTIPLEX] Re-scanned profile 'verifier' after
config/.env change (0 adapter(s) connected)` shows the multiplexer re-reads a profile after its config
changes, so config staleness is not the cause either.

What the official documentation says about this exact behaviour (`~/workspace/hermes-docs/multi-profile-gateways.txt`):

> `hermes -p <name> gateway run` while it is live attaches instead of starting a second process: it prints
> the host gateway's PID and served set and exits 0. If `<name>` is not served yet, it asks the host
> gateway to re-scan `profiles/` and attaches once the answer includes it; it refuses (non-zero) only when
> the host gateway cannot be made to serve it.
>
> `hermes gateway run --force` starts a separate gateway without asking the host process at all (the escape
> hatch when it is wedged or answering wrongly).
>
> `hermes gateway start --all` / `restart --all` mean the one host multiplexer.

Also relevant, on how a profile's secrets and references resolve
(`~/workspace/hermes-docs/configuration.txt`):

> Under a multiplexed multi-profile gateway, references in a profile's `config.yaml` resolve against that
> profile's `.env` (its secret scope), not the shared process environment.
>
> A local `hermes chat` / TUI session served by the gateway resolves these references, `provider_routing`
> and `prefill_messages_file` when the gateway creates the session, from its profile's current
> `config.yaml` and `.env`.

## Why it is not a configuration defect

The profile was repaired and re-tested in steps, each verified from a file rather than from intent:

1. `model.provider custom:9router` was rejected as `Unknown provider 'custom:9router'` — the lookup
   lower-cases the name and searches the profile's **own** config, which had no `custom_providers` block.
2. The 29 variables of the main profile's `.env` were copied into the profile's `.env` (mode `600`), with
   the 20128 key among them. The earlier copy attempt had silently missed it because the comparison used
   the pattern `[A-Z_]+`, which excludes digits, so `HERMES_CUSTOM_LOCALHOST_20128_API_KEY` matched nothing
   in either list.
3. `custom_providers` with the `9Router` entry (`base_url http://localhost:20128/v1`,
   `key_env HERMES_CUSTOM_LOCALHOST_20128_API_KEY`) was appended to the profile's `config.yaml`, and
   `model.provider` was set back to the named form `custom:9Router`.
4. `hermes -p verifier doctor` then reported the provider and key as configured, and the 9router endpoint
   answered `HTTP 200` — while `chat` still failed with the same deadline.

## The three ways to satisfy condition 2

| # | path | cost | risk |
|---|---|---|---|
| A | enable the **API server platform** on the gateway (`gateway.platforms.api_server`) and dispatch to profiles through its `/p/<profile>/` ingress, which the docs describe as requiring an explicit `target_profile` per request | config edit plus a gateway restart | low, but needs a restart of the shared gateway |
| B | `hermes gateway run --force` for the role profile, i.e. a second gateway process dedicated to that profile (the documented escape hatch for a host gateway that answers wrongly) | one extra process per role in use | medium — two gateways on one host, and the doc warns only `--force` skips the host lock |
| C | keep delegation in-process (`delegate_task`) and inject the target role's skill into each child, stating plainly that profile isolation is not achieved | none | none, but it does not literally satisfy condition 2 |

Path C is what the work has used so far, and it is labelled as such rather than passed off as profile
isolation. Paths A and B both require restarting or duplicating the gateway that carries the live TUI
session, which is why neither was done unilaterally: that is a side effect on the user's own runtime,
not a source change in the repository.

## What is NOT claimed

- It is not claimed that the gateway is broken in general: it serves the default profile, and the TUI
  session it carries is unaffected.
- It is not claimed that the profile config is wrong: `doctor` is green and the router answers.
- It is not claimed that any of A, B or C has been executed beyond C, which is running.

---

## Update, 8 Nov 2026 — the defect is patched; a port collision is what remains

The user chose to patch the Hermes defect and restart the host gateway. What was done, and what each step
proved:

| step | evidence |
|---|---|
| patch `gateway/control_socket.py::build_identify_payload` | backup `$PREFIX/tmp/hermes-control_socket.py.orig-20261108T073425`, sha256 `8404e61aa2ed4d86bf185ef68328f8321c44916cc90699199fa158ee48fb8354`; patched file compiles (`py_compile`), sha256 `74dbc7406b03707285811e35bd8fd37c6e6a2f738c2b16951c72c51955f09992` |
| the old payload reproduced the bug | raw control-socket query answered 9 entries, **all** `-> /data/data/com.termux/files/home/.hermes` |
| the new payload is correct | after restarting the gateway (pid 24557): `default -> ~/.hermes`, `architect -> ~/.hermes/profiles/architect`, … `verifier -> ~/.hermes/profiles/verifier` |
| positive control of the matching rule | with the real state file: the `verifier` client's home matches in the NEW list and **not** in the OLD one |

Replace-the-gateway mechanics, measured: `hermes gateway restart`, `hermes -p default gateway restart` and
`hermes gateway run -p <name>` all **attach** instead of restarting; `hermes gateway stop` answers
*"No gateway running for this profile"*; `sv status hermes-gateway` reports the service directory does not
exist. The old gateway (pid 5382) was idle — `platforms={}`, `active_agents=0` — so it was stopped with
`TERM` and a fresh `hermes gateway run --quiet` took its place. The live TUI session was unaffected, which
is itself evidence that this gateway was not carrying the interactive turn.

### The remaining blocker is a port collision, not configuration

`build_identify_payload` hardcodes `"api_origin": "http://127.0.0.1:9119"`, and the chat client dials
`api_origin` + `/api/ws` (`hermes_cli/gateway_client.py::gateway_ws_target`). On this host port 9119 belongs
to a **different** process:

```
4808  … from hermes_cli.main import main; sys.exit(main()) dashboard --host 0.0.0.0
6580  … -m tui_gateway.entry
24557 … gateway run --quiet        (the patched gateway)
```

Measured against that port:

| probe | answer |
|---|---|
| `GET http://127.0.0.1:9119/` | `302` |
| `GET http://127.0.0.1:9119/api/ws` | `401` |
| WS upgrade, no credential | `403 InvalidStatus` |
| WS upgrade with `?ticket=<minted by the gateway>` | `403 InvalidStatus` |
| WS upgrade with the ticket as subprotocol `hermes-gateway-ticket.<t>` | `403 InvalidStatus` |
| `HERMES_DASHBOARD_SESSION_TOKEN` in the environment | unset |

So the client reaches the **dashboard's** gate rather than the gateway's own endpoint and every credential
form is refused. Consequence: `hermes -p <name> chat` now fails in 7 s with
*"Gateway connection failed; no local fallback (InvalidStatus)"* instead of a 66 s deadline — faster and
further along, still not working — and the default profile fails the same way, which is why this is a host
port conflict rather than a profile defect.

### Ways to finish it

| # | action | risk |
|---|---|---|
| A | move the dashboard off 9119 (or stop it) so the gateway's own `/api/ws` on 9119 is reachable | the dashboard is a user service; `--host 0.0.0.0` currently binds all interfaces |
| B | point `api_origin` at the port the gateway/TUI actually serves `/api/ws` on (needs confirming which process serves it) | another host-file patch |
| C | leave profile delegation to in-process children with the role skill injected | none; does not literally satisfy condition 2 |

A and B touch the user's runtime rather than the repository, so neither is done without a decision.

