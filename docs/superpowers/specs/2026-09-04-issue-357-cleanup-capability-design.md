# Issue 357 Capability-Bound Worker Cleanup Design

## Goal

Prevent worker cleanup from changing permissions outside the owned temporary
workspace when a same-user process replaces a path with a symlink between
inspection and permission repair.

## Scope

This change covers cleanup of ordinary `tempfile`-backed workers:

- permission repair for the temporary wrapper and worker root;
- bounded traversal and removal of worker entries;
- read-only files, read-only directories, non-UTF-8 names, links, and special
  Unix entries;
- retry and `Drop` behavior after a cleanup error;
- replacement races at a worker entry and at the wrapper permission boundary.

Managed workers remain on `ManagedChild::cleanup`, whose owned parent and child
capabilities already drive identity-checked removal. Snapshot directories are
not exposed to mutant processes and keep their current `TempDir` lifetime.

The security claim is permission-target confinement, not successful deletion
against a process that continuously creates or renames entries. Cleanup may
return an error and be retried when a binding changes during an operation.

## Confirmed behavior on main

`WorkerWorkspace::try_cleanup` currently closes `WorkerRoot` before ordinary
temporary cleanup. It then walks ambient paths with this sequence:

```text
symlink_metadata(path)
  -> decide which permission bits are missing
  -> set_permissions(path, stale_permissions)
```

On Unix, a deterministic test pause after `symlink_metadata` can replace a
regular read-only file with a symlink to a read-only file outside the worker.
The subsequent `set_permissions` follows the new symlink and adds owner-write
permission to the outside file. Windows has the same stale-binding shape when
the read-only attribute is cleared by path.

The wrapper uses the same ambient helper. Its path therefore has the same
inspection/effect split even though it is outside `WorkerRoot`.

## Root cause

The cleanup code discards both capabilities it needs before performing
permission effects:

1. `WorkerRoot` owns an open handle to the worker root but is closed first.
2. The outer temporary wrapper is represented only by `TempDir`'s ambient
   pathname; no cleanup handle is retained.

Checking `symlink_metadata` does not make a later pathname effect refer to the
same filesystem object. Skipping entries that were links at inspection time
therefore does not protect against a replacement after that inspection.

## Contract decision

Every cleanup permission change must satisfy one of these rules:

- it operates on a handle captured before the adversarial mutation window; or
- it resolves relative to an owned directory capability, cannot escape that
  capability, and revalidates the entry before opening or descending.

Ordinary worker cleanup will retain the wrapper handle before a mutant can run.
It will keep `WorkerRoot` open while draining worker contents through the
existing no-follow removal primitives. Only after the worker content is gone or
prepared for final removal will it close the root and wrapper handles and call
`std::fs::remove_dir_all` for the final wrapper removal.

`remove_dir_all` is not used for permission repair. On Linux, macOS, and
Windows, Rust's supported implementation opens directories without following
symlinks and removes descendants relative to those handles. Its documented
exceptions (Miri, QNX, Redox, and VxWorks) are outside hoimin's supported
execution targets.

## Approaches considered

### 1. Drain the worker through `WorkerRoot`, then remove the wrapper

Add a cleanup operation to `WorkerRoot` that makes the already-open root
directory accessible and removes each child through the existing
handle-relative, no-follow removal state machine. Retain a wrapper directory
handle from materialization and repair wrapper permissions through that handle.
Close both handles only for final `remove_dir_all`.

This approach removes the ambient chmod surface and reuses the same removal
logic as reset. It also narrows the final ambient operation to deletion, for
which supported Rust targets provide symlink-race protection.

### 2. Enumerate paths, repair every entry by handle, then remove the full tree

This follows the issue's literal suggestion: call `entries`, reopen every entry,
repair permissions, and then use `remove_dir_all`. It performs two full walks
and retains a gap between preparation and deletion. More importantly,
`entries()` must open a directory before a later pass can repair it, so a mode
`0000` directory regresses unless the collection algorithm gains a separate
top-down repair mode.

### 3. Rely only on `std::fs::remove_dir_all`

This removes the vulnerable chmod calls, and read-only Unix files do not block
unlink. It does not preserve cleanup of an inaccessible wrapper or nested
directory, and it would discard the stronger capability already held by
`WorkerRoot`.

### 4. Route ordinary workers through managed-root cleanup

The managed cleanup protocol has parent capabilities, identity records,
budgets, leases, and janitor semantics. Applying that protocol to ephemeral
workers would couple ordinary execution to cross-process recovery machinery
that this issue does not require.

Approach 1 is selected.

## Components

### Retained temporary-wrapper capability

`OwnedWorkspaceDirectory` will expose a crate-private operation that captures
the temporary wrapper as an open directory handle. `WorkspacePlan` calls it
immediately after `TempDir` creation and before it creates or copies worker
contents. Managed owners return no additional handle.

