# Disk-safe mutation execution design

**Status:** Approved by the user on 2026-08-27

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
| Retained focused command log bytes | `16MiB` per command |

Users may change the byte limits or job count with explicit options. A run may
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

## Terminology

**Owned root** is a directory that Hoimin created below its managed temporary
root and protected with a valid live lease.

**Owned bytes** is the conservative logical size of regular files below all
owned roots in a run. The focused workflow also counts its output directory.

**Free-space reserve** is the minimum filesystem capacity that Hoimin leaves
unused for the operating system and other applications.

**Portable guard** combines owned-byte measurement with filesystem free-space
measurement. It runs on macOS, Linux, and Windows.

**Hard backstop** is a host capability that rejects a write independently of
the portable sampler. The report must name the exact capability. A Unix
per-file `RLIMIT_FSIZE` is a per-file backstop, not an aggregate quota.

**Lease** is a marker plus an operating-system file lock held for the lifetime
of an owned root.

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

A new `workspace::disk` module owns these responsibilities:

- create and validate the managed temporary root;
- issue a locked lease for each worker root;
- register and unregister worker roots with one run-wide monitor;
- measure aggregate owned bytes without following symlinks;
- measure free bytes for every distinct filesystem that holds an owned root;
- publish the first terminal disk observation through an idempotent stop
  signal;
- record peak usage, minimum free space, measurement latency, and cleanup
  results.

The shell run loop treats the disk stop signal like a run-wide infrastructure
stop. It stops new effect dispatch, drains processes through the existing
termination and output paths, invokes workspace cleanup, and then reports the
typed failure.

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

The wrapper creates one leased run root below a managed temporary root. It sets
`TMPDIR`, `TMP`, and `TEMP` to a child of that root for every cargo-mutants
command. This redirects cargo-mutants temporary copies and build trees into a
directory that the wrapper owns. The wrapper passes `--jobs 1` unless the user
chooses another positive value.

Each selected candidate uses a child run directory. After cargo-mutants exits,
the wrapper reads the exact inventory and outcome, copies the bounded diagnostic
slice into the compact run record, and deletes the candidate directory. Later
candidates do not accumulate earlier `mutants.out` trees.

The command runner replaces direct stdout and stderr files with draining
spools. The two spools share one per-command `--max-log-size` allowance and
retain deterministic prefixes and tails within that allowance. The runner
continues draining discarded bytes so a full pipe cannot block the child. The
command record stores total observed bytes, retained bytes, and truncation
status for each stream.

`--keep-scratch` marks the lease as retained after the run. It does not disable
size monitoring, free-space monitoring, process cleanup, or bounded logs. The
report prints a deletion command containing the exact retained path.

## Measurement contract

### Owned bytes

The meter walks each owned root with symlink following disabled.

- It sums regular-file logical lengths with checked arithmetic.
- It counts a hard-linked file once per run when the platform exposes a stable
  file identity. Without one, it counts each directory entry and records the
  conservative fallback.
- It treats a vanished entry as a concurrent deletion and continues.
- It returns a typed measurement failure for permission errors or integer
  overflow.
- It counts the focused workflow output directory because retained reports and
  logs consume disk too.

Logical lengths overcount sparse files and copy-on-write extents. The false
positive preserves disk safety. Filesystem free space remains the independent
physical-capacity signal.

The monitor runs one scan at a time. It waits `250ms` between scan starts after
accounting for scan duration. It records sampling delay so reports show when a
large tree reduced observation frequency.

### Free space

The monitor groups roots by filesystem identity and queries available bytes for
each group. It performs this query before analyzer, baseline, or mutant work
starts. A filesystem at or below `10GiB` fails preflight without launching a
child.

During execution, an observation at or below the reserve triggers a stop. The
reserve supplies reaction headroom for writes that occur during one sample and
process termination. The portable sampler cannot prevent one child from
writing more than the reserve inside a single observation interval. Reports
state this limit.

