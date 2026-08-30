# Disk-safe mutation execution design

**Status:** Approved by the user and re-reviewed on 2026-08-28

## Review record

1. The architecture/security pass added anchored traversal, bounded enumeration,
   coordinator and lease ordering, synchronous boundary samples, and deferred cleanup
   when a process, output drain, or monitor may still be live.
2. The implementation-feasibility pass aligned snapshot preflight, persisted-session
   compatibility, output ownership, platform APIs, report fields, and TDD seams with the
   current repository.
3. The formal/race/delivery pass fixed same-observation precedence, lease lifetime
   through absence verification, execution-versus-delivery scratch, retained roots,
   report/session acknowledgement, Lean correspondence layers, and exact CI gates.
4. The resource-safety pass bounded walker descriptors, entries, elapsed scan time,
   inventory capture, and candidate count. It also removed destructor-driven recursive
   deletion and whole-report rendering.
5. The concurrency/platform pass separated read-only Windows scan sharing from
   destructive claim handles, anchored the user output directory, added setup rollback,
   and covered catchable termination signals.
6. The formal-executability pass removed an unavailable `Finset` dependency, represented
   secondary stop evidence, defined failed components as settled, and added complete
   counterexample records.
7. The orphan-process pass added cleanup-ready and heartbeat evidence before janitor
   reclaim, handle-anchored free-space queries, and a hard maximum of four mutation jobs.
8. The cleanup-resource pass made recursive removal resumable and bounded, distinguished
   deferred cleanup from removal failure, and bounded janitor work and retained
   diagnostics.
9. The capture/schema pass reconciled the inventory and command-log byte budgets, added
   the complete report/event golden-fixture set, and bounded all metadata probes.
10. The platform/formal pass fixed Windows output anchoring, removed combinatorial Lean
    trace expansion, and stated the same-credential process trust boundary explicitly.
11. The launcher/semantic-classification pass removed an unprovable `RLIMIT_FSIZE`
    mutation result and put Python environment provisioning outside the guarded command.
12. The formal lifecycle pass separated process-drain completion from one child exit,
    required successful deletion boundaries, and made report/delivery failure reject
    `finished`.
13. The correspondence pass added per-implementation corpus targets for Rust's two-root
    delivery topology and Python's one-root focused topology.
14. The crash/disk-full recovery pass added marker-derived atomic-output cleanup,
    chunk-aware reserve checks, exact bounded stderr recovery paths, and a fixed
    coordinator slot wire format.
15. The capture/memory pass bounded prior inventory, outcome JSON, selectors, reported
    paths, and diagnostic log reads.
16. The executable-gate pass made expected `rg` misses and concurrent-process checks
    distinguish a real no-match from tool or process-list failure.

## Purpose

Hoimin must stop mutation work before it exhausts a filesystem and must remove
large mutation artifacts after success, failure, timeout, or interruption. The
same policy covers the public `hoimin run` workspace lifecycle and the
repository-local Rust focused-mutation workflow.

The default policy is:

| Limit | Default |
| --- | ---: |
| Run-wide Hoimin-owned workspace or scratch bytes | `8GiB` |
| Minimum free bytes on every affected filesystem | `10GiB` |
| Disk sampling interval | `250ms` |
| Focused cargo-mutants jobs | `1` |
| Maximum focused cargo-mutants jobs | `4` |
| Retained focused command log bytes | `16MiB` per command |
| Maximum configured command log bytes | `64MiB` per command |
| Retained cargo-mutants inventory stdout | `8MiB` |
| Maximum prior-inventory or cargo-mutants outcome JSON | `8MiB` each |
| Retained candidate diagnostic bytes | `16KiB` per candidate |
| Retained run-wide candidate diagnostics | `16MiB` |
| Maximum focused JSON or Markdown report | `32MiB` each |
| Maximum UTF-8 and escaped reported path | `16KiB` each |
| One cleanup slice | `5s` and `50,000` examined entries |
| Current-owner cleanup budget | `60s` |
| Startup janitor budget | `30s` and `256` candidates |

Users may change workspace/reserve byte limits, choose a retained command-log limit from
one byte through 64 MiB, or choose a job count from one through four with explicit
options. A run may
not disable portable disk monitoring.

## Incident and root cause

The recent failures exposed two independent gaps.

1. `hoimin run --max-copy-size` charges the source files copied into worker
   workspaces. It does not charge files that a test command creates after the
   worker starts. A Rust test command can therefore create several GiB under
   `target/` without crossing the copy allowance.
2. `tools/focused_mutation.py` owns command deadlines and process termination,
   but cargo-mutants chooses a temporary build root. The wrapper neither owns
   nor monitors that root. The wrapper also retains one `mutants.out` tree per
   selected candidate and writes command stdout and stderr without a byte cap.

Parallel cargo-mutants jobs multiply build trees, memory demand, and transient
disk writes. Cleanup that depends on cargo-mutants destructors cannot run after
`SIGKILL`, a host crash, or power loss.

## Goals

- Apply a safe disk policy by default to `hoimin run`, `hoimin plan`, plan
  verification, resumed runs, and `tools/focused_mutation.py`.
- Stop scheduling before cleanup starts and classify a disk stop as
  infrastructure failure rather than a killed mutant.
- Terminate and reap owned processes before removing their workspaces.
- Remove large scratch, build, and per-candidate cargo-mutants output on every
  catchable exit path.
- Recover abandoned Hoimin-owned directories on a later invocation without
  inspecting or deleting unrelated temporary files.
- Preserve compact machine evidence after cleanup.
- Report the enforcement capabilities that the host supplied.
- Keep the policy consistent across Rust and Python through a Lean-generated
  finite case corpus.

## Non-goals

- Hoimin does not promise a portable hard aggregate quota. macOS, Linux, and
  Windows do not expose one common unprivileged API.
- Hoimin does not monitor arbitrary directories chosen by a test command.
  Owned roots and filesystem free space define the portable enforcement
  boundary.
- Hoimin does not delete Cargo caches, a repository `target/`, user-selected
  output, or any directory that lacks a valid Hoimin lease.
