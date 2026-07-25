# Capability-Relative Workspace Operations Design

## Context

Issue #28 addresses audit finding `RUST-AUDIT-004`. Workspace paths are
currently checked for symlinks and then passed to ordinary pathname APIs.
Another actor able to modify the worker tree can replace a checked parent
between validation and use, redirecting reads, writes, removals, restoration,
or permission changes outside the worker root.

The fix must make the worker root a stable filesystem capability and perform
security-sensitive operations relative to open directory handles. Normal
workspace behavior must remain available on Linux, macOS, and Windows. If a
safe capability cannot be established, workspace creation fails closed rather
than falling back to the current check-then-use sequence.

## Decision

Introduce a private `WorkerRoot` abstraction backed by the `cap-primitives`
filesystem APIs and `cap-fs-ext` no-follow open options.

- `WorkerRoot` owns an open handle for the materialized worker root and retains
  its path only for display and process-working-directory compatibility.
- Every untrusted path is normalized as a nonempty relative path before it
  reaches a filesystem operation.
- Parent components are opened one at a time with
  `cap_primitives::fs::open_dir_nofollow`. Each successful open produces the
  stable parent capability used for the next component.
- Final-component operations use only a single filename relative to the
  already-open parent. They reject symlinks and Windows reparse points instead
  of traversing them.
- Reads, writes, removals, existence queries, mutation application, reset
  enumeration, restoration, and permission changes use `WorkerRoot`.
- Failure to open the initial root capability or to perform a no-follow
  operation is an operation error. There is no ambient-path fallback.

This uses a maintained cross-platform implementation while making the stricter
hoimin contract explicit: cap-std's general guarantee that a path remains
beneath a directory is not by itself sufficient, because hoimin rejects
symlinks rather than accepting safe in-root symlinks.

## Capability and Path Contract

`WorkerRoot` exposes a small operation-oriented API rather than returning
absolute paths:

- open a regular file for reading;
- open or create a regular file for replacement;
- create missing parent directories;
- inspect an entry without following it;
- list an open directory;
- remove a file or empty directory;
- change permissions through an opened file or directory handle.

The exact method split may follow call-site needs, but callers cannot recover
ambient authority from the abstraction.

Accepted paths contain only normal relative components. Absolute paths,
prefixes, `.`, `..`, root components, and empty paths are rejected before I/O.
The existing external path-validation behavior is preserved, including its
error category.

Intermediate components must be real directories. The final component must be
the expected kind for the requested operation. Symbolic links, junctions, and
other reparse-point traversal are rejected. A rename of an already-open parent
does not redirect the operation: the operation continues against that opened
directory object or fails.

The stored worker pathname remains available for diagnostics and for spawning
commands whose API requires a pathname. It is not used to authorize or perform
hoimin's own workspace content or permission changes.

## Operation Semantics

### Initialization

Materialization continues to create the temporary worker tree using its
existing trusted setup flow. Immediately afterward, `WorkerRoot` opens the
worker directory without delete sharing where required by Windows. The
capability lives as long as `WorkerWorkspace`.

If initialization cannot obtain the required handle semantics, worker
construction returns an explicit workspace error. It never silently changes
to pathname-based operation.

### Read and Existence

Read opens the final entry without following it, verifies it is a regular file,
and reads from that handle. Existence is derived from no-follow metadata:
missing entries return false, safe entries return true, and invalid paths or
symlinks remain errors where the current public contract distinguishes them.

### Write and Mutation

Missing parent directories are created and reopened as capabilities one
component at a time. The final file is opened without following links.
Existing files are made writable through the opened handle before replacement;
new files are created relative to the stable parent.

Mutation verification and mutation writing use the same opened file handle
where practical. Hash and span checks therefore describe the object that is
modified, rather than an earlier pathname lookup. Truncation and writing occur
only after all mutation preconditions pass.

### Remove

Remove opens the stable parent, inspects the final entry without following it,
adjusts permissions through a handle when needed, and removes the single entry
relative to that parent. A concurrently exchanged parent cannot redirect the
deletion outside the worker capability.

### Reset and Recovery

Reset replaces `ignore::WalkBuilder` over the ambient worker path with
capability-relative directory enumeration. Recursion holds an open directory
capability for each level and never constructs an authoritative absolute path.

Unexpected files and directories are removed relative to their open parents.
Snapshot files are restored through capability-relative creation and handle
writes. Content comparison, hashes, and permission restoration likewise use
opened objects. Ordering remains compatible with the current reset contract:
remove extra descendants, restore expected content, restore permissions, then
verify the snapshot.

## Errors and Recovery

Errors identify the logical relative path and operation (`read`, `write`,
`remove`, `reset`, or `mutation`) without treating an ambient resolved path as
trusted evidence. Symlink/reparse rejection is distinguishable from ordinary
not-found and I/O failures where callers need that distinction.

Partial-operation handling preserves the current cleanup and recovery
contract. A failure may leave the worker needing reset, but it cannot modify
content or permissions outside the opened worker root. Recovery itself uses
the same capability-relative primitives and fails closed if they are
unavailable.

## Testing

### Characterization

Extend `workspace_handler` and `workspace_recovery` tests before changing the
implementation. Cover existing read, write, remove, mutation, reset, nested
directory creation, permission restoration, missing-entry, and invalid-path
behavior so the security refactor does not silently alter valid workflows.

### Deterministic Race Tests

Add a test-only synchronization hook at the boundary after a parent directory
capability has been acquired and before the final operation. Tests replace the
parent pathname with a symlink, junction, or alternate directory while the
operation is paused, then resume it.

For read, write, remove, mutation, and reset, assertions verify:

- the operation affects only the originally opened worker directory or fails;
- an outside sentinel retains its content, existence, and permissions;
- no external file or directory is created;
- the result does not depend on probabilistic retry loops.

The hook is compiled only for the relevant test surface and must not introduce
test-only symbols into normal cross-platform builds.

### Platform Coverage

The same security contract runs on Linux, macOS, and Windows CI. OS-specific
test setup is limited to creating a symlink or Windows directory junction and
checking platform permissions. A platform that cannot initialize the required
capability semantics must have an explicit fail-closed test; it must not skip
to a vulnerable fallback.

### Focused Mutation

Run focused Rust mutation testing against:

- relative-path component rejection;
- no-follow parent traversal and final-entry classification;
- the mutation verification-to-write boundary;
- reset decisions that remove, restore, or change permissions.

The retained focused set must catch viable security predicate mutations.
Unviable and timeout outcomes are recorded separately. Mutation testing is not
expanded to unrelated process, scheduler, or reporting code.

## Compatibility and Scope

- Valid workspace read, write, mutation, removal, and recovery behavior is
  preserved on Linux, macOS, and Windows.
- Manifest, event, result, and report schemas do not change.
- Worker command execution, process containment, original-project scanning,
  and temporary-directory creation are outside this finding unless a change is
  required to construct or retain the root capability.
- No public capability handle is exposed.
- The design and implementation plan are committed on
  `refactor/issue-28-capability-relative-workspace` in the Issue #28 worktree.

## References

- `cap_primitives::fs` provides openat-like, beneath-root filesystem
  operations on Linux, macOS, and Windows.
- `open_dir_nofollow` provides the required no-follow directory-component
  traversal.
- Linux `openat2` documents `RESOLVE_BENEATH` and `RESOLVE_NO_SYMLINKS` as
  pathname-resolution race defenses.
- Windows `NtCreateFile` supports names relative to an open `RootDirectory`
  and `FILE_OPEN_REPARSE_POINT`; the selected library encapsulates those
  platform details.
