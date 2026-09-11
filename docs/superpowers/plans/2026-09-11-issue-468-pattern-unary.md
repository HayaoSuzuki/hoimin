# Issue 468 implementation plan

Spec: ../specs/2026-09-11-issue-468-pattern-unary-design.md

Global constraints: independent issue worktree from origin/main; no merging, no cargo-mutants; three self-reviews per stage. Controller owns design/plan/OKF and review/publication. Implementation owns Rust source/tests and any concise verification evidence file requested below.

## Plan self-review

1. Dependencies: reproduce before changing facts; unit scope tests and CPython compilation verify different contracts before full workspace validation.
2. Coverage: negative pattern cases are paired with ordinary expression, guard and boolean positives, preventing blanket suppression from passing.
3. Scope: context belongs in AstFacts; no new syntax parser, runtime recompilation gate, operator ID, ranking rule or model is required.

## OKF self-review

1. Consulted analyzer concept and source roles; this is token eligibility in patterns, separate from the previous collection/operator audit results.
2. Added a scoped pattern-sign contract and actual design hash; preserved historical sources and the other operators' stated boundaries.
3. Checked all 16 pages, source/footnote pairs, local links, root reachability, complete design indexing and 161-entry count. Actual CLI outcomes will be added after implementation verification.

Controller grammar check: CPython3.14.7 compiled patterns -1, -1.5, -2j, -1-2j, -1+2j and rejected each leading-plus substitution with SyntaxError. This confirms the grammar premise independently of candidate generation.