- This change does not make full-workspace cargo-mutants runs a delivery gate.
- This change does not infer that a disk-limited mutant was caught by tests.
- This change does not add a background service or require administrator
  privileges.
- Portable deadlines are cooperative between filesystem operations. Hoimin bounds its
  own traversal work but cannot preempt a single kernel or remote-filesystem call that
  does not return.
- The managed-root ACL is a user isolation boundary, not a sandbox between processes
  running with the same credentials. A deliberately malicious same-credential child can
  edit any user-writable management metadata; preventing that requires a sandbox or a
  distinct operating-system identity and is outside this change.

## Terminology

**Owned root** is a directory that Hoimin created below its managed temporary
root and protected with a valid live lease.

**Owned bytes** is the conservative logical size of regular files below all
owned roots in a run. The focused workflow also counts its output directory.

**Free-space reserve** is the minimum filesystem capacity that Hoimin leaves
unused for the operating system and other applications.

**Portable guard** combines owned-byte measurement with filesystem free-space
measurement. It runs on macOS, Linux, and Windows.

**Hard backstop** is a verified host capability that limits aggregate bytes for the
owned root independently of the portable sampler. The report must name the exact
capability probe and result.

**Lease** is a marker plus an operating-system file lock held for the lifetime
of an owned root.

**Anchored output** is a user-owned output directory that the focused wrapper opens once,
locks for the run, and accesses through that directory capability. Hoimin meters and
writes it but never recursively deletes it.

## Architecture

### Shared policy

Rust and Python use the same conceptual policy:

```text
DiskPolicy {
    max_owned_bytes: 8 GiB,
    min_free_bytes: 10 GiB,
    sample_interval: 250 ms,
}
```

The implementations remain language-native. A versioned Lean corpus defines
the decisions made from observations and lifecycle events. Rust and Python
adapters execute the same corpus cases against their public policy entry
points.

### Public Hoimin runtime

`RawRunLimits` and `RunLimits` gain:

```rust
pub max_workspace_size: u64,
pub min_free_space: u64,
```

The CLI exposes:

```text
--max-workspace-size 8GiB
--min-free-space 10GiB
```

The normalized values participate in plan serialization, configuration
fingerprints, resume compatibility, and report provenance.

Before materializing a snapshot, preflight computes the manifest's logical bytes plus
the requested worker aggregate with checked arithmetic. It refuses a value at or above
`max_workspace_size` before `create_disk_snapshot` writes anything. The live meter then
covers snapshot, workers, and files generated after materialization.

A new `workspace::disk` module owns these responsibilities:

- create and validate the managed temporary root;
- issue one locked lease for the run root that contains snapshot and worker children;
- register and unregister worker roots with one run-wide monitor;
- measure aggregate owned bytes without following symlinks;
- measure free bytes for every distinct filesystem that holds an owned root;
- publish the first terminal disk observation through an idempotent stop
  signal;
- record peak usage, minimum free space, measurement latency, and cleanup
  results.

The public runtime creates two leases. The execution root contains snapshots, workers,
process output, and analyzer scratch and is cleaned before `RunFinished`. The delivery
root contains only the bounded JSON report spool that must survive until final output.
Both roots are metered. The report handler writes and flushes `RunFinished`, drops the
spool, cleans and absence-verifies the delivery root, and only then acknowledges
`OutputEmitted`; session completion follows that acknowledgement. A write or delivery-
root cleanup failure therefore leaves the session incomplete.

The meter opens the owned root as a directory capability. It performs a streaming
depth-first walk, opens each child directory without following the last path component,
and enumerates through the open handle. It holds at most 129 directory handles, one for
each permitted depth plus the root, and never collects a directory listing before
processing it. A test process cannot redirect a scan outside the root by replacing a
checked directory with a symlink between metadata and traversal. The walk fails closed
after depth 128, 250,000 entries, or five seconds. These fixed caps bound the monitor's
file descriptors, identity set, memory, and cooperative shutdown work; an individual
filesystem operation remains subject to the operating system's blocking behavior.

The shell run loop treats the disk stop signal like a run-wide infrastructure
stop. It stops new effect dispatch, drains processes through the existing
termination and output paths, invokes workspace cleanup, and then reports the
typed failure.

The 250 ms setting is a delay after a completed periodic scan, not a 250 ms reaction
guarantee. With the five-second cooperative scan deadline, ordinary periodic detection
takes at most about 5.25 seconds plus scheduling; one blocking filesystem operation can
exceed that as stated in the non-goals. Synchronous pre-dispatch and post-drain samples
close lifecycle boundaries but cannot prevent a child from consuming the reserve between
samples.

### Focused Rust mutation workflow

`tools/focused_mutation.py` gains these options:

```text
--max-disk 8GiB
--min-free-space 10GiB
--jobs 1
--max-log-size 16MiB
--scratch-root PATH
--keep-scratch
```

The disk guard starts inside the Python process, so the supported invocation uses an
already provisioned `.venv/bin/python`. `uv sync --frozen` is a separate setup action
with free-space checks before and after it; the safe mutation command does not use
`uv run`, which may create or update an environment before the wrapper can sample disk.
The wrapper fails before output or scratch creation when the selected interpreter or
locked environment is absent.

The wrapper creates one leased run root below a managed temporary root. It sets
`TMPDIR`, `TMP`, `TEMP`, and `CARGO_TARGET_DIR` to children of that root for
inventory, baseline, and cargo-mutants commands, and sets
`CARGO_INCREMENTAL=0`. This redirects temporary copies and build trees into a
directory that the wrapper owns. The wrapper passes `--jobs 1` unless the user
chooses another value from one through four. The user-selected output must be absent or
empty at startup; Hoimin counts it during the run but never deletes it.

The wrapper leaves the shared Cargo home intact for cache reuse, but opens the effective
`CARGO_HOME` (or its nearest existing parent before first creation) as a capacity-only
root. Every pre-dispatch, periodic, post-drain, and report-boundary observation checks
that filesystem's free-space reserve and deduplicates it by filesystem identity. Hoimin
does not traverse, charge as owned bytes, retain, or delete Cargo home. The report labels
this capability `capacity_only:cargo_home` so it cannot be mistaken for cleanup
ownership.

