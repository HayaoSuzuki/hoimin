# Issue #124: Original Verification Performance

## Outcome

`WorkerWorkspace::apply_mutation` and `WorkerWorkspace::reset` no longer rebuild and hash the complete original-workspace manifest for every mutant. A mutation/reset cycle now performs zero original-tree walks and hashes zero original bytes.

Original-source integrity remains enforced by `WorkspacePlan::verify_originals` at the explicit state-machine `VerifyOriginals` checkpoint before the final report.

## Change

- Removed the worker-level `verify_originals` implementation and its calls from mutation application and reset.
- Removed the now-unused `original_root` and `CopyOptions` fields from `WorkerWorkspace`.
- Kept mutation safety checks against the immutable preflight manifest:
  - candidate file hash;
  - actual worker target hash;
  - byte span bounds;
  - exact original bytes.
- Kept reset isolated to the private disk snapshot created during preflight.
- Updated handler and recovery tests to distinguish snapshot reset from the explicit original-integrity checkpoint.
- Added a deterministic regression probe and an ignored release benchmark for repeatable measurements.

## Correctness Boundary

The old per-mutant scans detected any original file change immediately, but the worker never reads the original tree during mutation or reset. It applies mutations to its capability-bound worker root and restores from its process-private snapshot. Scanning the original tree twice per cycle therefore did not protect either operation from stale worker bytes; the existing worker hash/span checks and snapshot checks do that directly.

The state machine already requests a full `VerifyOriginals` operation at `IntegrityCheckpoint::PreFinalReport`. An external source change still produces the typed `OriginalChanged` failure before a successful final report. Tests cover both sides of this boundary:

- reset succeeds from the private snapshot after the original changes;
- the worker remains usable after that reset;
- the explicit pre-final checkpoint reports `OriginalChanged` and preserves its effect ID.

No mtime/size shortcut was introduced, so same-size changes and timestamp resolution do not weaken integrity detection.

## Benchmark

Command, run from both the baseline implementation at `e0dda4c` and the optimized worktree in a release build:

```console
cargo test --release -p hoimin-cli benchmark_original_manifest_work_per_mutant_cycle -- --ignored --nocapture
```

For the baseline run, the test-only benchmark harness added later in `d246d81` was applied as an uncommitted patch on a temporary detached worktree at `e0dda4c`; none of the production changes from later commits were present. This gives both revisions the identical fixture, loop, counters, and timer. The temporary worktree was removed after measurement.

Fixture and workload:

- selected original bytes: 8,388,617 (8 MiB padding plus a 9-byte mutation target);
- cycles: 10;
- each cycle: one `apply_mutation`, then one `reset`;
- elapsed time covers only the ten cycles, after preflight and worker creation;
- filesystem cache state was not controlled, so elapsed time is supporting evidence rather than a pass/fail threshold.

| Metric | Baseline | Optimized | Reduction |
|---|---:|---:|---:|
| Original manifest builds | 20 | 0 | 100% |
| Original bytes read and hashed | 167,772,340 | 0 | 100% |
| Elapsed time | 128 ms | 39 ms | 69.5% |

The committed non-ignored regression test uses a minimal 9-byte target and asserts `(manifest_builds, manifest_bytes) == (0, 0)` for one mutation/reset cycle. Before the implementation it failed with `(2, 18)`.

## Scope Boundary

This change intentionally does not optimize reset's worker/snapshot scans (#125), synchronous async-dispatch I/O (#126), candidate replay (#127), worker materialization (#128), or line/column indexing (#129). Those remain separately measurable Performance Work items.

## Verification

Executed on 2026-08-05:

```console
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

The worktree used a local symlink to the repository's controlled `.venv` so the CLI plan tests could resolve their Python interpreter. The symlink was not committed.
