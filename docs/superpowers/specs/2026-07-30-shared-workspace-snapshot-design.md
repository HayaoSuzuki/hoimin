# Shared Workspace Snapshot Design

## Problem

Every `WorkerWorkspace` currently owns a `BTreeMap` whose `SnapshotFile` values
contain complete file bytes. With `N` workers, the CLI retains approximately
`workspace bytes × N` in heap memory in addition to the on-disk worker copies.
The descendant `--max-memory` budget does not constrain this CLI allocation.

## Decision

Create one immutable disk-backed snapshot per `WorkspacePlan`. The plan copies
and verifies each manifest file into a private temporary directory once.
Workers materialize from that snapshot and share it through `Arc`; their
per-file metadata contains permissions and fingerprints but no file bytes.
Reset reads one pristine file at a time on demand from the shared snapshot.

The snapshot is created during preflight, after the source manifest is built.
Every copied file is checked against the manifest and the complete source is
verified again after snapshot creation. Worker creation continues to verify the
original source before materialization, retaining the existing changed-source
failure semantics. Once a worker exists, restoration bytes are independent of
later source changes.

## Safety and accounting

The private snapshot directory is never exposed to descendants and remains
owned by the plan through an `Arc`. Worker filesystem operations retain their
capability-relative boundary. Existing copy allowance and observed-copy
accounting continue to count each materialized worker exactly as before; the
shared pristine store is one run-scoped internal disk copy rather than another
worker reservation.

Peak transient heap is bounded by one file read per materialization/reset
operation. Retained heap contains only manifest paths, hashes, permissions, and
shared snapshot handles, independent of workspace logical bytes times worker
count.

## Verification

- An allocator-tracking multi-worker test holds several large workers alive and
  bounds retained heap far below the aggregate fixture bytes.
- Reset/recovery tests continue to cover changed originals, permissions,
  deletion, replacement, and cleanup.
- README documents that `--max-memory` applies to descendants and that
  workspace snapshots use shared disk-backed storage.
