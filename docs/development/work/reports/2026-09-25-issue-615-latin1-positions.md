# Issue 615 implementation and validation record

Design/plan committed before implementation as `2dcd24b`, based on `efe3531`. Implementation stores only raw Latin-1 expansion positions and validates Latin-1 locations with raw physical-line starts. No schema, codec, source-size limit, or original-byte identity changes are intended.

## RED and focused GREEN

Baseline core codec/location tests: 24 passed. The isolated public-API allocation test then failed all nine nonzero-density conditions (three sizes, sparse/half/full density). Both the paired decoder coordinate bound and duplicate context-correction bound were exceeded. ASCII-only Latin-1 controls did not fail. Logs: `/tmp/hoimin-batch-604-632/615-red-heap.log`.

After both representation changes, all 12 density/size conditions pass, with context peak no more than 73 bytes above decoder peak. At the largest dense input, decoder peak is 5,242,891 and context peak 5,242,964 bytes, versus 9,437,195 and 13,631,572 before. Public CLI codec tests: six passed, including the new dense-prefix LF/CRLF/CR plan/run/verify loop and exact worker bytes. These peak requested allocation bytes exclude the preconstructed input and allocator metadata.

## Implementation self-reviews

1. Mapping arithmetic and boundary review: derived end is raw_start + entry_index + 2. Every stored entry was produced after appending its scalar to an allocated String, so the derived sum is bounded by that existing String length; public usize::MAX offsets still return None. Reverse comparison remains <= and forward comparison <. Added a logarithmic comparison-count test for the derived-coordinate binary search; no whole-prefix query scans.
2. Location and compatibility review: Latin-1 raw bytes each denote one scalar, including 0x85 (not a Python physical newline). Raw starts use the existing LF/CRLF/CR helper. The private enum avoids changing the public Unicode index or BOM semantics. Both context length checks remain, and raw span checks/encoding/error precedence still precede location validation. No implementation correction was required after this review.
3. Allocation and API review: decoder entries remain usize, preserving its standalone width contract; only context line starts use existing u32 limits. The separate analyzer Unicode index remains transient and is not claimed as eliminated. String growth policy is unchanged. Both reported redundant retained tables are addressed; no public type signature changed.

## Test self-reviews

1. Allocation sensitivity: checking fields or element sizes alone would mirror implementation. The committed test invokes public decoder/context APIs with the existing System tracking allocator and fails the baseline empirically. One test owns each process-global measurement window; source construction, drop assertions, and diagnostics run outside that window. The independent release probe additionally checks retained live bytes and return to baseline after drop.
2. Boundary and identity coverage: exhaustive byte values plus adjacent/sparse expansions test scalar-width mapping and interior rejection independently. Location expectations use the unchanged public decoded Unicode index, including newline combinations and 0x85/0xa0/0xff. Original validation-error and UTF-8/BOM tests remain. Public flow now checks dense comments and same-line non-ASCII before False across all three physical newline forms; raw span/hash, plan/run/verify ID agreement, source restoration, and actual worker bytes are checked.
3. Measurement review: query timing performed during a Cargo build can contain load noise. The initial timing artifact is exploratory only; final before/after query timing must run together after Cargo becomes idle. RSS includes parser/allocator pages and is reported separately from requested allocations. No timing threshold is used as a CI assertion. The unrelated issue 612 Git hunk-coordinate fix is intentionally outside this change.

## Verification and independent review

`cargo test --locked --workspace`: 2,303 passed, zero failed, 22 ignored across 99 result entries. Exact CI `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings` pass. Initial Clippy runs found a test helper four lines over the limit and an unreadable test literal; the worker-byte assertion was extracted and numeric separators added, without production changes. The six public codec tests and isolated allocation test were rerun successfully afterward. Workspace/vendor formatting and diff checks pass.

Root's independent read-only review found no blockers: derived offsets are bounded by the existing decoded string, scalar/interior checks remain, raw Latin-1 columns follow one-byte/one-scalar semantics, decoded u32 limits remain, and the full-byte/newline/EOF differential tests cover the changed paths. No Python production or CI registration files changed; no Python mutation-testing or Lean-regeneration claim is made.

## Release measurements

macOS 15.7.7 arm64, rustc 1.98.1, Python 3.14.7. Both release builds used the same flags (`--offline --locked --release -p hoimin-cli`, one build job, default thin LTO); baseline source was `efe3531`. The issue's generate.py/probe.rs/measure.py were extracted into `/tmp/hoimin-batch-604-632/615-perf`, with paths changed to preserve before/after executables and results. The allocator probe links the production hoimin_core rlib, not copied production code. Each API/CLI combination ran in three separate processes with a 30-second deadline; every allocator result returned live bytes to its pre-construction baseline after drop. Requested allocation counts were identical across repeats.

