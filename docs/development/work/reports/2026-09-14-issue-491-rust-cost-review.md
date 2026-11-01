# Issue #491 Rust cost adapter and workspace preflight peak review

## Design reviews

1. Matched existing counters to actual operations: binding tree activation updates and binary-search comparisons; sorting/allocation are excluded from this cost vector.
2. Found that annotation record-site counters miss relocated state clones and collection caller counters miss relocated builders. Chose actual `KnownImports::clone` and successful replacement construction as observation boundaries, with record/callback deltas excluding legitimate flow clones.
3. Separated allocator preflight peak from retained worker heap and sampled RSS. Fixture construction must finish before resetting peak; zero workers is an error control, and positive workers affect aggregate bytes without requiring worker creation during preflight measurement.

## Plan reviews

1. Add a failing observation test before implementing clone instrumentation; consume generated Lean expectations without recomputing bounds in Rust.
2. Exercise real deliberately broken scans, clones and eager builders with the same cost predicates while checking unchanged semantic results.
3. Bound corpus conversions, powers-of-two and arithmetic; isolate allocator tests in their own integration binary and retain all existing counters/tests.

Implementation and test review results will be recorded after execution.

## Implementation review 1: observation boundaries

The clone test failed before instrumentation: observed `(0, 0)`, expected one clone and three entries. After instrumenting the actual test-build `KnownImports::clone`, it passed. Successful collection helper construction now measures both calls and returned UTF-8 String bytes, including direct calls that bypass candidate collectors. Non-test Clone remains derived.

## Implementation review 2: measured workspace model correction

The initial byte-independent preflight assertion failed: a 1 MiB file produced 1,095,036 peak allocated bytes, versus 111,996 for the 64 KiB baseline. Investigation found the existing `manifest.rs` whole-file `fs::read` during hashing. The gate therefore declares entries plus largest-file bytes, independent of total bytes multiplied by workers. This preserves permitted input-proportional memory and does not claim a streaming hasher. A real eager-read control buffers all files for each worker and must violate the same bound. No workspace implementation change is included.

## Implementation review 3: correspondence and isolation

Reviewed the complete owned diff. The adapter consumes generated bounds directly and reads real counters; it does not recompute optimized cost formulas. Full-scan sensitivity visits each actual event and folds the same binding effects. Clone and eager-replacement sensitivity run in actual analyzer callbacks/collectors, and a drop guard resets thread-local flags even on panic. Candidate vectors are compared exactly between normal and broken executions; operator, original byte span and reparsed replacement syntax are also checked. Corpus schema/enum fields are strict, IDs unique, case count nonzero, input sizes bounded, numeric conversions and allocation-size products checked. Sorting, allocator metadata, whole-control-flow clone totals and wall time remain outside the modeled counter vector.

## Test review 1: failure evidence

The clone instrumentation test was a behavioral RED (`(0,0)` versus `(1,3)`) then GREEN. An intermediate adapter compile failed because `AnalyzerCandidate` is not Serialize; the adapter now compares its existing `Eq` candidate vectors directly. This compile failure is infrastructure evidence, not a Lean mismatch.

## Test review 2: independent controls

The direct builder observation test passed. Existing annotation retention regression passed (1 test), and collection-literal regressions passed (3 tests). Workspace preflight peak passed (1 test), including zero files, zero-length file, zero-worker rejection, and independent N/2N/4N file-count/file-size/worker controls. Observed peaks in bytes: files 111996/117596/123548; largest file 1/2/4 MiB 1095036/2143612/4240764; workers 1/2/4 all 111996. A real eager-read of all files for four workers was rejected by the same memory bound. These are allocator requested-byte peaks during preflight, not retained heap, sampled RSS, or runtime.

## Test review 3: generated corpus and final correspondence

The Lean owner generated and freshness-checked `formal/HoiminOracle/corpus/performance-cost.jsonl`. All four adapter tests passed in both default and `contracts` builds: 56 generated cases, exact inventory of 21 binding / 14 annotation / 21 replacement cases, and all three real broken families detected. The independent literal checks require `list[int]` → `Sequence[int]` and the exact flat collection contents; each candidate's original span and replacement syntax are checked. Boundary inventory includes 0/1/7/8/9/16/32, duplicate/nonmonotone bindings, selected/unselected operators, candidate-zero cases and limit 1. Nested size 0 and 1 deliberately share the depth-1 fixture and are not claimed as different input depths.

The adapter evidence mode is **internal-fixture**: it executes real private production functions and analyzer entry points with test-only counters. It is not a public CLI protocol adapter or a proof of the compiled Rust implementation. Lean build/proof/freshness evidence is owned by the main issue report; this report records the Rust correspondence evidence only.

Scoped `cargo clippy -p hoimin-cli --lib --tests -- -D warnings` passed after extracting candidate validation into a separate helper in response to the function-length lint. Formatting and `git diff --check` passed. No changes outside the three delegated files were made, and no commit was created.

## Reproduction

Use `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0` in the issue worktree.

```sh
cargo test -p hoimin-cli --lib analyzer::rust::performance_cost_tests
cargo test -p hoimin-cli --features contracts --lib analyzer::rust::performance_cost_tests
cargo test -p hoimin-cli --test workspace_preflight_heap -- --nocapture
cargo test -p hoimin-cli --features contracts --test workspace_preflight_heap -- --nocapture
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::annotation_import_snapshots_are_not_retained_per_site -- --exact
cargo test -p hoimin-cli --lib collection_literal
cargo clippy -p hoimin-cli --lib --tests -- -D warnings
```

Workspace preflight peak also passed with `contracts` (1 test, 2.09 seconds), with the same three observed peak series. All listed checks completed. No unbounded benchmark or public-CLI timing/RSS claim is made here.
