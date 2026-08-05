# Issue #125: Single-Pass Workspace Reset

## Outcome

`WorkerWorkspace::reset` now classifies the worker tree once and compares every expected snapshot file with its worker counterpart once. A reset therefore performs one tree walk, one worker-content pass, and one snapshot-content pass instead of two of each.

The implementation preserves exact byte and permission comparison, removal of unexpected paths and links, restoration of missing or type-changed paths, capability-relative filesystem access, and the existing depth limit. The complete second postcondition pass remains enabled by the `contracts` feature.

## Change

- Precompute all required snapshot directories in a `BTreeSet` instead of searching every snapshot path for every directory.
- Capture the existing regular-file paths during the single worker traversal.
- Read each snapshot file once and compare its bytes and permission fingerprint with the worker file once.
- Reuse the already-read snapshot bytes when restoration is necessary.
- Skip the complete post-reset traversal in normal builds; retain it as a contracts-only invariant check.
- Add deterministic I/O counters, an ignored release benchmark, and regression coverage for same-size changes to arbitrary non-candidate files.

## Correctness Boundary

Tracking only the mutation target as dirty is unsafe because the test subprocess runs inside the worker and can create, modify, delete, or replace any path. Reset must therefore inspect the whole worker tree. The implementation does not trust mtime or size: a same-size arbitrary write is detected by exact bytes, and permission-only changes are also restored.

The worker traversal continues to reject or remove links and reparse points without following them. File access remains relative to the opened worker root, and Unix reads retain the non-blocking behavior needed for hostile file types.

Normal builds return after every expected path has been checked or restored and every unexpected entry has been removed. Contracts builds intentionally repeat a complete snapshot comparison as an independently evaluated postcondition, so their deterministic counters are twice the normal-build values.

## Design Evaluation

An intermediate design retained each snapshot's validated BLAKE3 hash and read snapshot bytes only for dirty files. It reduced snapshot I/O, but hashing the worker pass made the warm-cache workload slower: its five-run median was 46 ms, compared with 38 ms for the baseline. That design was rejected.

The final design instead compares worker and snapshot bytes directly once. This has no hash-collision boundary, preserves exact behavior, and produced a five-run median of 22 ms.

## Benchmark

Command, run in release mode on the baseline harness revision and final revision:

```console
cargo test --release -p hoimin-cli benchmark_workspace_reset_io -- --ignored --nocapture
```

Fixture and workload:

- snapshot bytes: 8,388,617 (8 MiB padding plus a 9-byte target);
- cycles: 10;
- each cycle mutates the target and resets the worker;
- elapsed time covers only the ten reset cycles, after preflight and worker creation;
- each revision was run five times under the same warm-cache conditions;
- elapsed values are supporting evidence rather than a CI threshold.

The benchmark harness and I/O counters were committed in `f53046a`, before the production optimization, so the baseline and optimized revisions use the same code without an uncommitted patch.

| Metric | Baseline | Optimized | Reduction |
|---|---:|---:|---:|
| Worker tree walks | 20 | 10 | 50% |
| Worker bytes read | 167,772,340 | 83,886,170 | 50% |
| Snapshot bytes read | 167,772,340 | 83,886,170 | 50% |
| Median elapsed time | 38 ms | 22 ms | 42.1% |

Baseline elapsed runs were 38, 38, 37, 41, and 43 ms. Optimized elapsed runs were 22, 22, 23, 22, and 22 ms.

The committed non-ignored regression uses a 1 MiB padding file plus the target and asserts exactly one tree walk and one complete read of each side in normal builds. With contracts enabled, it asserts the intentional second verification pass.

## Scope Boundary

This change does not offload synchronous filesystem work from async dispatch (#126), optimize candidate replay (#127), change worker materialization (#128), or add line/column indexing (#129).

## Verification

Executed on 2026-08-05:

```console
cargo fmt --check
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

Focused workspace handler, recovery, root, reset, and contracts-enabled reset suites also passed. The worktree used a local symlink to the repository's controlled `.venv` for tests requiring a Python interpreter; the symlink was not committed.
