# Issue #128: Worker Materialization I/O

## Outcome

Worker materialization now reads each file from the process-private disk snapshot once per worker and writes those bytes into the worker. The original workspace is verified once at the state-machine's `PostMaterialization` boundary, after every worker has been created and before the baseline can start.

This removes redundant worker-level original-tree scans and normal-build snapshot hashes. The release benchmark's five-run median fell from 163 ms to 35 ms (a 78.5% reduction, or 4.66x faster) while preserving the same 67,108,936 bytes copied into eight workers.

## Correctness Boundary

The snapshot is process-private and is trusted by normal builds for materialization. Original-source integrity is still checked exactly once for the completed worker-creation wave through the existing typed `VerifyOriginals` effect at `IntegrityCheckpoint::PostMaterialization`. The state machine does not start the baseline until every worker succeeds and that checkpoint succeeds.

Contracts builds retain the stronger per-file snapshot assertion: each snapshot read is checked against the preflight manifest's size and BLAKE3 digest before it is written. Normal builds intentionally perform no BLAKE3 pass over snapshot bytes during materialization.

The change preserves effect IDs and pending-effect validation, typed workspace failures, cancellation and cleanup, capability-relative filesystem operations, permissions, and copy-allowance rollback. Copy allowance continues to count worker bytes written, not source or snapshot integrity reads.

## Benchmark

Both revisions used the identical committed benchmark harness and command:

```console
cargo test --release -p hoimin-cli benchmark_worker_materialization_io -- --ignored --nocapture
```

The fixture has an 8 MiB `padding.bin` file plus the 9-byte `target.py` (`fixture_bytes=8,388,617`). It materializes eight workers, then runs one final original verification. The timer starts after preflight and covers only worker creation plus that final verification. Each revision was measured five times under the same host conditions; elapsed time is supporting evidence, not a CI threshold.

The baseline is the Task 2 harness commit `7d50e71`; the optimized revision is `156b735`. Because the instrumentation and benchmark were committed before the production optimization, both revisions use the same fixture, counters, loop, and timer.

| Run | Baseline elapsed | Optimized elapsed |
| ---: | ---: | ---: |
| 1 | 166 ms | 31 ms |
| 2 | 163 ms | 39 ms |
| 3 | 161 ms | 33 ms |
| 4 | 168 ms | 61 ms |
| 5 | 161 ms | 35 ms |
| Median | 163 ms | 35 ms |

| Deterministic metric (8 workers) | Baseline | Optimized | Change |
| --- | ---: | ---: | ---: |
| Original manifest builds | 17 | 1 | -94.1% |
| Original bytes read | 209,715,425 | 8,388,617 | -96.0% |
| Original bytes hashed | 209,715,425 | 8,388,617 | -96.0% |
| Snapshot bytes read | 67,108,936 | 67,108,936 | unchanged |
| Snapshot bytes hashed (normal build) | 67,108,936 | 0 | -100% |
| Bytes copied into workers | 67,108,936 | 67,108,936 | unchanged |
| Median elapsed | 163 ms | 35 ms | -78.5% |

Before this work, the current-main materialization path performed three original full-data passes plus one snapshot read/hash per worker, in addition to preflight and the final verification; this exceeds Issue #128's original estimate. The deterministic counts make that complete work visible: baseline original bytes and hashes are 25 fixture-sized passes, and snapshot reads/hashes are eight fixture-sized passes. The optimized normal path has one original manifest/read/hash pass, eight snapshot reads, no snapshot hashing, and eight worker copies.

## Scope

This is limited to Issue #128 worker materialization. It does not optimize preflight, change scheduler concurrency or CLI behavior, alter candidate replay, or implement Issue #129 line indexing.

## Verification

Executed fresh on 2026-08-05 against the final implementation through `5f17507`:

```console
cargo fmt --check
cargo test -p hoimin-core --test machine
cargo test -p hoimin-cli materialization -- --nocapture
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

Every command exited 0. The focused core suite passed 66 tests. The focused materialization command passed the deterministic I/O test and the handler-level post-materialization original-change test; its manual benchmark remained ignored. The normal workspace run passed 752 tests with 6 ignored, and the all-features workspace run passed 743 tests with 6 ignored. Clippy completed with warnings denied, and the branch diff check reported no whitespace errors.

The worktree-local `.venv` symlink used by tests remained untracked and was not committed.
