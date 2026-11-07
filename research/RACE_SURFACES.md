# Race surfaces in a shimming runtime — does PDN need a race-condition mechanism?

Question asked of this project: *given how PDN's components work with shimming, does it need
race-condition mechanisms?* Answered from code and from measurements, not from intuition. Every
claim below carries a `file:line`; anything not verified is labelled **UNVERIFIED** and is not
counted as a finding.

## 1. What "shimming" means in this tree

PDN mediates another component's view of the world at four boundaries. These are the places a race
can be *introduced* rather than merely hit:

| boundary | what it mediates | where |
|---|---|---|
| store + image ingest | a container's on-disk identity vs what a caller is told | `proot-distro-core/src/{container,blobstore,refcount,storage}.rs`, `podnext/src/native/{image,meta}.rs` |
| session / PTY | a detached guest's terminal vs a later reattach | `podnext/src/native/session.rs` (`session_socket_path`, readiness), `proot-distro-core/src/session.rs` |
| runtime backend | kernel features the guest expects vs what the executor can do | `pn-next/src/{supervisor,runtime,cgroup_view}.rs`, `readiness.rs` |
| socket-activation / CRI | Docker's and k8s's expectations vs PDN's container model | `pn-next/src/container_socket_activation/`, `cri_plugin/` |

## 2. The primitives that already exist (measured, with a control)

Counts are `grep -rE` over `crates/*/src`.

