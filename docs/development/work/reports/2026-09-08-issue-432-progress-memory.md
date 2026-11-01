# Issue #432: Progress history memory report

`progress` now reads reports sequentially and keeps only the current and immediately preceding decoded report. The shared comparison accumulator serves both the public `compare_reports` wrapper and the CLI. Rendering receives compact input dispositions, comparisons, and cached candidate-set eligibility, preserving delayed output and diagnostic order without retaining decoded mutant histories.

## Regression

The allocation-tracked integration regression creates a valid 2,000-mutant schema-v3 report from the golden document and invokes the real `progress::run` path with sink writers. Before the production change it failed with a two-report peak of 4,681,873 bytes and a sixteen-report peak of 27,932,727 bytes. After the change, the measured peaks were 4,681,823 and 4,684,205 bytes respectively: a 2,382-byte increase, below the 524,288-byte limit.

The public output regression covers a usable report, an unusable gap, a usable candidate-set mismatch, and a final unusable report. It asserts the complete JSON input dispositions and warning order; the human result has no stale comparison fields. A second regression verifies that a malformed final input produces exit code 2, empty stdout, and no earlier warning for both output formats.

## Validation

- `cargo test --offline -p hoimin-cli --test progress_heap -- --test-threads=1`
- `cargo test --offline -p hoimin-cli --test progress -- --test-threads=1` — 62 passed
- `cargo test --offline -p hoimin-cli --test report_heap -- --test-threads=1`
- `cargo test --offline -p hoimin-cli --test lean_progress_decision_oracle -- --test-threads=1` — 2 passed
- `cargo fmt --all --check`
- `cargo clippy --offline -p hoimin-cli --all-targets --all-features -- -D warnings`

## Whole-workspace validation

- `cargo test --offline --workspace --all-features -- --test-threads=1` — 1,558 passed, 0 failed, 12 ignored (67 result groups).
- `cargo +1.88 check --offline --workspace --all-targets --all-features --locked` — passed.

## Process memory experiment

A separate CLI process compared the same valid 50,000-mutant report repeated 2, 4, and 8 times. macOS `wait4` measured peak RSS. The before and after JSON outputs were byte-for-byte identical for all three histories.

| Reports | Before RSS (bytes) | After RSS (bytes) |
| --- | ---: | ---: |
| 2 | 163,315,712 | 170,065,920 |
| 4 | 258,768,896 | 226,328,576 |
| 8 | 467,877,888 | 244,580,352 |

For eight reports, peak RSS decreased by about 48%. Elapsed time was essentially unchanged (11.61 versus 11.55 seconds). RSS includes allocator-retained pages and remains higher than live allocations; the allocation regression above directly establishes that decoded histories are no longer retained. This synthetic workload measures report comparison, not mutation execution.

- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` — passed.