An unavailable free-space query or an unidentifiable filesystem fails closed.

### Hard backstops

The portable aggregate guard stays active on every host.

- Unix launchers apply `RLIMIT_FSIZE` when the platform supports it. The limit
  constrains a single file and the report labels it `hard_per_file`.
- A user may place `--scratch-root` on a capacity-limited filesystem. Hoimin
  records the filesystem capacity and free-space observations. It labels an
  aggregate hard backend only when a platform capability probe verifies a
  quota for the owned root. A path choice or user assertion does not prove a
  quota.
- A future aggregate quota backend implements the same capability interface.
  It may not weaken or replace the portable guard.

The report uses `portable_only`, `hard_per_file`, or a named aggregate backend.
It never labels per-file enforcement as an aggregate quota.

## Stop and cleanup state machine

The first terminal disk observation wins:

```text
running
  -> stop_requested(reason, observation)
  -> processes_draining
  -> workspaces_cleaning
  -> evidence_writing
  -> finished(primary_outcome, cleanup_outcome, report_outcome)
```

The transition order has these rules:

1. `stop_requested` prevents new analyzer, baseline, and mutant dispatch.
2. Later disk or process errors do not replace the first disk reason. Reports
   retain them as secondary failures.
3. The runtime terminates and reaps every owned live root before workspace
   removal.
4. The runtime drains bounded output before removing a workspace.
5. The runtime unregisters roots, stops and joins the disk monitor, then issues
   one logical cleanup request for each owned root. A request may contain
   bounded platform retries.
6. Cleanup failure sets `cleanup_outcome=failed`. It does not replace the
   primary run outcome.
7. The runtime writes evidence after cleanup attempts. A report failure does
   not erase an earlier cleanup failure or primary disk reason.

Disk stops use typed infrastructure codes:

| Code | Meaning |
| --- | --- |
| `workspace.size.exceeded` | Aggregate owned bytes reached the configured maximum |
| `filesystem.reserve.reached` | Available bytes reached the configured reserve |
| `disk.measurement.failed` | Hoimin could not establish a safe measurement |
| `workspace.cleanup.failed` | At least one owned root remained after cleanup |

A candidate interrupted by one of these failures remains unverified. Mutation
score accounting excludes it.

## Lease and stale cleanup protocol

Public Hoimin and the focused workflow use separate managed roots below the
current user's temporary directory. The root has user-only permissions or ACLs.
Each implementation scans only its own direct children.

Creation follows this sequence:

1. Create an unadvertised staging directory with `create_new` semantics.
2. Create a regular lease marker containing schema version, run ID, creation
   time, and owner kind.
3. Acquire and hold an exclusive operating-system lock on the marker.
4. Rename the staging directory to the managed active prefix.
5. Create worker or cargo-mutants children below it.

A crash can leave a staging directory before the rename. The janitor removes a
staging entry only when it is a direct, non-symlink child with the staging
prefix, is older than 24 hours, and contains no entry other than an optional
valid lease marker. It ignores a staging entry with user payload or an unknown
entry.

The janitor accepts a deletion candidate only when all checks pass:

- the entry is a direct child of the canonical managed root;
- the child name matches the managed prefix;
- the child and marker are real directories/files rather than symlinks;
- the marker schema and owner kind match the scanning implementation;
- the marker does not request retention;
- the janitor acquires the lease lock without blocking.

The lock closes the PID-reuse race. The operating system releases it after an
uncatchable process death. An active process keeps the lock and the janitor
skips its directory.

`--keep-scratch` creates a separate regular retention marker with `create_new`
semantics while the lease lock remains held. The janitor validates both marker
types and skips a retained root. It does not rewrite the locked lease file.

Normal cleanup closes child processes and their inherited handles, releases the
lease lock, renames the root to the managed deleting prefix, and removes the
tree. Removal treats `NotFound` as success. Permission repair is restricted to
the owned tree. A failed removal leaves the deleting entry and a typed report;
the next invocation retries it.

