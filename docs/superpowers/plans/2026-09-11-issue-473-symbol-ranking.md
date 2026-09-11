# Issue 473 implementation plan

Spec: ../specs/2026-09-11-issue-473-symbol-ranking-design.md

Global constraints: independent worktree from origin/main, no merging; three self-reviews per stage. Controller owns docs/superpowers and docs/knowledge. Implementer owns ranking/plan Rust code/tests and README/docs/development current behavior. No cargo-mutants, no broad analyzer refactor, no historical audit rewriting.

## Plan self-review

1. Dependencies: capture old scores/order first, update membership and ranking version together, then test public plan→verify and old-version rejection. Existing serialized schema3 remains unchanged.
2. Coverage: exact and ancestor matches pair with dot-prefix/cross-file negatives and duplicate selectors. CLI verifies actual selected candidate, not only a sorted in-memory vector.
3. Scope and resources: one standard multi-file task, retain existing HashSet indexing and borrowed substrings. Run focused regressions then a single full suite; no unrelated workspace/session changes.

## OKF self-review

1. Read selection/plan/verify concept, current ranking implementation and documented version mismatch. Corrected preflight assumption from schema2 to actual schema3 before implementation.
2. Add new source with actual hash for descendant ranking and ranking4 compatibility. Mark prior version-discrepancy table as historical, preserving original source revisions and Lean scope.
3. Validate YAML/reserved files, source-footnote pairs, links, reachability, source hashes, full design index and displayed count. Final executed evidence is appended after checks.

## Task 1: Align explicit-symbol ranking with descendant selection

Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-473`, base4adf809. Own `crates/hoimin-cli/src/plan/ranking.rs`, ranking_tests.rs, relevant plan.rs header diagnostic, `crates/hoimin-cli/tests/plan.rs`, README.md and docs/development.md. Controller owns spec/plan/OKF. No subagents.

- [x] Read spec and /private/tmp/issue473-preflight-notes.md. Actual `PLAN_SCHEMA_VERSION` is3; old currentdocs2 are wrong, don'tchangeproduction to2. Capture semantic RED in ranking tests and public plan order before production edit.
- [x] Keep per-file HashSet indexing; match full candidate symbol and borrowed dot-boundary ancestors, no allocation per ancestor or full selector scan. Award one +250 reason once, preserve other reasons, path semantics and order.
- [x] Advance RANKING_RULE_VERSION3→4; schema stays3. Existing header rejection gains regenerate guidance as needed. Current README/development text explains descendant bonus, exact boundary, schema3/ranking4 and old-plan regeneration. Preserve historical audit/spec descriptions.
- [x] Unit matrix: exact, class/method, nestedfunction/nestedclass, BoxOther negative, duplicate parent+child, cross-file/missing symbol. Check literal scores and single reason/order, maintain existing index/selection regressions.
- [x] Public two-file plan example --source . --symbolcalc:Box boolean_literal: calc:Box.check rank1score350, a:other score100. Actual CLI verify --top1 executes the planned class candidate (ID/path/symbol), baseline0, genuine boolean kill. Old ranking3 plan with unchanged schema3 rejected before baseline with regeneration guidance. Keep plan/sourcebytes stable.
- [x] Focused ranking/plan tests and existing Lean candidate-ranking oracle; full `cargo test --workspace --all-features`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`. EveryCargo `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`; no other Cargo implementer. .venv symlinkCPython3.14.7 nevercommitted. No new Lean model for explicit_symbol resolver; report existingoracle's boolean-input boundary accurately.
- [x] Three implementation and three test selfreviews with concrete checks/findings. Report exactcommands/rawlogs `/private/tmp/issue473-*.log`, totalpass/ignoredcounts and limitations. Commit only ownedfiles, no pushPR; report at suppliedscratchpath. No fullsuite repeats aftergreen withoutnewchange/failure.

## Implementation and verification record

Status: DONE

Commit: `1c2554f fix: rank explicitly selected symbol descendants`

### Implemented

- Ranking keeps the per-file `HashMap<&Utf8Path, HashSet<&str>>` index and now checks the candidate's full qualified name followed by borrowed dot-delimited ancestor slices. The explicit-symbol reason is added once on the first match.
- Ranking rule version advanced from 3 to 4. `PLAN_SCHEMA_VERSION` remains 3. Ranking-version rejection now directs users to regenerate the plan.
- README and current development documentation now describe schema 3, ranking rule 4, descendant matching, exact dot boundaries, single bonus behavior, and old-plan regeneration.
- Candidate IDs, spans, stable ordering fields, other ranking reasons, path matching, strict selection, and diverse selection were not changed.

### TDD evidence

RED and bounded old-algorithm probe (the public semantic probe temporarily restored exact membership after the initial version assertion; it is not a full-base run):

- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test -p hoimin-cli --lib plan::ranking_tests::ranking_matches_explicit_symbol_ancestors_at_dot_boundaries_once -- --exact`
  - `/private/tmp/issue473-red-ranking.log`
  - Failed as expected: nested descendants scored 100 instead of 350.
- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test -p hoimin-cli --test plan explicit_class_symbol_ranks_and_verifies_its_method_before_an_unrelated_function -- --exact`
  - `/private/tmp/issue473-red-plan-semantic.log`
  - Bounded old-algorithm probe failed as expected: `a.py:other` was rank 1/score 100 and `calc.py:Box.check` was rank 2/score 100 instead of rank 1/score 350. This is the public semantic ranking RED, distinct from the version assertion.
- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test -p hoimin-cli --test plan verify_rejects_a_ranking_version_three_plan_before_baseline_with_regeneration_guidance -- --exact`
  - `/private/tmp/issue473-red-version.log`
  - Failed as expected because a schema-3/ranking-3 plan was accepted.

GREEN:

- Ranking matrix: same command scope through `cargo test -p hoimin-cli --lib plan::ranking_tests`; 10 passed, 0 failed, 0 ignored. Final log `/private/tmp/issue473-focused-ranking-after-clippy.log`.
- Public plan suite: `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test -p hoimin-cli --test plan`; 47 passed, 0 failed, 1 ignored. Log `/private/tmp/issue473-focused-plan.log`.
- Existing Lean candidate-ranking oracle: `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test -p hoimin-cli --test lean_candidate_ranking_oracle`; 2 passed, 0 failed, 0 ignored. Log `/private/tmp/issue473-focused-lean-ranking.log`.

### Final verification

- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test --workspace --all-features`
  - Exit 0; aggregate result lines: 1611 passed, 0 failed, 13 ignored, 0 measured. Raw log `/private/tmp/issue473-full-test.log`.
- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo fmt --all -- --check`
  - Exit 0; raw log `/private/tmp/issue473-fmt-check-final.log`.
- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - Exit 0; raw log `/private/tmp/issue473-clippy-final.log`.
- `git diff --check`
  - Exit 0, no output.
- `.venv/bin/python --version`
  - Python 3.14.7; `.venv` remains an untracked symlink and is excluded from the commit.

The first Clippy run (`/private/tmp/issue473-clippy.log`) found that the new matrix test was 101 lines against the 100-line lint. I extracted its repeated positive-reason assertion into a test helper, reran formatting, the affected 10 ranking tests, format check, and Clippy. Production code did not change after the full workspace test pass, so the full suite was not repeated.

### Implementation self-review

1. Checked the resolver loop from full symbol through `rsplit_once('.')` ancestors. It uses borrowed slices, performs O(symbol depth) hash lookups, allocates nothing per ancestor, and never scans all selectors. No issue found.
2. Checked compatibility fields and diagnostics. Schema remains 3, ranking is 4, old ranking 3 is rejected before baseline, and both schema and ranking mismatch messages contain regeneration guidance. No issue found.
3. Checked the owned diff for scope and preservation. Only owned current docs and ranking/plan code/tests changed; historical design/audit files and controller-owned knowledge edits were not touched. Existing comparison/order code, reason ordering, IDs, spans, and selection policies are unchanged. No issue found.

### Test self-review

1. Checked the unit matrix against the spec: exact, class method with duplicate parent/child selectors, nested class, nested function, `BoxOther` boundary, indexed cross-file name, and missing symbol. Literal rank/scores and exact reason vectors prove one 250-point reason. No missing required case found.
2. Checked the public two-file scenario: actual `plan --source . --symbol calc:Box --operators boolean_literal` produces `calc.py:Box.check` rank 1/350 and `a.py:other` rank 2/100; actual `verify --top 1` checks the same ID/path/symbol, baseline exit 0, killed status, and unchanged plan/source bytes. No mock behavior is used.
3. Checked version and regression coverage: schema-3/ranking-3 rejection occurs before the marker baseline; full plan tests preserve source/candidate/index and top-selection regressions; the Lean oracle still covers downstream reason/scoring/order from an `explicit_symbol: bool` input. It does not prove qualified-name resolution, which is covered by literal Rust boundary tests and public plan-to-verify correspondence. No new Lean model is warranted.

### Concerns and limitations

- No material design issue was found. A shared analyzer/ranking symbol-scope abstraction would broaden the change and is unnecessary for this bounded fix; the chosen small alternative is the private borrowed ancestor predicate plus direct boundary tests.
- Full-suite aggregate counts include two one-test subprocess fixture summaries emitted during the library tests, exactly as recorded in the raw Cargo log.

## PR self-review

1. Compared base4adf809 to implementation1c2554f: descendant membership and ranking-version compatibility form one bounded change; IDs, strict selection and stable comparison remain intact.
2. Reconciled commands with raw logs: full suite1611 passed/13 ignored; final focused10 passed after a test-helper extraction; fmt/Clippy exit0. The aggregate includes child fixture summaries and is not a unique-test count.
3. Checked issue linkage, independent branch, source hashes, design-index count161 and OKF links. PR validation distinguishes the old-algorithm semantic probe from the initial version-only failure and states the existing Lean oracle takes a boolean resolver result. IDE MCP cannot inspect this project because hoimin is not open.

## Independent task review

Reviewer review473_task checked spec compliance and code quality for4adf809..1c2554f without repeating passed tests. No actionable findings: same-file dot-boundary ancestry, one250bonus, schema3/ranking4, public plan→verify behavior and pre-baseline old-version rejection all satisfy the contract. No substantive rulings or deferred findings.