Initialization rejects a non-empty output directory before it creates scratch or
launches a child. It atomically creates or opens the real directory, creates and locks a
fixed ownership marker with `create_new`, and retains an anchored directory handle until
the last report write finishes. A concurrent invocation cannot pass the same empty-root
check and interleave evidence. Once accepted, the wrapper writes only its compact record
and bounded command evidence there. The output root participates in every synchronous
and periodic sample even though cleanup never owns it. The output may not be a symlink
or reside inside either implementation's canonical managed root.

Before it creates the output ownership marker, the wrapper queries capacity through the
opened parent or output handle and requires free bytes above the configured reserve. The
marker has a 64 KiB schema cap and remains as provenance after a successful run. Every
subsequent checkpoint uses the guarded writer; no unmetered report write occurs between
ownership acquisition and guard startup.

Atomic report temporaries have deterministic names derived from the marker's run ID and
the fixed destination kind. At most one exists at a time. A catchable failure closes and
unlinks that exact regular file. On a later invocation, the wrapper may lock and validate
an abandoned output marker, revalidate the anchored output identity, and unlink only the
two derivable regular non-symlink temporary names. It preserves every completed report
and other entry. If the directory then contains only the abandoned marker, it removes
that wrapper-created marker and may reuse the empty directory; otherwise it rejects the
non-empty output after bounded orphan-temporary cleanup. This recovery performs no
recursive deletion and does not accept a path or temporary name from report contents.
For an existing abandoned marker this exact orphan cleanup precedes the fresh-run reserve
check, so a disk-full condition cannot prevent deletion of the wrapper's bounded
temporary. The wrapper then samples capacity again and either continues from an empty
directory or rejects without creating scratch.

On Unix, output creation and atomic replacement use directory-relative operations on
the held descriptor. On Windows, the wrapper opens the output directory without
`FILE_SHARE_DELETE` and keeps that handle through the final flush, so another process
cannot rename or delete the root while path-based child writes occur. It rejects reparse
points and verifies the final path, volume, and file identity before and after each
atomic replacement. This differs intentionally from read-only meter handles, which do
share deletion so candidate compaction can proceed.

Inventory is also bounded. The wrapper retains at most 8 MiB of cargo-mutants list
stdout and never more than the stdout share of the configured combined command-log
allowance. With the default 16 MiB combined allowance this is exactly 8 MiB. It accepts
at most 10,000 discovered entries, 1,000 requested selectors of at most 16 KiB encoded
bytes each, and 1,000 selected candidates. Crossing a cap,
or lowering `--max-log-size` so that complete inventory JSON does not fit in the stdout
share, is a typed tool or infrastructure failure before baseline or mutation work.
Explicit symbol selection must resolve each symbol to one production function.

Every user/tool JSON input is bounded before decoding. `--prior-inventory` and each
cargo-mutants `outcomes.json` are regular files of at most 8 MiB, read as at most the cap
plus one byte and rejected on overflow. A candidate compiler/debug log is never loaded in
full; the extractor obtains only bounded prefix/tail slices with seek/read and records the
observed file size. Parsed candidate and string counts still obey the inventory limits,
so a small compressed-looking JSON structure cannot create an unbounded object graph.

Each selected candidate uses a child run directory. After cargo-mutants exits,
the wrapper reads the exact inventory and outcome, copies the bounded diagnostic
slice into the compact run record, and deletes the candidate directory. Later
candidates do not accumulate earlier `mutants.out` trees.
A candidate-directory cleanup that fails or defers stops dispatch before the next
candidate and leaves the whole run incomplete; the wrapper never trades cleanup debt for
additional mutation progress.

The command runner replaces direct stdout and stderr files with draining
spools. The two spools share one per-command `--max-log-size` allowance and
retain deterministic prefixes and tails within that allowance. The runner
continues draining discarded bytes so a full pipe cannot block the child. The
command record stores total observed bytes, retained bytes, and truncation
status for each stream.
Each drain reads at most 64 KiB at a time into fixed-capacity `bytearray` prefix and
circular-tail buffers. It never grows a tail by repeated immutable-byte concatenation;
temporary live buffer capacity stays within the command allowance plus two read chunks.
`--max-log-size` may not exceed 64 MiB. The full per-command prefix/tail buffer exists
only while that command is active. After classification, the run record keeps at most
16 KiB of diagnostic bytes for that candidate and releases the spool; one shared
16 MiB allowance bounds all candidate diagnostics retained across the run. Metadata for
additional candidates remains, but their diagnostic body is deterministically truncated.
Short version and repository-discovery probes use the same draining primitive with a
fixed 64 KiB combined allowance. Production code has no unbounded
`capture_output=True` path.

On every focused-workflow catchable exit, including `SIGINT` and `SIGTERM`, the wrapper
closes dispatch first, terminates
and reaps the active command, joins both output drains, extracts compact outcome
evidence, stops the disk meter, and then cleans or retains scratch. It writes the
final JSON and Markdown after the cleanup attempt. The meter therefore remains
active while a terminated child can still write.

`--keep-scratch` marks the lease as retained after the run. It does not disable
size monitoring, free-space monitoring, process cleanup, or bounded logs. The
report prints a deletion command containing the exact retained path.

## Measurement contract

### Owned bytes

The meter walks each owned root through anchored directory handles with symlink
following disabled.

- It sums regular-file logical lengths with checked arithmetic.
- It counts a hard-linked file once per run when the platform exposes a stable
  file identity. Without one, it counts each directory entry and records the
  conservative fallback.
- It treats a vanished entry as a concurrent deletion and continues.
- It returns a typed measurement failure for permission errors or integer
  overflow.
- It performs streaming depth-first traversal with at most 129 open directory handles.
- It fails closed after depth 128, 250,000 entries, or five seconds.
- It counts the focused workflow output directory because retained reports and
  logs consume disk too.

Logical lengths overcount sparse files and copy-on-write extents. The false
positive preserves disk safety. Filesystem free space remains the independent
physical-capacity signal.

The monitor runs one scan at a time. It waits `250ms` after one scan completes before it
starts the next, so a slow scan cannot create a continuous I/O loop. The walker checks a
monotonic five-second deadline at least every 256 entries. It records sampling delay and
scan duration. A deadline overrun is a measurement failure. Synchronous samples share
the same single-scan lock and deadline.