The janitor ignores unknown entries, invalid markers, symlinks, and retained
leases. It reports them without deleting them.

## Error and report contract

The Rust run report and focused JSON record include:

- configured maximum and reserve;
- start and end free bytes per filesystem;
- peak owned bytes and minimum observed free bytes;
- sample count and maximum measurement duration;
- enforcement capability names and probe evidence;
- the first stop reason and observation;
- secondary process, measurement, cleanup, and report errors;
- every owned root and its cleanup result;
- bytes reclaimed when measurement permits the calculation;
- stale leases reclaimed during startup;
- bounded-log observed and retained byte counts.

Cleanup success requires absence verification after recursive removal. A cleanup
callback that returned without removing the path does not count as success.

The runtime keeps primary-run, cleanup, and report-delivery outcomes as three
orthogonal internal values. A successfully delivered report contains the first two and
all disk observations. If delivery itself fails, that fact cannot be written reliably
to the failed channel; Hoimin exposes it through the typed return error, stderr, and an
incomplete session record. The implementation must not predeclare delivery success in
the report it is still attempting to write.

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

## Lean oracle

The Lean model covers decision and lifecycle semantics. It excludes filesystem
walking, clocks, process APIs, byte parsing, and report presentation.

The finite model contains:

- observations below, at, and above each threshold;
- measurement success and failure;
- zero, one, and two active work items;
- first and secondary stop reasons;
- cleanup success and failure for each owned root;
- report success and failure.

Lean proves these model claims:

- a stop request disables future dispatch;
- the first terminal reason stays unchanged;
- each owned root receives one cleanup request;
- cleanup failure cannot produce `cleanup_outcome=clean`;
- the model cannot reach `finished` before cleanup attempts complete.

Lean generates a versioned corpus. Rust and Python adapters invoke their real
policy entry points for each case. They classify results as `match`, `mismatch`,
or `infrastructure_error`. Deliberately broken policies must fail corpus
comparison. The audit report distinguishes model proofs from implementation
observations.

## Testing

### Rust TDD

Tests inject a disk meter, clock, stop sink, and filesystem fixture. They cover:

- preflight refusal at the free-space boundary;
- aggregate size stops across several workers;
- no dispatch after a stop;
- sticky first failure and retained secondary errors;
- measurement failure;
- process drain before workspace cleanup;
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
- bounded stdout and stderr drain without child blockage;
- cleanup after success, tool failure, timeout, and interruption;
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
scratch before completion and records peak use and reclaimed bytes.

## Documentation

README and `docs/development.md` explain:

- the distinction between copy size, generated workspace size, and free-space
  reserve;
- safe defaults and examples for raising them;
- the sampler's reaction-window limit;
- focused mutation's single-worker default and compact evidence retention;
- cleanup behavior, `--keep-scratch`, stale recovery, and exact manual cleanup;
- hard-backstop capability labels;
- why direct unguarded `cargo mutants --workspace --jobs N` is discouraged.

Repository mutation examples use `tools/focused_mutation.py` or a guard-backed
command. They do not recommend parallel full-workspace mutation as a routine
gate.

## Acceptance criteria

The change is complete when:

1. New CLI and focused-workflow runs use the agreed defaults without opt-in.
2. Deterministic tests show that size, reserve, and measurement failures stop
   dispatch and trigger process drain plus cleanup.
3. Catchable success and failure paths leave no non-retained owned root.
4. A later invocation removes a simulated abandoned valid lease and preserves
   live, retained, malformed, symlink, and foreign entries.
5. Reports distinguish portable aggregate monitoring, per-file hard backstops,
   and caller-provided capacity roots.
6. Disk-stopped candidates remain unverified and do not improve mutation score.
7. Rust and Python match the Lean-generated state-machine corpus.
8. The one-worker real focused mutation evidence stays under the configured
   limits and removes its scratch.
9. All normal, compatibility, contract, platform, and documentation gates pass.
