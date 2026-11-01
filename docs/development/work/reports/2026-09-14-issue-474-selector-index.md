# Issue 474 explicit selector index report

## Scope and evidence

Base revision: `165a2d284a1af92eb02ffd214ba8c0070c2f3808` (`origin/main`). This report records fresh tests and measurements for this branch separately from the 2026-09-11 boundary audit.

## Review record

### OKF

1. Provenance review found the new untracked design source lacked the repository-required SHA-256. The contract and design index now record `0325588c…`.
2. Claim review checked the complexity statement against both ordered maps: construction is O(F log F) and S direct lookups are O(S log F). It does not claim that filesystem discovery changed.
3. Japanese prose review separated the implemented platform rule from native evidence. The text states that macOS results do not establish Windows behavior.

### Design

1. Correctness review found that replacing the primary map would collapse Windows case-equivalent spellings and could change output. The design now keeps a supplemental index.
2. Edge-case review found that a non-Python entry must not occupy an equality key ahead of a Python entry. The index filters before first-entry insertion.
3. Scope review found that filesystem discovery and line normalization are separate costs. The design explicitly excludes issues #453 and #475.

### Implementation plan

1. Coverage review found symbol module resolution was initially implicit. Task 2 now names its index parameter and preserves `.py` precedence.
2. TDD review separated the cost regression from characterization tests that already pass, so the report will not claim false red evidence.
3. Completion review added release metadata and the affected-package gates instead of relying on a timing threshold.

### Implementation

1. Data-model review confirmed that the original `available` map remains responsible for source enumeration and returned target order. Replacing it would have changed case-collision behavior.
2. Collision review confirmed that filtering non-Python entries before `or_insert` prevents a regular-file spelling from masking a case-equivalent Python path on Windows. A Windows-only regression test records this requirement.
3. API and allocation review found no need for a public index type. The supplemental map is local to one resolution, stores one cloned normalized path per Python equality key, and is dropped before return.

### Tests and performance evidence

1. Regression review found that counting calls to `paths_equal` was too implementation-specific: another linear scan could bypass it. The test now counts all equality-key derivations and expects exactly one build plus one lookup derivation per selected file.
2. Coverage review added line, successful symbol, failed symbol, and Windows non-Python collision cases. Replacing direct `get` with a value scan made the line/symbol test fail with 638 derivations instead of 130; restoring direct lookup passed it.
3. Measurement review kept wall time outside the blocking assertion. Five release samples per size produced medians of 2.454 ms, 5.538 ms, and 11.817 ms for 2,000, 4,000, and 8,000 files with the same number of selectors.

### PR

1. Scope review checked that the draft PR contains only issue #474 code, tests, design, plan, report, and OKF references; the local `.venv` links remain untracked and unstaged.
2. Evidence review distinguishes the successful workspace gate from the contracts-feature run, where one unrelated cleanup-contention test failed once and passed on an exact rerun.
3. Dependency review states that #453 may reduce end-to-end discovered-file counts and #475 edits the same resolver. The PR remains based directly on `origin/main` and includes neither issue.

## Verification results

- TDD red: 128 file selectors produced 8,256 linear path comparisons before the index. The extended line/symbol sensitivity check produced 638 key derivations with an injected value scan, against the required 130.
- Focused green: three deterministic lookup-cost tests passed; the release measurement passed with the medians recorded above.
- `cargo test -p hoimin-core`: all unit, integration, oracle, and documentation test targets passed; the release measurement remained ignored in the ordinary run.
- `cargo test -p hoimin-core --features contracts`: passed in the same verification batch.
- `cargo test --workspace`: passed. The CLI library reported 612 passed and 9 ignored; all subsequent integration and documentation targets passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo test -p hoimin-cli --features contracts`: one unrelated cleanup-contention test failed (`rollback_contention_marks_root_for_immediate_janitor_recovery`, expected one reclaimed root and observed one preserved root). Its exact rerun passed. This report does not classify the complete contracts run as passed.
- Rust and vendored parser formatting passed. Vendored parser clippy passed.
- Python unittest discovery passed 66 tests.
- Release wheel build and smoke test passed for `aarch64-apple-darwin`.
- `/private/tmp/hoimin-okf-check.py .` passed YAML, reserved-file, source-footnote, and local-link checks for 16 knowledge pages.
- Environment: rustc 1.98.0 (`88d9e12ae`), macOS Darwin 24.6.0, arm64. Native Windows execution was not run.
