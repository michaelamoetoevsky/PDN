# Race surfaces in a shimming runtime. Does PDN need a race-condition mechanism?

Question asked of this project: does PDN need race-condition mechanisms, given how the PDN
components work with shimming? This document answers from code and from measurements, not from
intuition. Every claim below carries a `file:line`. Anything not verified is labelled
**UNVERIFIED**, and it is not counted as a finding.

## 1. What "shimming" means in this tree

PDN mediates the view of the world that another component has, at four boundaries. These are the
places where a race can be *introduced*, not merely hit:

| boundary | what it mediates | where |
|---|---|---|
| store + image ingest | a container's on-disk identity vs what a caller is told | `proot-distro-core/src/{container,blobstore,refcount,storage}.rs`, `podnext/src/native/{image,meta}.rs` |
| session / PTY | a detached guest's terminal vs a later reattach | `podnext/src/native/session.rs` (`session_socket_path`, readiness), `proot-distro-core/src/session.rs` |
| runtime backend | kernel features the guest expects vs what the executor can do | `pn-next/src/{supervisor,runtime,cgroup_view}.rs`, `readiness.rs` |
| socket-activation / CRI | Docker's and k8s's expectations vs PDN's container model | `pn-next/src/container_socket_activation/`, `cri_plugin/` |

## 2. The primitives that already exist, measured with a control

The counts come from `grep -rE` over `crates/*/src`.

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
pattern, but `Arc<` returned 230. The patterns used `-E` together with `\|`.
In ERE, `\|` is a literal pipe, not alternation. The control must find a symbol known to exist.
That symbol is `FileLock`, present at `locking.rs:65`. With that control, the corrected run found 27.
A count of zero is a *measurement failure* until a positive control says otherwise. The gate skill
states the same rule for empty output.

**Absent, and this is a design consequence, not a gap:** `compare_exchange` 0, `Condvar` 0,
`notify_one`/`notify_all` 0, `watch::channel` 0. All four are *in-process* coordination primitives.
PDN coordinates across **processes** through files, `flock` and sockets. PDN coordinates within one
process through 314 async locks. No requirement that surfaced in the tree asks for the missing four.

## 3. The race class shimming actually introduces: TOCTOU on the mediated view

A shim checks something about the host, and then it acts. Or the shim lets the guest act later.
The window is between the check and the use. This race is *different* from the race of two threads
that touch one variable. The adversary is the mediated party: a guest process, a container, or a
registry. The adversary can also be the host, which changes under the shim.

The fixes in this campaign are recognisable as exactly this remedy. The direction is therefore
correct:

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

**PDN does not need a general race-condition mechanism.** It needs three things. Two of the three
things are defects, and one of them lacks evidence. A "concurrency subsystem" has no consumer
in this tree. That subsystem is lock-free structures, atomics rework, or in-process signalling.

### 4a. The suspected torn read. CHECKED, WITHDRAWN.

An earlier revision of this document reported a torn read at `container_socket_activation/mod.rs:455-456` and
`:524-525`. The grounds were that two guards are read in sequence. The surrounding block settles this
question: **both read guards are held across both lookups**.

```rust
let active_guard = active.read().await;
let config_guard = configs.read().await;
if let Some(state) = active_guard.get(&socket_name) {
    if let Some(config) = config_guard.get(&socket_name) {
```

- So the state and config pair is consistent, and `configs.remove()` at `:202` cannot interleave. A writer
cannot take the configs write lock while any reader holds a read lock. No defect. The inference came from
two `get` calls, and the guards never had a lifetime check. That mistake is the same class as the reading
of a count out of a broken pattern. The evidence for a claim must be the thing that can refute it.

### 4b. The inconsistent-looking write. CHECKED, not a finding.

`native/image.rs:980` publishes metadata through a temporary file. That file is
`metadata.json.tmp-<uuid>`, which is write-then-rename. A call with a different shape appears at
`native.rs:146`: `std::fs::write(tmp.path().join("metadata.json"), ...)`. The surrounding function
settles this question. That line is inside `#[cfg(test)] mod image_runtime_config_tests`, in the
helper `fn scratch_dir(metadata: &str)`, and it writes into a `tempfile::TempDir`. It is a **test
fixture, not a production write path**. Therefore the only production metadata write in the tree is
the atomic one. This line is recorded here, so the next reader does not re-litigate it from the two
call sites that look different.

### 4c. Mechanisms that exist but have never been raced. Missing evidence, not missing code.

Each of these mechanisms has a single writer by construction. Therefore no new mechanism is
necessary. However, only sequential tests prove each one. The history of this campaign shows the
result when concurrency evidence is absent. A12 shows "not reproduced", and A6 shows "transient
only under load":

| mechanism | why it should hold under concurrency | what is missing |
|---|---|---|
| `blobstore::put_reader` two-phase ingest | tmp name carries `pid` and a counter (`:161`), publication is `rename_if_unchanged` with an identity check (`:196`) | a test that ingests one digest from two processes at once |
| `refcount::sweep` vs a live install | both take the same sidecar `flock` (`:168-170`); a lock that can be taken proves its owner is gone | a test that sweeps while an install holds the sidecar |
| store `list` during `install` | metadata is read only from regular files (`S3`), staging trees are named `.tmp-install-*` (`:899`) | a test that lists under install load and asserts no staging tree is ever listed |
| session register vs attach | registration takes `LOCK_EX` (`session.rs:314`), liveness is the descriptor (`C8`) | a test that attaches while the record is being created |

## 4d. A29 result. Three held under the race, the fourth did not.

`crates/proot-distro-core/tests/single_writer_races.rs` races all four rows of 4c. Three rows hold.
Two processes that ingest one digest leave one correct blob at the content address. The temp name
carries `pid` and a counter, and that name carries the race. Make the temp name non-unique, and the
ingest collides. A `refcount::sweep` that races a live install refuses by name and never touches the
tree. `scan_store` under install load never lists a `.tmp-install-*` tree.

The fourth row does **not** hold, and this is a finding, not a failure to find one.
`register_session_in` at `session.rs:241-251` **creates the record before it takes `LOCK_EX`**. An
attach scan that runs in that window takes `LOCK_SH` on a file that is unlocked but published. The
scan reads the file as dead, and it unlinks the file at `session.rs:354-368`. The registration then
returns `Ok` with a guard whose record is already gone. The session is invisible to `active_sessions`
for its whole life. Measurement over repeated runs gives 10-14 registrations lost per ~3400. Every
one of those registrations had its record file unlinked. The reproduction is committed as the ignored
test `a_register_publishes_its_record_before_it_locks_it`. The fix is an order change: lock before publish.
It is not a new mechanism.

## 5. What follows for the worklist

- **A28, defect**: close the torn read at `container_socket_activation/mod.rs:455-456` and
  `:524-525`. The acceptance criterion is a test that removes a config concurrently with a
  lookup. That test must never observe a state without its config, and the base tree must be
  quoted as failing.
- **A29, evidence**: make the four rows of 4c into real tests. The value is not new code. It is the
  change from the claim that a row holds to the claim that the row was raced and held. That form is
  the only form of concurrency claim this project has accepted anywhere else. **Done, with one
  change. Three rows raced and held. The session row raced and did not hold. See 4d, which raises
  the publish-before-lock order in `register_session_in` as a new finding.**
- **A30, CLOSED by the check above**: 4b was a test fixture, not a production write. No item.

This document does **not** propose a lock-free store, an in-process event bus, or `Condvar`-based
handshakes. It does **not** propose the replacement of `flock` with advisory atomics. None of these
has a consumer here. The shimming boundaries are process boundaries by construction.
