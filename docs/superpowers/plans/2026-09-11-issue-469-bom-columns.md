# Issue 469 implementation plan

Spec: ../specs/2026-09-11-issue-469-bom-column-design.md

Global constraints: independent worktree from origin/main; no merging; three self-reviews per stage. Controller owns docs/knowledge/design/plan/review/publication. Implementation owns Rust source/tests. No cargo-mutants, no source normalization, no unrelated newline fix.

## Plan self-review

1. Dependencies: first demonstrate validator/public discovery mismatch, then centralize the existing analyzer rule; do not change raw bytes to satisfy tests.
2. Coverage: pair valid BOM coordinates with rejection of the old off-by-one coordinate, and cover real plan→verify/run worker execution.
3. Scope: helper owns only column calculation. Caller-specific line indices and error mapping remain intact; all full workspace checks run once after focused tests.

## OKF self-review

1. Read the analyzer concept and prior span-validation audit distinction; this change concerns display columns while raw-byte contracts stay intact.
2. Added the first-file-BOM rule and shared validator responsibility with a concrete source hash, preserving historical sources and newline scope.
3. Checked YAML/reserved files, 16 pages, source-footnote correspondence, local links, reachability, all design sources and the displayed 161-entry count. Final implementation evidence appears below.

## Task 1: Shared BOM-aware source columns with public regressions

Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-469`. Read spec above relative to docs/superpowers/plans. Own `crates/hoimin-core/src/candidate.rs`, analyzer `rust.rs`, relevant core/public CLI/analyzer tests; no controller docs. No subagents.

- [x] Inspect core CandidateValidationContext/validate_candidate_with_context and analyzer LineIndex. Existing analyzer tests at line_index_ignores_only_a_leading_file_bom_in_columns and reports_columns_without_counting_a_leading_file_bom pin the contract and should stay valid.
- [x] Add failing core and public-discovery/CLI regressions first, record red against base, then introduce a shared core helper such as `python_source_column(source: &str, line_start: usize, offset: usize) -> Option<u32>` and use it from both caller paths. `.get(line_start..offset)` gives checked UTF-8 bounds; strip_prefix of one FEFF only for line_start==0; checked code-point count. Document public behavior/invariants. Do not duplicate BOM logic or weaken exact coordinate validation.
- [x] Test correct first-line coordinates accepted, old BOM-counted coordinate rejected, source hash/span/stable ID preserved, byte offset0/afterBOM, plain file, multibyte text, later line and later/in-string FEFF. Include meaningful invalid range/UTF-8-boundary checks if helper is public. Existing size/byte validation must retain its precedence.
- [x] Use public discovery and actual CLI plan→verify/run for BOM-first-line, no-BOM, BOM comment then second line, multibyte prefix and interior FEFF cases. Keep fixtures small and reuse test helpers. Assert actual baseline success, selected boolean mutation execution and expected killed/survived result, plus unchanged original BOM/source bytes and exact manifest hashes/spans. A genuine boolean assertion test should kill the valid mutant, unlike a syntax failure. Do not rely solely on private LineIndex tests.
- [x] No new Lean model; deterministic boundaries plus actual CPython execution suffice. Run focused red/green and full workspace all-feature tests, fmt and all-target/all-feature Clippy with warnings denied. Every cargo uses `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`. Worktree `.venv` symlinks root CPython3.14.7; never commit it. No other Cargo implementer active.
- [x] Perform and record three concrete implementation reviews and three test reviews: shared semantics, byte/UTF-8 boundaries, unchanged ID/hash/worker behavior, negative coordinate rejection, nonvacuous public pipeline and environment limitations.
- [x] Preserve command output in report (raw logs in /private/tmp/issue469-*.log when convenient) and report total workspace pass/ignore counts. Commit only owned source/tests after verification; no push/PR. User authorizes issue branch commits, use escalation for sandboxed Git metadata. Full task report goes at supplied scratch path. Return concise status/commit/test summary/concerns.

If the proposed helper signature has a material compatibility/idiom issue, explain it to controller and propose the smallest alternative. Continue independent checks while resolving substantive ambiguity.

## Implementation and verification record

Implementation commit: `a988c31`. Controller inspected the production/test diff and raw full-workspace log, independently counting 69 result groups, 1,612 passed, 0 failed, and 13 ignored. Core RED was a missing-helper compile error; the public RED reached the actual location-validation failure.



Implemented one public core column helper, `python_source_column`, and routed both strict candidate validation and the Rust analyzer `LineIndex` through it. The helper counts Unicode scalar values from checked UTF-8 byte bounds, removes exactly one U+FEFF only at file byte zero, and leaves all raw source bytes, byte spans, hashes, and stable-ID inputs unchanged.

Owned files:

- `crates/hoimin-core/src/candidate.rs`
- `crates/hoimin-core/tests/candidate_policy.rs`
- `crates/hoimin-cli/src/analyzer/rust.rs`
- `crates/hoimin-cli/tests/plan.rs`

No controller-owned documentation was changed by this task. The existing untracked `.venv` symlink was not staged.

## TDD evidence

- Core red: `/private/tmp/issue469-core-red.log`. The new core contract tests failed to compile because `python_source_column` did not exist.
- Public red against the old production code: `/private/tmp/issue469-public-red.log`. Both regressions reached real public discovery and failed with `candidate line or column does not match its byte span`.
- Core green: `/private/tmp/issue469-core-final2.log` and `/private/tmp/issue469-final-checks.log`: 14 passed, 0 failed.
- Public matrix green: `/private/tmp/issue469-public-matrix.log` and `/private/tmp/issue469-public-final2.log`: the focused matrix passed.

The public matrix covers leading-file BOM, plain source, BOM comment followed by a second-line candidate, a multibyte first-line prefix, and an interior/in-string FEFF. Each row creates a public plan, executes the selected candidate through both CLI `verify` and direct CLI `run`, observes a successful CPython baseline, and observes the genuine `True` to `False` mutant as killed. It compares the complete reported candidate with the planned candidate and checks the saved plan and original source bytes are unchanged.

## Verification

Every Cargo invocation used `CARGO_INCREMENTAL=0` and `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`.

- `cargo fmt --all -- --check`: passed. Log: `/private/tmp/issue469-fmt.log` (also included in the final chained check).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed. Logs: `/private/tmp/issue469-clippy.log`, `/private/tmp/issue469-final-checks.log`.
- `cargo test --workspace --all-features`: passed with **1,612 passed, 0 failed, 13 ignored**. Raw log: `/private/tmp/issue469-workspace-tests.log`.
- Existing analyzer BOM tests remained green, including `line_index_ignores_only_a_leading_file_bom_in_columns` and `reports_columns_without_counting_a_leading_file_bom`.

## Implementation self-reviews

1. Shared semantics: searched the owned production diff for BOM handling. Candidate validation and analyzer position reporting both call `python_source_column`; the analyzer-local `strip_prefix` logic is gone, so there is one rule.
2. Byte and UTF-8 boundaries: confirmed the helper uses `str::get(line_start..offset)` before slicing and checked `u32` conversion. Tests cover offset zero, immediately after BOM, reversed/out-of-range bounds, bounds inside BOM and beta encodings, plain ASCII, multibyte text, a later-line FEFF, an interior FEFF, and two leading FEFF values where exactly one is ignored.
3. Identity and worker behavior: confirmed the change occurs only after the existing schema/path/hash/mutation/span/original/UTF-8 checks. The helper receives immutable text and returns only a column. Public verify/run tests compare the full candidate object and assert unchanged plan/source bytes, covering IDs, hashes, spans, application, reset, and execution behavior.

## Test self-reviews

1. Negative coordinate rejection: the core regression accepts the BOM-adjusted first-line column and rejects the former BOM-counted column with `LocationMismatch`. Existing validation-precedence tests remain in the same suite and pass.
2. Nonvacuous public pipeline: every matrix row runs real CPython once as baseline and once with a selected boolean replacement; baseline exit is exactly zero and the mutant must be `killed`. The planned replacement is the valid boolean expression `False`; `assert enabled is True` makes that behavior change observable. Baseline success alone would not rule out an invalid mutant, so the exact replacement and source comparison are also part of the check.
3. Preservation and environment: matrix expectations use literal line/column/span values and independently hash the original byte slice. Both verify and run reports must contain the exact planned candidate. Tests use the repository-controlled `.venv` interpreter (CPython 3.14.7 in this worktree); no external service or timing assumption is involved.

## Concerns

None found. The helper is public because the analyzer is a separate crate. Its `Option<u32>` result expresses invalid byte ranges and UTF-8 boundaries without weakening the validator's typed `LocationMismatch` mapping.

## PR self-review

1. Scope and compatibility: compared the branch against base `4adf809`; production changes share only display-column computation. Newline scanning, hashes, IDs and byte writes retain their existing behavior, while the former BOM-counted column remains invalid.
2. Evidence: read raw focused and workspace logs, inspected the five-case public pipeline and literal byte/column expectations, and distinguished the compile-only core RED from the semantic public RED. Formatting, Clippy and OKF checks are recorded as executed checks; no new formal or native-IDE validation is claimed.
3. Reviewability: checked the issue link, design/plan/OKF links, source hash and 161-source index, PR template and changed-file scope. The worktree is independent, and the Python environment symlink is excluded from the commit.

## Independent review

Task reviewer `review469_task` accepted shared semantics and the positive pipeline, then requested a public rejection regression for the old BOM-counted column. Test-only commit `e508467` added that regression; scoped re-review found the item addressed with no new breakage. Final branch review is recorded in the issue execution tracker after completion. No deferred findings or substantive design rulings.

## Review fix evidence

Test-only follow-up commit: `e508467`. Full workspace results above precede this added regression; after this change, the new focused test, fmt, and all-target/all-feature Clippy passed. No production change required another full suite run.


Added `cli_verify_rejects_a_bom_counted_column_before_baseline` in `crates/hoimin-cli/tests/plan.rs`. It creates a valid leading-BOM plan through public discovery, confirms the valid column is 10, changes only the serialized column to the former BOM-counted value 11, and invokes the actual CLI `verify` command with the original selected ID. The CLI returns exit 2 with the exact diagnostic `plan.candidate.invalid: candidate line or column does not match its byte span`; stdout stays empty, the baseline marker is absent, and original BOM source bytes remain unchanged.

Focused evidence: `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test -p hoimin-cli --test plan cli_verify_rejects_a_bom_counted_column_before_baseline -- --exact` passed 1/1. Raw log: `/private/tmp/issue469-review1-focused.log`.

Implementation fix self-checks:

1. No production code changed; the existing strict validator already rejects the stale coordinate through `python_source_column` and `LocationMismatch`.
2. The tamper changes only the flattened manifest `column`; ID, hash, span, source, selection, and test command remain those emitted by valid public planning.
3. Rejection happens during verify preparation, before baseline or worker creation, preserving existing validation order and worker behavior.

Test fix self-checks:

1. The test uses actual `run_with_io` CLI parsing and verification, rather than calling the core helper or validator directly.
2. The exact exit code, empty stdout, full typed diagnostic, absent baseline marker, and unchanged source bytes prevent a malformed-manifest or post-baseline failure from satisfying the test.
3. The initial focused run caught an incorrectly nested JSON edit as `plan.manifest.invalid: unknown field candidate`; correcting the edit to the real flattened `column` field then produced the intended deterministic location rejection.
