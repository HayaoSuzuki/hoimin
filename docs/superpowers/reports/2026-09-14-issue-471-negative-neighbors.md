# Issue #471: Negative neighbor implementation and review

Base commit: 8b33167. Worktree: .worktrees/issue-471. Host execution results will be recorded below as checks run.

## OKF stage: three self-review passes

1. Compared analyzer.md and the original structural operator spec against decimal_literal_value and its callers. Found an explicit historical negative exclusion; retain the historical source and add a scoped extension instead of silently rewriting the old contract.
2. Checked candidate range and interpretation claims against u64 parsing and existing zero/max tests. Found that i64 would remove supported positive inputs; selected signed magnitude bounded by u64::MAX and retained positive zero behavior.
3. Inspected root/design indexes and source-provenance rules. Existing analyzer concept is already reachable; extend it instead of creating a disconnected duplicate. New spec/report need both original-document inventories, hashes and footnotes. Status stays draft; no execution claim at this stage.

## Design stage: three self-review passes

1. Read the proposed value domain against acceptance examples: -1 step must filter the +1 neighbor zero, whereas existing code filtered only subtraction. Specify filtering on both directions.
2. Read expression/span boundaries against AST collectors: matching the entire source text would reject whitespace and operand parentheses. Extract digits from the operand and replace the whole unary range. Add multiline/inner-parenthesis compile cases.
3. Reviewed compatibility and exclusions: ambiguous -0 and unsigned zero could change candidates inadvertently. Explicitly retain unsigned zero's single candidate, define -0's two neighbors, and exclude repeated signs and unary plus. Store/delete and annotation behavior stays in existing collectors.

## Plan stage: three self-review passes

1. Mapped all issue acceptance conditions to tasks. Found compile-only coverage insufficient for detection semantics; added real plan/verify and a unary-sign surviving control.
2. Reviewed numeric expected values independently: u64::MAX + 1 must still be skipped and negative magnitude overflow must not panic. Added endpoint/oversize table rows rather than relying on generic parsing tests.
3. Reviewed execution reproducibility and source inventory lifecycle. A Python dependency cannot silently imply 3.14 validation: require explicit 3.14 for the dedicated check, fail explicitly for missing/wrong runtimes, and compute source hashes after final edits. Keep builds to two jobs.

## Implementation stage: three self-review passes

1. Compared arithmetic to the original unsigned behavior and independent negative endpoint values. i128 safely contains both u64 magnitudes and their one-step neighbors; the range filter preserves the old positive upper bound. Confirmed zero filtering occurs for both directions and that unary-plus/repeated-sign ASTs cannot enter magnitude parsing.
2. Read collectors and mutation ranges alongside parenthesized fixtures. No collector changes were necessary: annotation and Load context checks precede neighbor generation, and the AST range already includes the minus and inner parentheses. The compiler fixture exercises outer parentheses, inner parentheses, an internal newline/comment and a multiline slice bound.
3. Reviewed actual public helper signatures and CLI options while assembling the integration harness. Corrected the helper return type from u8 to i32 and replaced nonexistent --timeout with --baseline-timeout and --mutant-timeout. Kept test-only machinery in the integration test; no Python production files or dependencies changed.

## Test stage: three self-review passes

1. Compared fixture coverage against all index/lower/upper/step requirements. Added explicit -2 lower and upper table rows so both directions are asserted per position, in addition to -1 step's absent zero.
2. Inspected the verify control and report assertions for false positives. The test names the deliberately weak input, asserts baseline Exit(0), exactly one survived mutant, exact killed replacement -2, expected candidate sets and original source bytes. Syntax validity is checked independently with CPython 3.14.
3. Investigated contradictory results after another worktree compiled into a shared Cargo target. An integration binary linked the other branch's analyzer and returned zero negative candidates. Withdrew shared-cache results from final evidence and rebuilt in this worktree's dedicated target with debug info and incremental compilation disabled. New tests fail for a missing/wrong interpreter rather than being silently skipped.

## TDD observations

Before the implementation, the three negative-candidate analyzer assertions failed with empty/missing negative candidates, and existing positive/suppression cases passed. This initial observation used the subsequently-withdrawn shared cache. Dedicated-target red/green sensitivity and final verification are recorded below. Shared-cache compilation/test results are not final branch verification.

## Final verification on the dedicated target

Host: macOS arm64, CPython 3.14.5. Cargo builds used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2` with this worktree's default target directory. The existing project .venv was temporarily linked for older integration suites that require that location. The new suite used `HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/.local/bin/python3.14`.

- Restored the original decimal helper, leaving new tests in place: `cargo test -p hoimin-cli --lib structure_` failed exactly three missing-negative-candidate assertions (9 passed). Restored implementation before every following result.
- `cargo test -p hoimin-cli --lib --test negative_neighbors --test plan --test rust_analyzer --test run_e2e`: library 631 passed/12 ignored; new integration 2 passed; plan 55 passed/1 ignored; real CLI run_e2e 73 passed; rust_analyzer 201 passed/3 ignored. No failures. The initial plan invocation failed because its required worktree .venv was missing; linking the existing environment resolved the setup failure.
- New public-dispatch integration: 21 individual negative-neighbor replacements and original input compiled with CPython 3.14.5; both baseline runs exited zero; unary-sign control survived, and boundary replacement -2 was killed. Source bytes remained unchanged.
- Additional real-binary subprocess check with this worktree's `target/debug/hoimin`: plan/verify for unary_sign produced one survived; structure_index_neighbor produced one killed and one survived; baseline Exit(0), source unchanged and verify exit 1 in both cases. This is separate from the automated run_with_io dispatch test.
- Formatting: workspace and vendored-parser `cargo fmt ... -- --check` passed.
- The first all-target Clippy check found two potentially truncating u64-to-usize casts in the new test's span application. Replaced both with checked conversions; `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed, and the final `cargo test -p hoimin-cli --test negative_neighbors` rerun passed both tests.
- OKF inspection used PyYAML 6.0.3: 19 Markdown pages, 408 source IDs with footnotes, 757 local links, complete spec/report inventories, all pages reachable from the root index, and the three new Issue471 source hashes matched. This is a local structural/reference check, not external-link validation or attested OKF verification.

An independent parent-agent review read the arithmetic, whole-expression spans, suppression and integration assertions and found no blocking issue. It requested precise evidence labels: automated tests call public CLI dispatch and spawn real CPython processes; the additional subprocess check invokes the real hoimin binary.

## Scope and remaining limits

No arbitrary-precision neighbor generation, alternative literal formats, broader annotation behavior, new Lean model, Python production changes or dependency updates. The existing bounded value policy intentionally skips integers beyond the u64 magnitude range. Linux and Windows native execution, full workspace tests, wheel builds/smoke and Lean generators were not run for this localized analyzer change. Existing ignored tests remained ignored. Historical OKF sources retain their recorded revisions; this report's results apply only to this change and host.