`WorkerWorkspace` stores the optional handle. Construction receives it from
materialization rather than opening the wrapper after copying. This ordering
keeps the capability acquisition outside the period in which a mutant process
can know and mutate the worker path.

The handle is dropped before `TempDir` attempts final removal. This ordering is
required on Windows, where an open directory handle can deny deletion. If
materialization fails, local declaration order likewise drops the handle before
the pending `TempDir` owner.

### Capability-bound worker drain

`WorkerRoot::clear_for_cleanup` will:

1. add the required access bits to the root through its existing open handle;
2. enumerate direct children relative to that handle;
3. remove links/reparse points without following them;
4. open regular files and directories with no-follow semantics before any
   permission change;
5. remove directories with the existing bounded, iterative post-order state
   machine;
6. retain the shared depth limit of 128.

Directory permission repair moves before child enumeration. A directory with
mode `0500` otherwise opens successfully but rejects unlinking its children.
For a mode `0000` Unix directory that cannot initially be opened, cleanup uses
the capability-confined permission operation relative to the already-open
parent. It then repeats no-follow metadata/type/identity validation before
opening the directory. On non-Linux Unix, a no-follow `fchmodat` fallback is
permitted only for the inaccessible-directory case; Linux keeps the
cap-primitives FD-bound implementation and fails safely when its required
kernel/procfs support is unavailable.

Unix file permission bits are not needed for unlink, but the existing remover
may open a regular file and apply its change through that file handle. Windows
continues to clear a read-only attribute on an opened file handle before
deletion. Neither path performs a pathname chmod after a no-follow check.

### Final wrapper removal and retries

For an ordinary temporary worker, `try_cleanup` follows this state transition:

```text
capabilities retained
  -> clear worker contents through WorkerRoot
  -> repair wrapper through retained handle
  -> close WorkerRoot and wrapper handles
  -> final remove_dir_all
  -> cleanup_complete
```

If clearing or wrapper repair fails, both handles remain available for a retry.
Once they are closed, no further permission repair is needed; a failed final
removal is retried directly. The presence of the optional wrapper handle is the
retry phase marker, avoiding a second boolean that could drift from handle
state.

Managed cleanup keeps its current ordering: close `WorkerRoot`, then invoke the
managed child's cleanup capability. `Drop` no longer closes an ordinary root
before calling `try_cleanup`; it lets `try_cleanup` use the capability first and
closes any remaining handles before fields are dropped.

### Errors

No public error code or schema changes. Capability-open, traversal, permission,
and removal failures remain `workspace.io`; the shared depth error remains
`workspace.path.depth`. Final temporary-owner failures continue to be mapped to
the effect-level operation `remove worker workspace`.

A replacement observed after metadata inspection may return `InvalidPath` or
an I/O error rather than completing cleanup. That fail-closed result is
intentional: retrying is safer than applying stale permissions to a new target.

## Testing

### Deterministic filesystem regressions

A test-only pause is placed after no-follow metadata inspection and before the
next cleanup effect. The regression test:

1. creates a read-only worker file and a read-only outside sentinel;
2. starts cleanup and pauses after the worker file is inspected;
3. replaces the worker file with a symlink to the sentinel;
4. resumes cleanup;
5. asserts the sentinel bytes and permission fingerprint are unchanged.

The unmodified ambient walker changes the sentinel permissions, so the test is
a real red/green witness. The fixed code either removes/rejects the replacement
or acts on an already-open worker object.

A second fixture makes the temporary wrapper inaccessible and confirms cleanup
still removes it through the retained wrapper capability. Existing tests for
read-only non-UTF-8 entries and the shared depth error remain part of the
focused suite. A new mode-`0000` nested-directory test prevents the security
fix from weakening cleanup availability.

### Lean audit and implementation oracle

The Lean model splits cleanup into `inspect`, `bind`, `swap`, and `effect`
events. It models two correct binding strategies:

- `retained`: the wrapper capability is captured before the schedule;
- `post_inspection`: an entry is opened no-follow after inspection and either
  binds the owned object or rejects a replacement link.

The model proves that `outsideWritable = false` is preserved for every correct
trace starting from a read-only outside object. A broken ambient transition
resolves the current path at `effect`; bounded search through depth 4 over four
events must find the inspection/swap/effect witness. The bound covers 341 raw
traces per binding strategy and is not presented as a proof; the inductive
theorem is the model proof.

Lean generates a versioned JSONL corpus. The Rust adapter exercises public
`WorkerWorkspace::try_cleanup` for stable and pre-existing-link cases and uses
the owned pause seam for the exact inspection/replacement interleaving. A case
that swaps after handle capture remains `model-only` because production has no
public scheduling control at that point.