## Task 1: Fix pattern unary token eligibility with real syntax regressions

Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-468`. Read the spec above relative to docs/superpowers/plans. Own `crates/hoimin-cli/src/analyzer/rust.rs`, relevant analyzer/unit/public CLI tests, and README only if necessary to clarify operator scope (coordinate with controller). Controller owns all docs/knowledge/design/plan. No subagents.

- [x] Inspect AstFacts visit_expr/visit_pattern and token eligibility. Use the existing visitor save/restore convention to suppress pattern unary sign tokens before token candidate generation; don't merely clear unary_sign_starts and permit binary fallback. Keep ordinary unary signs, complex binary separators and boolean pattern candidates.
- [x] Add regressions first, capture red against base implementation, then implement the smallest idiomatic structural fix. Cover negative int/float/imaginary/complex, nested/grouped/OR/sequence/class/mapping-key patterns, and scope restoration in match subject/guard/body/later code. Avoid accidentally testing unrelated mapping duplicate-key issue485.
- [x] Use CPython3.14 compilation of actual emitted candidates through public plan output; test positive ordinary unary signs and boolean patterns as well as absence of invalid pattern signs. Existing operator_function_contracts.rs has interpreter discovery and plan helpers; reuse local conventions. Don't let zero candidates make compilation checks vacuous.
- [x] Add public CLI plan/run regression for issue's import-only function with case -1: formerly one syntax-error killed mutant, now no invalid unary candidate and zero killed. Use actual CLI/env CARGO_BIN_EXE_hoimin where practical. Include candidate discovery/selector behavior and a valid ordinary unary candidate if needed to exercise verify. Existing saved-plan validation should reject a previously invalid candidate; inspect that path and test if a compact genuine legacy manifest fixture is useful, without overbuilding.
- [x] Run focused tests red/green, then all-feature workspace tests, fmt and all-target/all-feature Clippy. Use `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target` for every cargo command. `.venv` symlinks root CPython3.14.7; never commit the symlink. No other Cargo implementer is active. No new Lean model: CPython grammar/compilation supplies direct oracle, existing corpus adapters run in workspace.
- [x] At least three implementation and three test self-review passes with concrete findings/evidence. Check token eligibility, context restoration, preserved operators/spans/selection; test negatives plus nonvacuous positives and real import-only classification. Explain material limitations and any existing-test flake separately.
- [x] Commit only owned code/test files after verification; no push/PR. User authorizes commits on this issue branch; use escalation if git metadata is sandboxed. Write full report to supplied task report with exact commands, red output, green results, review passes, changed files and concerns. Return brief status, commit, test summary.

If architecture or policy ambiguity arises, send a focused question to controller while continuing independent work. Do not modify documentation owned by controller.

## Verification results

Implementation commit: 616ee76. The focused analyzer regression first failed with 17 sign candidates instead of six semantic sites; it exposed leading pattern signs among valid expression sites. Literal expected columns were corrected against source layout; the later fixture also covers both complex binary separators. Final focused tests passed:

- `pattern_literal_signs_are_not_candidates_and_expression_context_is_restored`
- `planned_pattern_candidates_compile_with_cpython`
- `import_only_match_negative_literal_has_no_unary_mutant_to_kill`

The public-plan fixture requires nonempty candidates, four ordinary unary candidates and two boolean candidates, then compiles every emitted candidate with CPython3.14.7. The actual CLI plan/run fixture has zero retained unary candidates, no mutants and zero killed. These are finite syntax/classification checks, not universal grammar proofs.

`cargo test --workspace --all-features` passed, including CLI library593 passed/9 ignored, operator contracts17 passed, run_e2e60 passed and rust_analyzer179 passed/2 ignored. All other workspace/unit/doc-test groups also passed. All-target/all-feature Clippy with warnings denied, formatting and diff whitespace checks passed. No flakes occurred during this issue's local validation.

Cargo used `CARGO_INCREMENTAL=0` and the shared task target directory `.worktrees/issue-451/target`. No new Lean model or cargo-mutants invocation was used.

## Implementation and test self-reviews

Implementation review 1, token eligibility: followed candidate generation backward from `replacement(text, facts.is_unary_sign(start))` to `operator_token_starts` and `unary_sign_starts`. Confirmed pattern signs are omitted from both sets, preventing binary fallback rather than merely changing their classification.

Implementation review 2, context restoration: compared the new save/restore with the existing analyzer visitor convention. Nested pattern visits restore `true` to their parent; the outer pattern restores `false`. Match subjects, guards, bodies, and later statements are walked outside `walk_pattern`; the unit regression observes valid unary candidates in each of those contexts.

Implementation review 3, preserved behavior: verified `Expr::BinOp` remains independent of pattern context, so both complex separators retain `binary_add_sub` identity and exact spans. Boolean singleton eligibility remains in `visit_pattern`. `Not` and invert recording outside patterns is unchanged. The full suite and Clippy found no collateral behavior or type issues.

Test review 1, negative matrix: the unit source covers negative int, float, imaginary, both complex separators, grouping, OR, sequence, class, and a single mapping key, avoiding duplicate-key behavior from issue 485.

Test review 2, nonvacuous positives and syntax oracle: the public plan test requires candidates before compiling them, pins ordinary unary and boolean counts, and compiles every actual plan candidate with controlled CPython. Removing the pattern guard makes this check encounter the invalid leading-plus pattern candidate; suppressing all signs or booleans fails its positive assertions.

Test review 3, real issue classification and selection: the binary-level plan/run test selects only `unary_sign`, imports the module without calling the function, asserts plan discovery is empty, and asserts the run has no mutants and zero killed. This directly protects the former syntax-error-killed classification.

Saved-plan review: `prepare_verify` calls `validate_requested_candidates`; requested candidate IDs are validated and then rediscovered under the saved operator/profile/selector configuration. A formerly emitted pattern-sign candidate is absent after this fix and is rejected with `candidate is not discoverable under the planned configuration` before baseline execution. The controller agreed an additional legacy manifest fixture would be disproportionate because this path is already covered by plan validation tests and no schema behavior changed.

## PR self-review

1. Scope: compared facts/token eligibility with the issue and design; the fix suppresses registration for pattern unary signs and restores nested context. It does not change operator identity or add a syntax parser.
2. Evidence: distinguished initial count mismatch from corrected fixture columns; public CPython checks have positive retained candidates and the actual CLI reproducer verifies zero false kills. Workspace and lint results are recorded above.
3. Compatibility and documentation: verified requested-candidate rediscovery rather than claiming every old manifest is rejected; updated the design wording and actual OKF hashes. Existing historical audit claims remain distinct. Independent task and branch reviews are recorded before publication.