The runtime also samples synchronously before each process dispatch and after
process exit plus output drain, before it classifies the result. A threshold or
measurement failure at that boundary wins over the process result, so Hoimin
cannot credit a disk-stopped mutant as killed merely because both events became
ready together.

### Free space

The monitor groups roots by filesystem identity and queries available bytes through the
same open root capabilities. Unix uses `fstatvfs` on the directory descriptor. Windows
derives the volume path from the verified final handle path and confirms its volume
identity before `GetDiskFreeSpaceExW`; it never re-trusts the original root path. It
performs this query before analyzer, baseline, or mutant work
starts. A filesystem at or below `10GiB` fails preflight without launching a
child.

During execution, an observation at or below the reserve triggers a stop. The
reserve supplies reaction headroom for writes that occur during one sample and
process termination. The portable sampler cannot prevent one child from
writing more than the reserve inside a single observation interval. Reports
state this limit.

For one successful observation that reaches both boundaries, the reserve reason is
primary because it represents immediate filesystem capacity; the owned-size reason is
retained as secondary evidence. A measurement failure has its own terminal reason and
does not synthesize either numeric observation.

An unavailable free-space query or an unidentifiable filesystem fails closed.

### Hard backstops

The portable aggregate guard stays active on every host.

- A user may place `--scratch-root` on a capacity-limited filesystem. Hoimin
  records the filesystem capacity and free-space observations. It labels an
  aggregate hard backend only when a platform capability probe verifies a
  quota for the owned root. A path choice or user assertion does not prove a
  quota.
- A future aggregate quota backend implements the same capability interface.
  It may not weaken or replace the portable guard.

The report uses `portable_only` or a named verified aggregate backend. A per-file limit
is not installed for scored mutation commands: a child can catch `SIGXFSZ`, observe
`EFBIG`, remove the file, and later exit like an ordinary test failure. The parent cannot
prove that hidden path was infrastructure rather than a killed mutant, so using it would
violate the rule that a disk-limited candidate never improves mutation score.

## Stop and cleanup state machine

The first terminal disk observation wins:

```text
running
  -> stop_requested(reason, observation)
  -> processes_draining
  -> execution_scratch_cleaning
  -> evidence_writing
  -> delivery_scratch_cleaning
  -> finished(primary_outcome, cleanup_outcome, report_outcome)
```

The transition order has these rules:

1. `stop_requested` prevents new analyzer, baseline, and mutant dispatch.
2. Later disk or process errors do not replace the first disk reason. Reports
   retain them as secondary failures.
3. The runtime terminates and reaps every owned live process tree before workspace
   removal.
4. The runtime drains bounded output before removing a workspace.
5. The runtime unregisters roots, stops and joins the disk monitor, then issues
   one logical cleanup request for each execution root. A request may contain
   bounded platform retries.
6. The report handler issues the delivery-root cleanup request after output flush and
   before acknowledging output success.
7. Cleanup failure sets `cleanup_outcome=failed`. It does not replace the
   primary run outcome.
   Cleanup budget/lifecycle exhaustion sets `cleanup_outcome=deferred`; it is likewise
   incomplete but remains distinct from a hard removal failure.
8. The runtime writes evidence after execution cleanup. A report or post-flush delivery
   cleanup failure does not erase an earlier cleanup failure or primary disk reason; it
   is exposed by the typed failure path because the report cannot describe its own
   failed delivery finalization.
9. Managed-root and child guards never perform recursive deletion from `Drop` or Python
   finalizers. Catchable paths call explicit ordered cleanup. Unwinding or interpreter
   shutdown releases the lease and leaves the root for the next janitor.
10. If setup fails after either lease is published, setup rolls back in reverse order
    only after confirming that no process, drain, or monitor was started. A failure to
    prove that condition abandons the root for the janitor.
11. A destructive cleanup outcome (`clean` or `failed`) is accepted only after process
    reap, both output drains, and monitor join have succeeded. A failed or unproven
    boundary permits only `deferred` or explicit `retained`; component failure being
    terminal for reporting does not make recursive removal safe.

The lifecycle machine is constructed only after a process/drain/monitor owner has
started. Rule 10's pre-owner setup rollback is a separate owned-root operation guarded by
the explicit “no owner started” proof; it is not encoded as a cleanup transition from a
state whose pending components might later become live.

Disk stops use typed infrastructure codes:

| Code | Meaning |
| --- | --- |
| `workspace.size.exceeded` | Aggregate owned bytes reached the configured maximum |
| `filesystem.reserve.reached` | Available bytes reached the configured reserve |
| `disk.measurement.failed` | Hoimin could not establish a safe measurement |
| `workspace.cleanup.failed` | Identity, permission, traversal, or absence verification failed |
| `workspace.cleanup.deferred` | Safe cleanup stopped at a lifecycle or resource budget and will be retried |

A candidate interrupted by one of these failures remains unverified. Mutation
score accounting excludes it.

## Lease and stale cleanup protocol

Public Hoimin and the focused workflow use separate managed roots below the
current user's temporary directory. The root has user-only permissions or ACLs.
Each implementation scans only its own direct children.

Each managed root contains one coordinator lock. Creation, active-to-deleting
rename, and janitor claim run under that lock. A per-run lease still proves
liveness. The coordinator closes the race between the current owner's cleanup
and a janitor in another Hoimin process.

The coordinator marker and every lease, heartbeat, retention, or cleanup-ready marker
have fixed small schemas and
a 64 KiB read cap. A managed root scan considers at most 100,000 direct children per
invocation; reaching either cap is a typed measurement/setup failure rather than an
unbounded allocation.

Creation follows this sequence:

1. Create an unadvertised staging directory with `create_new` semantics.
2. Create a regular lease marker containing schema version, run ID, creation
   time, and owner kind.
3. Acquire and hold an exclusive operating-system lock on the marker.
4. Acquire the coordinator lock and rename the staging directory to the managed
   active prefix.
5. Create worker or cargo-mutants children below it.