| primitive | occurrences | what it is for |
|---|---|---|
| `flock` / `FileLock` | 27 | cross-process exclusion: containers, staging sidecars |
| `Mutex` / `RwLock` | 314 | in-process async state (tokio) |
| `AtomicU` / `AtomicBool` / `Ordering::` | 21 | small in-process flags and counters |
| `ESTALE` / `st_ino` | 10 | write-then-verify identity (B11's refusal) |
| `rename_if_unchanged` | 3 | publish only if the target did not move |
| `O_NOFOLLOW` | 9 | refuse to follow a link the guest could have planted |
| `sync_all` / `fsync` | 7 | durability at the write boundary |
| `try_lock` / `LOCK_NB` | 10 | bounded waits rather than deadlocks |

**A control, because the first measurement was wrong.** The initial run reported 0 for nearly every
pattern while `Arc<` returned 230 — the patterns used `-E` together with `\|`, and in ERE `\|` is a
literal pipe, not alternation. With the control that must find a symbol known to exist (`FileLock`,
present at `locking.rs:65`) the corrected run found 27. A count of zero is a *measurement failure*
until a positive control says otherwise; the same rule the gate skill states for empty output.

**Absent, and this is a design consequence, not a gap:** `compare_exchange` 0, `Condvar` 0,
`notify_one`/`notify_all` 0, `watch::channel` 0. All four are *in-process* coordination primitives.
PDN coordinates across **processes** through files, `flock` and sockets, and within a process
through 314 async locks. No requirement surfaced in the tree asks for the missing four.

## 3. The race class shimming actually introduces: TOCTOU on the mediated view

A shim checks something about the host, then acts — or lets the guest act — later. The window is
between check and use, and it is a *different* race from "two threads touching one variable": the
adversary is the mediated party (a guest process, a container, a registry) or the host changing
under the shim.

This campaign's fixes are recognisable as exactly this remedy, which is why the direction is right:

| fix | window it closed | mechanism |
|---|---|---|
| B11 | create-then-replace at a destination | refuse with `ESTALE` when `(st_dev, st_ino)` moved (`storage.rs`) |
| A17 | a refusal discarded at the call site (`let _ =`) | the mechanism now has a channel to the caller |
| C8 | a recycled pid read as a live session | liveness is the **inherited descriptor**; the pid is only an address |
| S3 | a FIFO at a metadata path blocking `list`/`remove` | one reader, `O_NONBLOCK|O_NOFOLLOW` + `fstat`, regular files only |
| S8 | a cgroup limit written down but not enforced | serve what can be honoured, refuse the rest by named errno |
| C10 | a start reported before the child was ready | one readiness byte, with `Timeout` and `Closed` as distinct errors |
| C6 | eviction racing a live staging tree | refusal while the sidecar `flock` is held (single-writer → refcount) |

## 4. Verdict on the question

**PDN does not need a general race-condition mechanism.** It needs three things, two of which are
defects and one of which is missing evidence. A "concurrency subsystem" (lock-free structures,
atomics rework, in-process signalling) has no consumer in this tree.

### 4a. The suspected torn read — CHECKED, WITHDRAWN

An earlier revision of this document reported a torn read at `container_socket_activation/mod.rs:455-456` and
`:524-525`, on the grounds that two guards are read in sequence. Reading the surrounding block settles it: **both
read guards are held across both lookups** -

```rust
let active_guard = active.read().await;
let config_guard = configs.read().await;
if let Some(state) = active_guard.get(&socket_name) {
    if let Some(config) = config_guard.get(&socket_name) {
```

- so the (state, config) pair is consistent, and `configs.remove()` at `:202` cannot interleave because a writer
cannot take the configs write lock while any reader holds a read lock. No defect. The inference was drawn from
two `get` calls without checking the guards' lifetime, which is the same class of mistake as reading a count out
of a broken pattern: the evidence for a claim must be the thing that would refute it.

### 4b. The inconsistent-looking write — CHECKED, not a finding

`native/image.rs:980` publishes metadata through a temporary file (`metadata.json.tmp-<uuid>`, i.e.
write-then-rename). A differently-shaped call appears at `native.rs:146`:
`std::fs::write(tmp.path().join("metadata.json"), …)`. Reading the surrounding function settles it:
that line is inside `#[cfg(test)] mod image_runtime_config_tests`, in the helper
`fn scratch_dir(metadata: &str)`, writing into a `tempfile::TempDir`. It is a **test fixture, not a
production write path**, so the only production metadata write in the tree is the atomic one.
Recorded here so the next reader does not re-litigate it from the two call sites looking different.

### 4c. Mechanisms that exist but have never been raced (missing evidence, not missing code)

Each of these is single-writer by construction, so no new mechanism is implied — but each is proven
only by sequential tests, and the campaign's own history (A12 "not reproduced", A6 "transient only
under load") shows what happens when concurrency evidence is absent:

| mechanism | why it should hold under concurrency | what is missing |
|---|---|---|
| `blobstore::put_reader` two-phase ingest | tmp name carries `pid` and a counter (`:161`), publication is `rename_if_unchanged` with an identity check (`:196`) | a test that ingests one digest from two processes at once |
| `refcount::sweep` vs a live install | both take the same sidecar `flock` (`:168-170`); a lock that can be taken proves its owner is gone | a test that sweeps while an install holds the sidecar |
| store `list` during `install` | metadata is read only from regular files (`S3`), staging trees are named `.tmp-install-*` (`:899`) | a test that lists under install load and asserts no staging tree is ever listed |
| session register vs attach | registration takes `LOCK_EX` (`session.rs:314`), liveness is the descriptor (`C8`) | a test that attaches while the record is being created |

## 5. What follows for the worklist

- **A28 (defect)**: close the torn read at `container_socket_activation/mod.rs:455-456` and
  `:524-525`; acceptance: a test that removes a config concurrently with a lookup and never observes
  a state without its config, with the base tree quoted failing.
- **A29 (evidence)**: make the four rows of 4c real tests. The value is not new code — it is turning
  "should hold" into "was raced and held", which is the only form of concurrency claim this project
  has accepted anywhere else.
- **A30 — CLOSED by the check above**: 4b was a test fixture, not a production write. No item.

Explicitly **not** proposed: a lock-free store, an in-process event bus, `Condvar`-based handshakes,
or replacing `flock` with advisory atomics. None has a consumer here, and the shimming boundaries
are process boundaries by construction.
