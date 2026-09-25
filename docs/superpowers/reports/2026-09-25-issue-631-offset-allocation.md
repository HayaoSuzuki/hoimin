# Issue #631 offset selection evidence and review

## Scope and correspondence

Range selection now skips borrowed candidates in complete strict/diverse order, then clones returned IDs into a vector reserved for the clipped page. Diverse still groups references by file within each contiguous score tier and rotates only nonempty groups. Its queue allocation and prefix traversal remain; this change eliminates discarded owned IDs and retained prefix capacity.

The Lean model promotes the six-candidate audit from `/tmp/hoimin-round13-paging-oracle`: asymmetric two-file/two-score-tier fixture, strict/diverse × offsets 0 through 6 × counts 1/2/4/8, plus zero count and two 64-bit maximum boundaries. All 59 cases use strict implementation correspondence. The adapter generates a real public plan from unchanged `a.py` (`True/False/+`) and `b.py` (`True/+/+`), saves it unchanged, maps opaque IDs by source path/line, and invokes public verify dry-run. It checks selected order, retained count, requested count, selection numbering, original ranks, precise zero-count/out-of-range errors, and unchanged manifest bytes. On 32-bit hosts only the two explicit 64-bit controls are skipped; native-width maximum cases remain in Rust unit tests.

`slice_prefix` proves slicing equivalence for arbitrary lists/Nats inside Lean. Three finite broken controls reject offset before diversity, take before skip, and ignored score tiers. The fixed pages reconstruct the diverse order. These claims do not prove Rust equivalence for arbitrary inputs, stable hashing, memory allocation, or whole-CLI performance. No mutation subprocess runs during the public dry-run adapter. Concurrency, filesystem failure, and arbitrary ranking operators are outside this read-only selection model.

## Three implementation self-review passes

1. **Ownership/order:** traced all `id.clone()` sites and active queue transitions. Clones occur only after skip/take; groups contain borrowed candidates. Creating groups while iterating a tier preserves first-file appearance rather than HashMap iteration order. Removing an exhausted group preserves the earlier #433 behavior. No change needed after this trace.
2. **Bounds/lifecycle:** checked empty input, offset at and past end, and maximum count/offset. The early guard makes subtraction safe and page-sized allocation avoids saturating-add pitfalls. Found the former prefix helper had become production-dead; restricted its existing compatibility wrapper to `cfg(test)`, keeping test code out of release builds. No new public surface.
3. **Integration/maintenance:** traced `prepare_verify_selection` to unchanged range selection and compared original/new tier entry/exit conditions. New tier creation is lazy and cannot skip a nonempty group. CI runs the paging model before its executable and adds guarded freshness/sensitivity checks. Scope remains Rust selection plus durable verification; no Python source or dependency changes.

## Three test self-review passes

1. **Regression sensitivity:** baseline eight selection tests passed. Added allocator test before implementation and observed real RED for both policies. A capacity-only guard would miss shrink-after-clone; the test therefore bounds peak requested heap and retains an eager-prefix-then-shrink broken control. Fixture creation and expected-ID calculations happen outside measurement. Separate binary prevents unrelated tests from sharing the allocator interval.
2. **Boundary/independence:** literal strict/diverse orders cover tier-crossing pages, round-robin continuation, out-of-range offsets, and integer maxima. Existing dense-file/singleton tests remain. Added exact returned capacity checks to the boundary matrix, including empty pages, and checked allocator expected positions independently for eight equal files. The public adapter consumes Lean IDs without reproducing the ordering algorithm.
3. **Schema/public behavior:** reviewed all 59 matrix coordinates and corpus rejection tests (missing/duplicate cases, unknown mode/policy/schema, invalid IDs/order/exit). Rejection cases require their specific diagnostics so arbitrary exit 2 cannot pass. Rank and selection-order checks cover projection preservation. Clippy identified an overlong adapter function; split fixture creation and result assertions into helpers without weakening assertions.

## Verification ledger

- Design and plan each received three documented self-review passes, committed before code as `2c0e3f6`.
- Baseline: `cargo test -p hoimin-cli --lib plan::selection_tests`: 8 passed.
- RED allocator evidence: strict offset 512 peak 2,113,560 bytes/capacity 513; offset 1023 peak 4,218,880/capacity 1024. Diverse corresponding peaks 2,122,416 and 4,220,568, capacities 513 and 1024. Failure was the intended ownership/capacity regression, with selected IDs correct.
- GREEN: 9 selection unit tests passed; allocator guard and eager-prefix sensitivity control passed; public adapter passed all 59 cases, with corpus schema validation passing.
- Guarded Lean model build: 4.443 seconds, peak 680,816 KiB; corpus generation 2.809 seconds, 683,120 KiB; freshness 3.042 seconds, 683,856 KiB; sensitivity 0.578 seconds, 613,184 KiB; stats 0.582 seconds, 626,208 KiB. Limits: 20 seconds, 2048 MiB, 10000 heartbeats per theorem, one process at a time. No bound increase or abandoned larger search.
- Infrastructure: initial sandboxed resource guard exited 126 (`monitor_error`) before compilation because process monitoring was restricted; the same guarded command succeeded with approved process access. This was not a model mismatch. Clippy's initial long-function finding was repaired by extracting test helpers.

Final workspace checks and observed post-change allocation values are recorded below after execution.

## Reproduction

From the worktree root, reuse the existing project Python environment and run:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib plan::selection_tests
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test selection_heap -- --nocapture
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_paging_oracle
cargo fmt --all -- --check
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets -- -D warnings
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --workspace
```

From `formal/HoiminOracle`, run each command sequentially:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/paging-build.json -- lake build +HoiminOracle.PagingModel:o
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/paging-generate.json -- lake env lean -j1 -DElab.async=false --run PagingAuditMain.lean --output corpus/paging.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/paging-check.json -- lake env lean -j1 -DElab.async=false --run PagingAuditMain.lean --check corpus/paging.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/paging-sensitivity.json -- lake env lean -j1 -DElab.async=false --run PagingAuditMain.lean --sensitivity
```

CI additionally builds the registered `generate_paging` executable and uses its existing guarded `--check`/`--sensitivity` convention. Corpus expectations are generated only by Lean.