| Comment characters | Latin-1 context retained before | After | CLI RSS median before | After |
| ---: | ---: | ---: | ---: | ---: |
| 65,536 | 1,704,084 | 655,508 | 9,142,272 | 8,290,304 |
| 262,144 | 6,815,892 | 2,621,588 | 16,351,232 | 12,107,776 |
| 524,256 | 13,631,572 | 5,242,964 | 22,888,448 | 19,431,424 |

At the largest size this is an 8,388,608-byte / 61.5% retained-context reduction. Decoder retained bytes fall from 9,437,188 to 5,242,884. The standalone Unicode index is unchanged at 4,194,320 bytes, as expected; UTF-8 context remains 4,194,384 and ASCII context 80. The unchanged analyzer index and allocator/parser pages are included in RSS, so RSS savings are smaller and are not attributed solely to index bytes. Largest Latin-1 public-plan wall medians were 12.98 ms before and 9.98 ms after; three subprocess samples are too few/noisy to establish general CLI latency improvement.

All 27 measured plans in each phase produce one True→False candidate at line 3, column 8. The complete candidate objects, including raw spans, hashes and IDs, match between phases for all nine codec/size fixtures. This comparison supplements committed plan/run/verify tests.

### Lookup/construction tradeoff

The independent timing probe uses public APIs and unchanged sources, alternating pseudo-random and reverse raw offsets. Each query maps raw→decoded→raw and asserts exact round-trip equality; before/after digests match. Each process runs two warmup rounds and seven measured rounds; three paired process repetitions alternate before/after order. No Cargo build ran in this lane during final timing. Values below are milliseconds, median [min,max] across 21 rounds; construction batches contain ten contexts, query batches 100,000 round-trip pairs. The initial timing during a build is excluded.

| Comment bytes | Non-ASCII stride (0=none) | 10 contexts before | After | 100k pairs before | After |
| ---: | ---: | --- | --- | --- | --- |
| 65,536 | 0 | 1.103 [1.019,1.128] | 0.787 [0.693,1.573] | 0.153 [0.148,0.205] | 0.188 [0.140,0.342] |
| 65,536 | 64 | 1.129 [1.024,1.579] | 0.764 [0.742,0.845] | 1.854 [1.827,2.579] | 2.361 [2.341,2.438] |
| 65,536 | 2 | 1.611 [1.371,1.975] | 0.894 [0.809,0.920] | 6.253 [6.031,7.572] | 5.909 [5.740,6.030] |
| 65,536 | 1 | 2.486 [2.252,2.699] | 1.030 [0.954,1.140] | 7.959 [7.498,8.880] | 7.411 [7.236,8.248] |
| 524,256 | 0 | 9.377 [8.759,9.516] | 6.162 [6.094,6.887] | 0.152 [0.143,0.191] | 0.149 [0.143,0.186] |
| 524,256 | 64 | 9.076 [8.931,9.917] | 6.856 [6.162,7.003] | 2.804 [2.766,2.864] | 3.483 [3.460,3.580] |
| 524,256 | 2 | 15.329 [13.937,17.330] | 7.681 [7.481,8.394] | 7.914 [7.570,10.053] | 10.026 [9.660,10.895] |
| 524,256 | 1 | 30.881 [27.360,33.821] | 11.033 [10.064,11.641] | 10.443 [9.536,13.607] | 10.858 [10.342,13.541] |

Derived-coordinate binary search trades arithmetic/branch behavior for a smaller position table. Some sparse/half-density mapping workloads are approximately 24–27% slower; the largest dense case is approximately 4% slower. Context construction improves for every measured density, and dense retained heap is substantially lower. These results justify the bounded memory improvement without claiming universally faster lookups. A future branchless/rank-index optimization would need separate evidence and is outside this change.

Artifacts retained: `615-red-heap.log`, `615-workspace.log`, `615-clippy-pass.log`, `615-clippy-vendor.log`, `615-final-focused.log`, and `615-perf/{results-before.json,results-after.json,timing-paired.json,probe.rs,timing.rs,measure.py,timing-paired.py}` under `/tmp/hoimin-batch-604-632/`. The committed allocation test is the portable CI regression gate; the report does not depend on wall-clock thresholds.

## Latest-main integration

Rebased the unpublished two-commit branch cleanly onto `bf09c91`; the design/plan remains the first commit (`92e067f`). Main's intervening changes do not modify either core production file measured above. On the rebased tree, full workspace tests passed again: 2,322 passed, 0 failed, 22 ignored across 100 result entries. Both exact CI Clippy gates, workspace/vendor formatting, and diff checks passed again. Evidence: `615-integration-workspace.log`, `615-integration-clippy.log`, and `615-integration-vendor.log`.
