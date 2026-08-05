# Issue #128 Worker Materialization I/O Design

## Goal

Reduce worker materialization from repeated full-tree reads and BLAKE3 passes to one snapshot read per worker plus one original-workspace verification for the complete materialization wave. Preserve the rule that the baseline cannot start when the original workspace changes during worker creation.

## Current Behavior

Preflight builds the original manifest, creates and verifies a process-private disk snapshot, and rebuilds the manifest to detect changes during snapshot creation. Each worker then repeats four expensive operations:

1. `verify_originals_for_materialization` reads and hashes every manifest file.
2. That helper calls `verify_originals`, which rebuilds and hashes the complete original manifest.
3. The copy loop reads and hashes every snapshot file before writing it to the worker.
4. The copy loop calls `verify_originals` again after writing every worker file.

For `jobs = N`, materialization therefore performs N snapshot reads and hashes plus 3N original-workspace reads and hashes. The workers are created concurrently after Issue #126, but this still multiplies disk traffic and hashing work by the worker count.

## Chosen Architecture

The run state machine will make original verification an explicit wave boundary:

```text
Preflight
  -> CreateWorker x jobs (concurrent)
  -> VerifyOriginals(PostMaterialization) x 1
  -> RunBaseline
```

Each worker will materialize only from the already-verified disk snapshot. The normal materialization path will read each snapshot file once, charge its copied byte count, write the bytes, and restore permissions. It will not re-read the originals or recompute snapshot BLAKE3 hashes.

After every worker has completed successfully, the state machine will emit exactly one `VerifyOriginals` effect with a new `IntegrityCheckpoint::PostMaterialization` value. Only a matching successful `OriginalsVerified` event may advance the run to the baseline. This detects original changes that occur at any point through the end of the materialization wave.

The explicit checkpoint is preferred over a `WorkspacePlan` once-gate because a once-gate verifies before or during the first worker and can miss changes made after that verification but before the remaining workers finish.

## Component Responsibilities

### `hoimin-core` state machine

- Add `IntegrityCheckpoint::PostMaterialization` without changing existing serialized checkpoint names.
- After the final valid `WorkerCreated` completion, emit one `VerifyOriginals` effect instead of `RunBaseline`.
- Enter a dedicated `RunPhase::MaterializationVerification` phase. The existing pending-effect validation rejects unrelated or duplicate completions, while the phase transition explicitly rejects an `OriginalsVerified` payload whose checkpoint is not `PostMaterialization`.
- Emit `RunBaseline` only after accepting `OriginalsVerified` with the `PostMaterialization` checkpoint, then enter `RunPhase::Baseline`.
- Preserve effect ID allocation, worker completion accounting, cancellation, failure, and cleanup behavior.

### `WorkspacePlan`

- Remove `verify_originals_for_materialization` and the per-worker trailing `verify_originals` call.
- Treat the snapshot as trusted after preflight because it is created from bytes checked against the manifest, is stored in a process-private temporary directory, and is not exposed to descendants.
- Remove normal-path size and BLAKE3 recomputation for snapshot bytes.
- Continue requiring a snapshot metadata entry for every manifest entry.
- Continue reading snapshot files through the existing `DiskSnapshot` API, charging exactly the bytes copied, creating parent directories, writing files, and restoring permissions.
- Preserve rollback of the current worker's charged bytes on every failed materialization path.

### `WorkspaceHandler` and shell

Use the existing owned `VerifyOriginals` blocking task and completion paths. No scheduler exception, new filesystem API, or direct state mutation from a blocking thread is required.

## Safety and Error Handling

If any worker fails, the state machine does not schedule the wave verification or baseline and follows the existing failure and cleanup path. If wave verification detects a changed, added, removed, unreadable, or otherwise invalid original entry, it returns the existing typed workspace failure and baseline does not start.

Snapshot lookup failures, snapshot read failures, worker-root failures, directory creation failures, write failures, permission failures, and copy allowance overflow remain typed workspace errors. Materialization failure releases only the bytes charged by that attempt. Successfully created workers remain owned by the handler until normal failure cleanup.

Normal builds trust the process-private snapshot after preflight. Contracts builds will compare each snapshot file's size and BLAKE3 hash with its manifest entry after reading the bytes and before writing them to a worker. This retains the internal invariant check without adding another snapshot read, while production builds perform no materialization-time snapshot hashing.

Copy allowance continues to account for bytes materialized into workers. Original verification is an integrity operation and does not transiently charge and release worker copy allowance.

## Testing and Performance Evidence

State-machine tests will prove that:

- the final `WorkerCreated` emits exactly one `VerifyOriginals(PostMaterialization)` effect;
- baseline is not emitted before that verification succeeds;
- a matching successful completion emits the baseline effect;
- failed, duplicate, unrelated, or mismatched completions cannot bypass the checkpoint; and
- single-worker and multi-worker runs use the same boundary.

Workspace tests will add deterministic test-only I/O metrics. For `jobs = N`, the materialization wave must observe:

- one original manifest build after all workers complete;
- N complete snapshot byte reads, one per worker;
- zero production snapshot hash bytes; and
- unchanged per-worker copy allowance accounting.

Integration coverage will change an original file while workers are materializing and assert that the post-materialization verification rejects the run before baseline. Existing coverage for missing snapshot entries, partial copies, allowance rollback, cleanup, link handling, permission restoration, and concurrent worker creation must remain green.

An ignored release benchmark will use the same fixture and worker count before and after the change. It will report tree builds, original bytes hashed, snapshot bytes read, snapshot bytes hashed, copied bytes, and elapsed milliseconds. Deterministic work counts are the regression gate; elapsed time is supporting evidence.

Verification will include focused core and workspace tests, all workspace tests, formatting, Clippy with warnings denied, and tests with the `contracts` feature.

## Scope

This change addresses only Issue #128. It does not optimize the three preflight data passes, change snapshot storage or lifetime, alter copy-budget semantics, change worker scheduling concurrency, modify CLI behavior, or implement Issue #129 analyzer line indexing.
