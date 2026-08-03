# Case-colliding snapshot paths

Issue: #147

## Problem

A source tree on a case-sensitive Windows directory can contain two manifest
entries such as `A.py` and `a.py`. The shared snapshot is normally created in a
case-insensitive temporary directory, where writing the second entry replaces
the first one.

## Design

Before writing each snapshot file, check whether its destination already
exists. An exact duplicate cannot be produced by `WorkspaceManifest`, so an
existing destination means that the destination filesystem aliases this
logical path with an earlier manifest entry. Return a typed
`SnapshotPathCollision` error instead of overwriting snapshot data.

This deliberately relies on the destination filesystem rather than
lowercasing paths in Rust. Windows filename equivalence is broader than ASCII
case folding, and the filesystem is the authority that matters for the
snapshot.

## Verification

- A Windows-only regression test builds a synthetic manifest containing
  `TARGET.py` and `target.py` and verifies that snapshot creation fails with
  `workspace.path.collision`.
- Existing workspace and CLI tests continue to pass on all platforms.