A crash can leave a staging directory before the rename. The janitor removes a
staging entry only when it is a direct, non-symlink child with the staging
prefix, is older than 24 hours, and contains no entry other than an optional
valid lease marker. It ignores a staging entry with user payload or an unknown
entry. If a marker exists, the janitor validates it and acquires its lease
nonblocking before claiming the staging entry.

The janitor accepts a deletion candidate only when all checks pass:

- the entry is a direct child of the canonical managed root;
- the child name matches the managed prefix;
- the child and marker are real directories/files rather than symlinks;
- the marker schema and owner kind match the scanning implementation;
- the marker does not request retention;
- the janitor acquires the lease lock without blocking;
- the marker and direct-child identity still match after the lease and
  coordinator locks are held.

The lock closes the PID-reuse race. The operating system releases it after an
uncatchable process death. An active process keeps the lock and the janitor
skips its directory.

`--keep-scratch` creates a separate regular retention marker with `create_new`
semantics while the lease lock remains held. The janitor validates both marker
types and skips a retained root. It does not rewrite the locked lease file.

An unlocked lease does not prove that descendants exited. Only after process reap,
both output drains, and the disk monitor have all terminated does the owner create and
flush a separate cleanup-ready marker while it still holds the lease. Every monitor
holds its own shared lease guard until its thread exits, so a join timeout cannot expose
an unlockable root to the janitor while a scan is still live. While a run is live, the
owner refreshes the modification time of a fixed,
open heartbeat marker at least once per minute without replacing its identity. A
heartbeat update failure stops dispatch through the measurement-failure
path. A janitor may reclaim an unlocked active or deleting root only when cleanup-ready
validates or the last valid heartbeat is at least 24 hours old. A missing, malformed, or
future heartbeat preserves the root. Staging roots without cleanup-ready evidence use
their creation time because they were never published as active. This grace period
prevents a new invocation from deleting below an orphaned child that survived parent
panic, `SIGKILL`, or host supervision failure.
Cleanup-ready and heartbeat validation checks structure, run identity, ordering, and
time; it is not cryptographic proof against the same-credential process excluded by the
threat model.

Normal cleanup closes child processes and their inherited handles, acquires the
coordinator lock, renames the root to the managed deleting prefix while the
lease remains held, and then releases only the coordinator before removing the tree.
The per-run lease remains locked until anchored removal and absence verification finish;
only then is its surviving handle closed. Removal uses an anchored no-follow walk and
treats `NotFound` as success. It is a resumable streaming depth-first operation. One
slice examines at most 50,000 entries and performs at most five seconds of cooperative
work. Cleanup deliberately does not reuse the meter's depth-128 stack: otherwise the
tree that triggered a depth stop could never be removed. It stores a no-follow cursor of
at most 4,096 components and 64 KiB of encoded names, holds at most the root, current
parent, and current child handles, and reopens each cursor component from the anchored
root while rechecking saved identities. A depth/cursor overflow is a typed hard cleanup
failure. Each completed file or empty-directory removal durably shrinks the tree; a
later slice starts from the remaining root. Permission repair is restricted to the owned
tree.

The current owner repeats slices for at most 60 seconds. Startup reclamation spends at
most 30 seconds and considers at most 256 eligible direct-child candidates, one slice
per candidate before a second pass, so one large tree cannot starve every other root.
Under the coordinator lock, a streaming selection scan runs for at most five seconds and
uses a fixed-schema persisted lexicographic cursor to choose the next 256 names after the
prior invocation; it advances after selection even when a selected root is deferred and
wraps after the final name. The stable coordinator protocol prevents compliant creators
from changing the direct-child set during selection. This gives bounded cross-invocation
fairness without collecting all names.
The coordinator is exactly 1,025 bytes: lock byte zero followed by two 512-byte cursor
slots at offsets 1 and 513. Each slot contains eight-byte magic `HMCUR001`, little-endian
schema `u32`, little-endian generation `u64`, little-endian cursor length `u16`, a
zero-padded 480-byte ASCII managed-child name, six reserved zero bytes, and a
little-endian IEEE CRC-32 of the first 508 slot bytes. A length above 480, a name outside
the managed grammar, nonzero padding, bad CRC, wrong schema/magic, or unequal contents at
the same highest generation is invalid. Equal highest-generation slots choose slot zero.
The creator preallocates and flushes both valid empty slots before any lock user opens the
file. The janitor reads both complete slots, chooses the valid highest generation, and
writes `generation + 1` to the inactive slot with a positioned write plus file flush.
Generation overflow is a typed cursor-persistence failure. Updating the inactive slot
uses no directory-entry allocation. A cursor-write or flush failure does not prevent
cleanup of candidates already selected; the previous valid slot remains authoritative,
so a disk-full janitor can still free space and retry.
The 100,000-direct-child cap remains a fail-closed enumeration ceiling, not a promise to
retain 100,000 records. Cleanup and reclaim reports keep at most 256 path/error details
of at most 4 KiB each and aggregate all additional/truncated counts.

Absence verification yields `clean`. A permission, identity, traversal, or no-progress
error yields `failed`. Reaching a slice or total budget after making safe progress yields
`deferred`; the locked `.deleting-` root remains eligible for a later janitor. Deferred
cleanup is incomplete and uses `workspace.cleanup.deferred`, but it is not mislabeled as
a removal error. Individual filesystem calls cannot be preempted portably, so the time
budgets are checked before and after each entry operation.
If a parent-relative, identity-checked `rmdir` returns success while its syscall crosses
the deadline, that successful syscall is the authoritative namespace-removal proof and
the result is `clean`; cleanup starts no follow-up `stat`. When budget remains, cleanup
still performs an anchored absence check so a surviving name or injected no-op adapter
is `failed`.

The sole path-evidence exception is an identity-integrity or namespace-I/O failure that
prevents validating any current path for the still-open owned-root capability. It yields
`deferred` with no `remaining_root`, records a bounded integrity diagnostic, and stops
further dispatch. Because no safe path can be rescanned, final evidence conservatively
carries the complete last pre-clean owned-byte floor and identity provenance into the
post-clean observation. This exception never reports `clean` or removed logical bytes.

