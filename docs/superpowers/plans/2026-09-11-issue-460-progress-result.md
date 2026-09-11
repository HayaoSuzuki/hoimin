# Issue 460 implementation plan

Spec: ../specs/2026-09-11-issue-460-progress-result-design.md

Global constraints: independent worktree on origin/main; no merging; preserve result policy and optional-termination compatibility. Minimum three self-reviews for every stage. The controller owns docs/knowledge and this design/plan. Implementation owns Rust and formal files plus a formal report.

## Plan self-review

1. Dependency review: Rust validation extraction and both input callers form one change; formal expectations must be generated before measuring the unfixed CLI.
2. Test review: a real CLI regression establishes rejection and absence of saturation, and existing sequence tests protect lifecycle precedence.
3. Scope review: extend existing Lean modules and executable, avoiding CI module-list churn; bounded serial Lean runs retain repository safety limits.

## OKF self-review

1. Source review: read the session/report concept and existing progress-input audit. The previous audit explicitly excludes issue 460, so its 368 historical observations cannot be presented as coverage of this fix.
2. Contract review: added the single-result shared validation design to the session/report concept while preserving the separate summary-coherence contract and null-termination compatibility.
3. Structure review: checked reserved frontmatter, source-footnote pairs, local links, root reachability, complete design indexing and the actual new design hash. The initial check passed for 16 pages, 309 source-footnote pairs and 611 links; audit references will be checked again after the report is final.

## Task 1: Shared result validation and executable Lean correspondence

Work in `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-460`. Read the spec above (resolve relative to this plan's directory), but do not read the entire plan. Own Rust source/tests, existing `formal/HoiminOracle` ProgressInput modules/corpus/generator, and a self-contained report `docs/superpowers/reports/2026-09-11-issue-460-progress-result-audit.md`. Do not edit docs/knowledge or design/plan; controller handles them. Do not spawn subagents.

- [x] Read existing `report.rs` result checks, both progress input validators, existing progress tests and ProgressInput Lean model/cases/proofs/generator/CLI adapter.
- [x] Extend the independent Lean contract for single-mutant status versus optional termination and output state; use finite equivalence classes for seven statuses, exit-zero/nonzero/timeout/OOM/process-limit/cancelled/absent termination, and complete/close-timeout outputs. Preserve all 184 existing summary/baseline cases. Include legitimate error override and legacy null cases. Keep expensive evaluation in executable, add kernel proofs and broken skip-validation and ignore-output-state witnesses. Diagnostics payload variations can stay direct Rust tests. The adapter must not compute expected dispositions itself.
- [x] Before modeling, put a correspondence worksheet and bounds in the audit report per `lean-formal-audit` and `lean-test-oracle` skills; read both skills and references. Every generated case should use real CLI schemas 2 and 3 with the same premises. Keep old cases compatible; do not weaken assertions. Observe red CLI regression before production edits and record minimal failing case.
- [x] Extract `MutantFinished::validate_result(&self) -> Result<(), ReportSequenceError>` from existing `ReportSequence::mutant_error` checks; document errors; use it from ReportSequence after existing lifecycle checks and from both progress validators. Preserve original output-state/diagnostics/classification precedence and null-termination compatibility. Use idiomatic Rust without unnecessary cloning/allocation or duplicated policy.
- [x] Add meaningful Rust tests for malformed diagnostics/required close-timeout fields, rejection context, legitimate output error, legacy compatibility and sequence invariants. Existing relevant tests plus generated real CLI cases must pass.
- [x] Run focused red/green tests, Lean proof checks and corpus freshness/sensitivity/stats, then fmt, workspace all-feature tests and Clippy once. Use `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`. The worktree `.venv` symlinks root Python 3.14; never commit it. No cargo-mutants. Cargo processes must not race other Rust implementers (none active).
- [x] Every Lean invocation must be serial through `tools/lean_resource_guard.py`: 30 second wall, 2GiB aggregate RSS, 250ms sampling, package `-j1 -DElab.async=false`. Read `docs/development.md`, guard help and CI for exact command forms. Root main has prebuilt `.lake/build`; copy needed cache into own worktree if useful, never mutate main cache via symlink. Do not run unbounded aggregate build; compile required modules individually. Preserve pinned toolchain and safety bounds. If a run fails, distinguish infrastructure error from mismatch and reduce work per invocation.
- [x] Record three concrete self-review passes each for implementation, tests, and formal audit, with fixes and real command evidence. Audit report must include claim, declared/implicit contract, worksheet, bounds, cost, minimal witness, proofs, sensitivity risk families (justify inapplicable atomicity/uniqueness), strict correspondence results, infrastructure limits, commands and resolved counterexample ledger.
- [x] Commit only owned files after verification; user authorizes issue branch commits. Use escalation if git metadata sandbox blocks. Do not push or create PR; controller does that after review.

Write full implementation report to the supplied report path, including commands, TDD failing output, passing evidence, changed files and concerns. Return only status, commit, one-line tests, concerns. If unexpected scope change is needed, send controller a precise question and continue independent checks.

## Implementation and test self-reviews

The [audit report](../reports/2026-09-11-issue-460-progress-result-audit.md) records three implementation passes, three test passes and three formal-audit passes with findings, corrections and commands. The original 184 corpus lines are unchanged. The extracted checks preserve result-error and lifecycle precedence. Both schema readers and the named minimal public CLI regression pass.

## Verification results

- Serial all-feature workspace: 1,613 passed, zero failed, 13 ignored across 69 groups. Initial parallel run had one unrelated managed-root rollback contention failure; exact isolated and serial reruns passed. This limitation is retained in the audit.
- Progress: 64 direct tests and 282 × two schema observations, plus the dedicated minimal-witness test.
- Default core report-policy and report-sequence oracle: 40 and five passed, including rejection cases excluded under contracts.
- Formatting and Clippy for all targets/features passed. After the final equivalent branch inversion requested by Clippy, direct core tests and Clippy were repeated.
- Lean model, proofs, cases, generator, corpus freshness, five sensitivity gates and stats passed. Peak observed successful guard command: 5.172 seconds, 723,024 KiB aggregate RSS. Retained limits: 30 seconds, 2GiB, 250ms sampling, serial execution.
- Final OKF checks cover 16 pages, 311 source-footnote pairs, 615 links, root reachability, all design/audit documents and new hashes.

## PR self-review

1. Scope: reviewed source diff against issue acceptance criteria and design; the single-result method replaces the writer's checks and is called by both readers. No lifecycle synthesis or policy duplication.
2. Evidence: checked raw workspace totals and final core/Clippy logs against the audit and PR draft. Disclosed initial contention failure and sandbox monitor limitation instead of reporting unconditional parallel-suite success.
3. Documentation: separated historical 368 observations from this change's 564 observations, verified new source hashes and index entry counts, and kept Lean model claims distinct from observed Rust correspondence. Independent task and branch review results are recorded in the local execution ledger before publication.
