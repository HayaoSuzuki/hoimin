# Issue 475 range normalization report

Base: `165a2d284a1af92eb02ffd214ba8c0070c2f3808`.

## Three-pass review record

### OKF

1. Provenance review found the new design was initially marked only as an untracked source; its exact SHA-256 was added to both consuming pages.
2. Contract review confirmed the Japanese OKF text distinguishes explicit-selector normalization from changed-range and ranking normalization.
3. Catalog review verified the design and audit counts, table rows, footnotes, local links, YAML, and reserved-file rules with the repository validator.

### Design

1. Semantic review found that file/source plus line currently produces a line-restricted target. The final pass preserves this behavior.
2. Boundary review retained `saturating_add(1)` for adjacency at `u32::MAX` and keeps invalid-range rejection in the input loop.
3. Scope review separated explicit resolution from changed-range and ranking normalization.

### Plan

1. Coverage review added symbol normalization because it repeats the same prefix operation.
2. TDD review requires a real call-count failure before moving the calls.
3. Evidence review separates deterministic counts from release elapsed time.

### Implementation

1. Control-flow review confirmed validation and path resolution remain inside their original selector loops, so the first error is unchanged.
2. State review found empty vectors must be skipped: normalization counters and work now occur only for groups that contain selectors.
3. Boundary review confirmed the final `BTreeMap::values_mut` pass does not change target ordering or the existing file-plus-line narrowing behavior.

### Tests and measurement

1. The red run observed 128 line normalizations where one was required and 256 symbol normalizations where one was required; the green run observes one of each for one file.
2. A second-file review added a two-file case and observes exactly two line and two symbol normalizations, closing the per-file acceptance boundary.
3. Release review used five samples at each size and no timing threshold: medians were 1.059833 ms for 2,000, 2.061917 ms for 4,000, and 4.173583 ms for 8,000 sparse ranges.

### PR

1. Scope review confirms the diff contains only issue #475 resolver code, tests, its design/plan/report, and issue-specific OKF/index entries.
2. Claim review describes deterministic operation counts as the regression guarantee and elapsed release measurements as nonblocking evidence.
3. Publication review will verify the pushed head SHA, `main` base, issue closure reference, and rendered PR body after creation.

## Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test -p hoimin-core`: passed.
- `cargo test -p hoimin-core --features contracts`: passed.
- `cargo test --workspace`: passed.
- `cargo test -p hoimin-cli --features contracts`: passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `python -m unittest discover -s tests -p 'test_*.py'`: 66 passed under Python 3.14.7.
- Release wheel build: passed; the wheel's `hoimin --version` smoke check returned `hoimin 0.1.0` under Python 3.14.
- OKF validator: 16 pages passed.
- `cargo fmt`/`cargo clippy` for `vendor/hoimin-python-worker`: not applicable because this checkout has no such Cargo manifest.