If the disk monitor cannot join within the shutdown budget, the runtime marks cleanup
deferred, emits no cleanup-ready marker, and leaves the monitor's shared lease guard
locked until that thread exits. A fallback destructor may not race removal against the
live monitor. If the process dies, the operating system closes both the scan handles and
lease atomically with process teardown; a later janitor must still satisfy heartbeat
grace before reclaim.

The janitor ignores unknown entries, invalid markers, symlinks, and retained
leases. It reports them without deleting them.

Rust uses capability directory handles for janitor removal. Python uses
directory-relative file descriptors with no-follow opens on Unix. On Windows,
read-only meter handles grant `FILE_SHARE_DELETE` so concurrent per-candidate cleanup
does not fail because a scan observed the directory. Each handle stays bound to the
opened object, rejects reparse points, and has its final path and file identity verified.
Destructive janitor handles request `DELETE` access and perform handle-relative
rename/disposition after the monitor has joined. Per-candidate owner cleanup may run
while the run-root monitor is active because meter handles share deletion and treat
vanished entries as concurrent cleanup. A platform that cannot establish this
anchored walk fails closed instead of falling back to path-based recursive
deletion.

## Error and report contract

The Rust run report and focused JSON record include:

- configured maximum and reserve;
- start and end free bytes per filesystem;
- peak owned bytes and minimum observed free bytes;
- sample count and maximum measurement duration;
- tagged enforcement capabilities: portable guard, capacity-only root, or verified
  aggregate backend with its successful bounded probe evidence;
- the first stop reason and observation;
- secondary process, measurement, cleanup, and report errors;
- every execution root and its cleanup result;
- cleanup status (`clean`, `failed`, `deferred`, `retained`, or
  `cleanup_after_delivery`), examined/removed aggregate counts, bounded diagnostic
  details, and any remaining-root identity;
- the delivery scratch identity and the explicit fact that its cleanup occurs after
  report bytes are flushed, so the report does not claim its own future cleanup;
- removed logical bytes for an absence-verified owned root;
- signed available-byte change per affected filesystem when both readings exist;
- stale leases reclaimed during startup;
- bounded-log observed and retained byte counts.

Cleanup success requires absence verification after recursive removal. A cleanup
callback that returned without removing the path does not count as success. A partial
slice never reports `removed_logical_bytes`; that field is set only when the complete
pre-clean owned size is known and root absence is verified.
After process drain, the runtime takes a final pre-clean sample, stops and joins
the monitor, cleans execution scratch, and queries the affected filesystem parents once
more. It records the pre-clean logical size as `removed_logical_bytes` only after
absence verification. It records end free space and a signed free-space change when
both filesystem readings are available; otherwise those fields remain unknown. The
report never calls a free-space delta physically reclaimed bytes because concurrent
writes, sparse files, hard links, and copy-on-write extents make that attribution
invalid. Delivery scratch is
already static at monitor shutdown and is cleaned by the report acknowledgement path.

The runtime keeps primary-run, cleanup, and report-delivery outcomes as three
orthogonal internal values. A successfully delivered report contains the first two and
all disk observations. If delivery itself fails, that fact cannot be written reliably
to the failed channel; Hoimin exposes it through the typed return error, stderr, and an
incomplete session record. The implementation must not predeclare delivery success in
the report it is still attempting to write.

For a session-backed run, cleanup precedes `RunFinished`; successful output acknowledgement
includes delivery-scratch absence verification and precedes the transaction that marks
the session complete. A report, delivery cleanup, or later session-finalization failure
therefore leaves the session incomplete and is returned through the typed error path.
`RunSummary.complete` describes the primary run plus execution cleanup, not a claim that
its own delivery, delivery cleanup, or subsequent session transaction has succeeded.

Every checkpoint occurs at a quiescent boundary after process reap and drain. The
focused wrapper reuses that boundary's required synchronous full-registry sample to
create a generation token; scratch registration and child dispatch stay frozen until
the checkpoint finishes. The guarded writer splits encoder output into at most 64 KiB
writes, rejects a total above 32 MiB, and checks
`base_owned_bytes + temporary_bytes + next_chunk_bytes < max_owned_bytes` plus
`available_bytes > min_free_bytes + next_chunk_bytes`, with checked arithmetic, before
every chunk. The initial reading already includes the old
destination, while the counter adds only bytes newly written to the atomic temporary.
This permits a small report under a user limit below 32 MiB without weakening the hard
report cap. The writer verifies the
output identity, generation token, and free space again after flush and before replace;
it does not rescan the whole workspace for every chunk. If a boundary is reached, it closes and
unlinks only that exact wrapper-created temporary and surfaces report delivery failure
through the typed return and stderr. The prior atomic report remains intact.
If the final report cannot be written while scratch is retained, deferred, or failed,
stderr emits one escaped line capped at 20 KiB containing the typed report code and the
already validated remaining owned-root path. It never embeds child output. This fallback
allocates no file and gives the operator a recovery target when the disk reserve blocks
the report itself.
Execution cleanup invalidates a pre-clean token. Final evidence therefore uses the
required post-clean absence/end-free observation to issue a new token; retained or
deferred scratch remains registered in that observation.
The Markdown renderer yields sections and candidate rows to the guarded writer; it does
not build a complete `str` or list of all rendered lines. JSON encoding uses
`JSONEncoder.iterencode` over the existing bounded dataclasses and does not first create
a recursive duplicate through `RunRecord.to_dict()`. Both paths retain only the bounded
run record, and discovery
caps bound that record's candidate collection.

## Configuration and persisted-data compatibility

New CLI runs receive the safe defaults. Byte values must be positive and use
the existing byte parser.

Plan manifests and resume records include both disk fields in their normalized
configuration and compatibility fingerprints. The implementation bumps the
persisted schema version. Hoimin rejects an older incomplete plan or session
with an error that instructs the user to create a new plan. It does not insert
disk defaults into an existing signed or fingerprinted artifact.

Verification reconstructs the same policy and rejects a manifest whose disk
fields differ from the recorded fingerprint.

With `--resume`, Hoimin first inspects the newest incomplete session record. If
no current fingerprint matches and that record uses an older fingerprint schema,
Hoimin returns a typed incompatibility error instead of starting a new run
silently. A run without `--resume` may start a new session record.

