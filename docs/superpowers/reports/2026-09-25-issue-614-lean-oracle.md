# Issue 614: resume diagnostic Lean oracle handoff

## Claim and scope

For valid current-schema saved rows, an owned latest compatible incomplete run within the requested budget wins over every diagnostic history fact. When no candidate exists, the observed-history classifier applies the documented priority. This oracle observes the actual `SessionHandler::load` result, including selected run ID and stable fresh-reason code.

The model uses a fixed requested budget2 and representative saved budgets1/2/3. Its five independent categories are two eligible matching rows, a matching incomplete over-budget row, a matching completed row, an unrelated incomplete row, and an unrelated completed row. All32category subsets in both insertion orders give64strict cases (maximum6rows). Two eligible rows have distinguishable IDs, so reversing insertion order verifies latest eligible selection, including older eligible candidates beneath unrelated newer history.

The additional16initial-none/later-eligible cases and1conditional-update-loss case are explicitly `model-only`. The adapter refuses them as a selected strict reproduction. Root's direct private-classifier unit test checks the first race fact without claiming SQLite scheduling control. This oracle does not establish SQL isolation, lock ownership, schema migration, corruption rejection, arbitrary budget behavior, or hash injectivity; existing tests remain responsible. `no_compatible_run` is a legacy externally supplied event fallback, outside actual current SessionHandler-load outcomes and this corpus.

## Design and plan review passes

1. Matched the root's amended design before writing files: successful ownership takes precedence; no-candidate history's newly eligible row is candidate_changed, not fingerprint_mismatch. Retained separate initial-candidate and later-history phases.
2. Chose independently enabled concrete row categories, not unconstrained EXISTS booleans that could describe impossible histories. Added both row orders and two distinguishable eligible IDs to test newest selection and older-compatible fallback.
3. Planned actual BeginSession/FinishSession/LoadSession calls on isolated SQLite databases, with no direct SQL expectation reconstruction. Model-only concurrency cases remain labeled and excluded from strict execution.

## Implementation review passes

1. Model selection uses reverse.find and eligibility on compatibility, completeness, and budget. General precedence theorems cover arbitrary Boolean remainder facts; owned-candidate theorem is conditional on successful selection. All new proofs have maxHeartbeats10000.
2. Adapter uses the existing real session persistence API for fixture creation and reading; writers are dropped before read. Effect ID, selected run ID, and reason are read from actual responses. Fixture errors fail as infrastructure errors, not model mismatch or success.
3. Independent root review found no model/adapter blocker but identified a guard gap: rows could disagree with declared category flags. Added exact fixture-input metadata/order validation without duplicating expected selection/reason logic. Registered model/native target/generator/corpus in all four existing CI registries, preserving generator declaration order.

## Test review passes

1. Corpus schema1 is closed; modes/phases are a finite domain; null observations must be present; unknown fields/codes, duplicate IDs/inputtuples, and missing domain cases fail. Strict64 plus race16 plus loss1 has unique complete declared input coverage.
2. Strengthened category validation tests with metadata changes (compatible/complete/budget/name), all six flags, missing rows, and swapped insertion order. These guards protect coverage claims without recomputing the oracle result.
3. Sensitivity detects always-fresh, first-eligible, incorrect empty-history reason, mismatch-before-completed, completed-before-budget, and incorrect stale-race mismatch. A deliberately always-fresh Lean selected function first failed the concrete eligible-candidate proof, then the real selector passed. No generated expectations were edited manually.

## Verification and reproduction

Working directory is `.worktrees/issue-614`. Cargo environment:

```
CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target/batch-progress
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1
```

Local core/cli package outputs were cleaned when switching from628, then the oracle rebuilt. Run:

```
cargo test -p hoimin-cli --test lean_resume_diagnostic_oracle
cargo clippy -p hoimin-cli --test lean_resume_diagnostic_oracle -- -D warnings
python3 -m unittest tests.test_ci_workflow
uvx --offline ruff check tests/test_ci_workflow.py
uvx --offline ruff format --check tests/test_ci_workflow.py
```

For a single strict reproduction, set `HOIMIN_RESUME_DIAGNOSTIC_ORACLE_CASE` to a generated strict case ID and run `real_session_history_matches_lean_diagnostics`.

In `formal/HoiminOracle`, run each command separately under the existing resource guard:

```
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/614-lean.json -- lake build +HoiminOracle.ResumeDiagnosticModel:o
# Same guard for each:
lake build +ResumeDiagnosticAuditMain:o
lake exe generate_resume_diagnostic --output corpus/resume-diagnostic.jsonl
lake exe generate_resume_diagnostic --check corpus/resume-diagnostic.jsonl
lake exe generate_resume_diagnostic --sensitivity
lake exe generate_resume_diagnostic --stats
```

All six guarded GREEN commands passed; maximum7.535s and1,292,976KiB across successful commands, limits20s/2GiB, no bound increase. An initial wrong working-directory invocation and reserved Lean identifier error were corrected before the meaningful semantic RED; neither was counted as a semantic result. Resource-monitor logs and RED/GREEN logs are under `/tmp/hoimin-batch-604-632/614-lean-*`. Global Lean slot released.

Workflow40tests and offline Ruff lint/format passed. Full-workspace/exact-CI static gates are owned by root; this subtask does not claim those broader results. Final focused adapter/clippy logs: `614-oracle-rust-final.log`, `614-oracle-clippy.log`. No unresolved mismatch or ownership decision remains within the modeled scope.