The formal audit report will distinguish model facts from filesystem
correspondence. Lean does not prove the Rust implementation or operating-system
syscalls.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Stable read-only worker entry | `post_inspection`, no `swap` | Create worker file and remove owner-write | cleanup result and outside fingerprint | public worker cleanup fixture | `strict` |
| Link exists before inspection | `swap` before `inspect` | Create an outside-pointing symlink before cleanup | cleanup result and outside fingerprint | public worker cleanup fixture | `strict` |
| Entry replaced after inspection | `swap` between `inspect` and `bind` | test pause after no-follow metadata | cleanup result and outside fingerprint | owned filesystem race fixture | `internal-fixture` |
| Wrapper replaced after inspection | `retained`, then `swap` before `effect` | test pause at wrapper repair boundary | outside fingerprint; completeness excluded | owned wrapper capability fixture | `internal-fixture` |
| Entry replaced after handle capture | `swap` between `bind` and `effect` | no public deterministic scheduling control | model state only | Lean case | `model-only` |
| Outside object stays read-only | `outsideWritable = false` | remove owner-write before the case | post-case permission fingerprint | filesystem metadata | mode follows case |

The wrapper replacement row compares only permission-target confinement.
Locating and deleting an owned wrapper after an attacker renames it to an
unknown name is excluded from this issue's claim.

## Mutation testing and resource policy

The dangerous mutation is explicit in Lean's broken transition and in the
filesystem red test: replacing a handle-bound effect with ambient
`set_permissions` changes the outside sentinel. Focused tests also exercise
missing permission bits, links, inaccessible directories, non-UTF-8 entries,
depth limits, retries, and `Drop` ordering.

No cargo-mutants run is planned. It would compile a large Rust workspace for a
predicate already covered by the broken-model sensitivity and same-premise
filesystem witness. If review exposes an untested branch, any mutation run must
be limited to that function, one job, a short timeout, the shared target, and a
dedicated temporary output directory removed immediately afterward.

Lean commands run one at a time through the repository resource guard with a
20-second deadline and 768 MiB RSS limit. Bounded exploration stays at depth 4;
the bound will not be increased without recording the previous state count,
elapsed time, and peak memory.

## CI policy

Automatic CI remains Linux-only. The manual Windows/macOS workflow is not
dispatched during implementation or PR validation and is not used as a merge
condition. Local macOS tests may validate POSIX behavior without invoking a
GitHub-hosted macOS runner.

## Compatibility

- Successful cleanup still removes the temporary wrapper and releases worker
  accounting.
- Managed-root behavior and janitor budgets do not change.
- Non-UTF-8 Unix entries and the 128-level depth contract remain supported.
- Read-only wrapper and nested-directory cleanup remains supported.
- Public errors and serialized schemas do not change.
- Cleanup may now fail closed on a detected replacement instead of modifying
  the replacement target and reporting success.

## Out of scope

- guaranteeing progress against a continuously mutating leftover process;
- discovering an owned wrapper after an attacker renames it to an unknown name;
- changing managed-root ownership, janitor, or time-budget policy;
- extending support to Rust targets where `remove_dir_all` documents no
  symlink-race protection;
- changing process-tree kill completeness on macOS;
- resolving other POSIX issues in the triage queue.

## Self-review record

### Round 1: security boundary and permission availability

The first review traced every permission effect, including the wrapper call
outside the worker-root walk. It rejected a root-only fix because the wrapper
would retain the same stale pathname chmod. It also found that direct reuse of
the current removal state machine would try to unlink children before making a
`0500` directory writable and could not open a `0000` directory. The design now
repairs opened directories before enumeration and defines a confined fallback
plus identity revalidation for the inaccessible Unix case.

### Round 2: lifecycle, retries, and Windows handle ordering

The second review followed successful cleanup, traversal failure, wrapper
repair failure, final-removal failure, explicit retry, and `Drop`. It found that
the current `Drop` closes `WorkerRoot` before ordinary cleanup and that a local
wrapper handle could outlive `TempDir` on an error path. The design now lets
`try_cleanup` consume capabilities first, uses wrapper-handle presence as the
retry phase, and requires every error path to close the handle before the
temporary owner is dropped.

### Round 3: evidence, formal boundary, and operational cost

The third review separated three claims: the Lean invariant inside the abstract
model, same-premise filesystem observations, and supported Rust
`remove_dir_all` behavior. It added a deterministic red witness at the exact
inspection/effect boundary, a public pre-existing-link case, and an explicit
`model-only` post-capture case. It also bounded Lean exploration, rejected a
workspace-wide mutation run, and recorded that no manual Windows/macOS Action
will be dispatched or used for gating.

### Round 4: deletion completeness versus permission confinement

The fourth review considered an attacker renaming the wrapper itself. A retained
handle prevents chmod escape but cannot discover the inode's new unknown name;
claiming complete cleanup for that schedule would overstate the fix. The design
therefore limits wrapper-race correspondence to the outside-permission
observation and lists renamed-wrapper discovery separately as out of scope.