New report output uses schema 3 and requires disk evidence. Historical schema-2 reports
remain readable only by a version-specific `hoimin progress` compatibility parser that
extracts progress fields without constructing current execution configuration or current
`OutputEvent` values. This path does not add defaults to plan/session deserialization.
The published run-event and run-result JSON Schemas describe v3; archived v2 goldens are
compatibility inputs, not documents accepted by the current-output schema.

## Security boundary

- The janitor never receives an arbitrary deletion path from a report, marker,
  environment variable, or child process.
- Canonical containment, direct-child membership, non-symlink metadata, marker
  validation, and lease acquisition all precede deletion.
- The runtime does not invoke `git clean`, delete repository caches, or delete a
  user-provided output directory.
- `--scratch-root` selects the managed root's parent. Cleanup still requires a
  child lease created during the current or an abandoned Hoimin run.
- Diagnostic reports escape paths and child output before rendering Markdown or
  JSON.
- The user-only ACL separates other accounts. Marker validation does not claim to
  authenticate metadata against a malicious process already running under the same
  account.

## Lean oracle

The Lean model covers decision and lifecycle semantics. It excludes filesystem
walking, clocks, process APIs, byte parsing, and report presentation.

The audit uses this correspondence worksheet before it defines the model:

| Premise or observation | Lean representation | Production setup and observation | Mode |
| --- | --- | --- | --- |
| Owned/free threshold class | `below`, `equal`, `above`, or meter failure | Public Rust and Python policy calls with injected readings | `strict` |
| First stop and post-stop dispatch | `stop : Option Reason`, dispatch event | Public policy lifecycle snapshot | `strict` |
| Root-specific cleanup idempotency | finite root IDs and requested/terminal sets | Public policy lifecycle calls; runtime wiring uses owned test seams | `strict` for policy, `internal-fixture` for wiring |
| Delivery cleanup after report terminal | delivery-root subset and report state | Instrumented report handler with an owned delivery spool | `internal-fixture` |
| Process reap, output drain, monitor join | explicit component states | Controlled Rust shell and Python runner fixtures | `internal-fixture` |
| Report delivery failure | report component failure | Injected report writer | `internal-fixture` |
| Uncatchable host death | abstract crash event | No deterministic same-premise public execution | `model-only` |
| Pre-owner setup rollback | outside the lifecycle model | Owned-root fixture proves no process, drain, or monitor started | `internal-fixture` |

Corpus records use only `strict`, `internal-fixture`, `model-only`, or
`infrastructure-error`. Each record also names its `policy` or `runtime` layer and a
nonempty `implementation_targets` subset of `rust` and `python`. Policy records target
both implementations. Complete one-root focused-runtime traces target Python; complete
two-root delivery traces target Rust because public Hoimin owns a delivery scratch root.
The Python wrapper has one leased scratch root and an anchored user-owned output that it
must not recursively delete. A smaller runtime transition case targets both only when
both adapters configure the same premise and expose the complete observation. Each
adapter consumes every record naming it, rejects unknown/duplicate/empty
targets and unknown layer/mode values, and compares complete observations for `strict`
and `internal-fixture`. Infrastructure failure yields no semantic verdict. The audit
reports per-target expected/executed case-ID sets, and an unexecuted applicable record
cannot count as a match.

The finite model contains:

- observations below, at, and above each threshold;
- measurement success and failure;
- zero, one, and two active work items;
- first and secondary stop reasons;
- cleanup success, failure, resource/lifecycle deferral, and explicit retention for each
  owned root;
- report success and failure.

The model tracks cleanup requests and clean, failed, deferred, or retained outcomes by
root ID. Clean, failed, and retained are terminal for lifecycle reporting; deferred is
explicitly incomplete and blocks `finished`. It also tracks process-drain, output-drain, and monitor-join
states, so `finished` requires all three boundaries plus a terminal cleanup
outcome for each owned root.
For process-drain, output-drain, monitor, and report components, “terminal” means
settled as success or failure. A cleanup request requires the process-drain,
output-drain, and monitor components to be
terminal. Clean or failed destructive cleanup additionally requires all three to have
succeeded; any failed component permits only deferred or retained. A failed report
component permits delivery-root cleanup after the other three components succeed, while
still preventing output acknowledgement.
`finished` represents successful output acknowledgement/session completion, not merely
returning an error to the caller. It therefore also requires report success and `clean`
for every delivery root. Report failure or delivery cleanup failure may reach a settled
error state after bounded cleanup, but neither transition can set `finished`. Execution
cleanup failure or explicit execution-root retention may be reported as a completed
failed/retained run when delivery succeeds; deferred cleanup remains incomplete.

The bounded refutation pass enumerates the three threshold classes plus meter
failure and explores eight normalized event families: dispatch, observe, process
terminal, output terminal, monitor terminal, cleanup, report terminal, and finish. At
depth five the family-skeleton count is exactly
`1 + 8 + 8² + 8³ + 8⁴ + 8⁵ = 37,449`. The generator does not expand every skeleton
across a larger root/payload alphabet. It uses canonical representatives, proves root
renaming and payload-class symmetry over the finite model, and adds one fixed witness
for each noncanonical root and payload class. Fixed traces are not depth-five samples:
the corpus includes complete ordered success, report-failure/rejected-finish,
delivery-cleanup-failure/rejected-finish, monitor-failure/deferred,
unsafe-cleanup-rejection, retained, one-root Python completion, and Rust delivery-cleanup
traces of up to 16 events. It
records skeletons, representative traces, accepted transitions, reachable states,
elapsed time, and peak memory. Imported Lean modules contain the transition
definitions and theorems;
the executable alone performs enumeration, sensitivity checks, and JSONL
serialization.
The generator reports `bounded_skeleton_count=37449` and a separate fixed-case count;
the tracked corpus total includes the fixed traces and is not asserted to equal 37,449.

Lean proves these twelve model claims:

- a stop request disables future dispatch;
- the first terminal reason stays unchanged;
- a simultaneous reserve/size observation keeps reserve primary and size secondary;
- each owned root receives one cleanup request;
- cleanup failure cannot produce `cleanup_outcome=clean`;
- a cleanup request is not accepted before process-drain, output-drain, and monitor components
  are terminal;
- clean or failed destructive cleanup is not accepted unless those three components
  succeeded;
- the model cannot reach `finished` before cleanup attempts complete.
- the model cannot reach `finished` before process drain, output drain, and monitor join
  are terminal.
- a delivery-root cleanup request is not accepted before the report component is
  terminal.
- `finished` implies that the report component succeeded;
- `finished` implies that every delivery root is clean.

Lean generates a versioned corpus. Rust and Python adapters invoke their real
policy entry points for each case. They classify results as `match`, `mismatch`,
or `infrastructure_error`. Deliberately broken policies must fail corpus
comparison. The audit report distinguishes model proofs from implementation
observations.

Each failed or deliberately broken case records the claim, model boundary, finite
domain or theorem premises, minimal input or trace, intermediate states,
classification, implementation correspondence, owner question, and reproduction
command. The audit labels walker descriptor/entry/deadline caps, OS handle behavior,
signal delivery, and renderer memory behavior outside the Lean model; deterministic
production tests provide `internal-fixture` evidence for those claims.

## Testing

### Rust TDD

Tests inject a disk meter, clock, stop sink, and filesystem fixture. They cover:

- preflight refusal at the free-space boundary;
- aggregate size stops across several workers;
- no dispatch after a stop;
- sticky first failure and retained secondary errors;
- measurement failure;
- process drain before workspace cleanup;
- non-destructive guard `Drop`, setup rollback, and janitor recovery after unwinding;
- bounded descriptor count, entry count, hard-link identity storage, and scan deadline;
- resumable cleanup slices, owner/janitor total budgets, fairness, no-progress failure,
  and bounded diagnostic retention;
- one cleanup attempt per owned root;
- failed cleanup and absence verification;
- live, abandoned, retained, malformed, and symlink lease entries;
- plan, verify, resume, and report schema behavior;
- macOS, Linux, and Windows launcher integration;
- existing workspace copy, reset, cancellation, and output precedence tests.

### Python TDD

Tests use fake cargo-mutants and injected measurements. They cover:

- explicit `--jobs 1` and option validation;
- owned `TMPDIR`, `TMP`, and `TEMP` propagation;
- size, reserve, and measurement stops;
- process-tree termination and reap before removal;
- compact outcome extraction before candidate-tree deletion;
- unique output ownership and anchored report writes under concurrent startup;
- bounded recovery of deterministic atomic output temporaries after simulated process
  death, without deleting completed reports or foreign entries;
- bounded inventory bytes, discovered entries, selected candidates, and streamed Markdown;
- bounded prior/outcome JSON, selector/path bytes, and seek-based diagnostic slices;
- bounded stdout and stderr drain without child blockage;
- cleanup after success, tool failure, timeout, `SIGINT`, and `SIGTERM`;
- abandoned lease recovery on the next invocation;
- `--keep-scratch` retention with active limits;
- safe rejection of malformed or foreign cleanup entries.

Tests model large byte counts. They do not create GiB-scale files.

### Integration and delivery gates

- Rust formatting and all-target, all-feature Clippy with warnings denied;
- Rust workspace tests and standalone CLI end-to-end tests;
- frozen Python focused-mutation tests;
- Rust 1.88, pinned-nightly shuffled tests, contracts, and wheel smoke tests;
- native Linux and macOS disk/free-space behavior;
- Windows lifecycle and locked-file cleanup behavior in CI;
- Lean build, broken witnesses, corpus freshness, Rust adapter, and Python
  adapter;
- a small real focused cargo-mutants run with one worker under the new guard.

The delivery does not run the full Rust workspace mutation inventory. Focused
mutation targets only the new Rust policy and lifecycle code. The run cleans its
scratch before completion and records peak use, removed logical bytes, and signed
free-space change without claiming physical byte attribution.

## Documentation

README and `docs/development.md` explain:

- the distinction between copy size, generated workspace size, and free-space
  reserve;
- safe defaults and examples for raising them;
- the 250 ms post-scan delay, five-second cooperative scan deadline, and lack of a hard
  reaction guarantee for a blocking filesystem call;
- focused mutation's single-worker default and compact evidence retention;
- cleanup behavior, `--keep-scratch`, stale recovery, and exact manual cleanup;
- portable-versus-verified-aggregate capability labels and the interpreter-startup
  boundary;
- why direct unguarded `cargo mutants --workspace --jobs N` is discouraged.

Repository mutation examples use `tools/focused_mutation.py` or a guard-backed
command. They do not recommend parallel full-workspace mutation as a routine
gate.

## Acceptance criteria

The change is complete when:

1. New CLI and focused-workflow runs use the agreed defaults without opt-in.
2. Deterministic tests show that size, reserve, and measurement failures stop
   dispatch and trigger process drain plus cleanup.
3. Catchable success and failure paths remove each non-retained owned root within the
   60-second owner budget or leave a validated, incomplete `deferred` root for bounded
   janitor retry; neither case runs unbounded cleanup.
4. A later invocation removes a simulated cleanup-ready or 24-hour-old abandoned valid
   lease and preserves
   live, retained, malformed, symlink, and foreign entries.
5. Reports distinguish portable aggregate monitoring, verified aggregate backends, and
   caller-provided capacity roots; no per-file signal path can receive mutation credit.
6. Disk-stopped candidates remain unverified and do not improve mutation score.
7. Rust and Python match the Lean-generated state-machine corpus.
8. The one-worker real focused mutation evidence stays under the configured
   limits and removes its scratch.
9. All normal, compatibility, contract, platform, and documentation gates pass.
10. The guard itself stays within its descriptor, entry, scan-time, inventory, candidate,
    retained-log, cleanup-slice, janitor-work, and diagnostic caps, and no destructor
    recursively deletes a live workspace.
11. A simulated crash leaves at most one bounded atomic output temporary; the next
    invocation removes only the marker-derived orphan and preserves completed or foreign
    output.
12. Report failure and failed delivery cleanup remain settled errors but cannot produce
    output acknowledgement or a completed session.
