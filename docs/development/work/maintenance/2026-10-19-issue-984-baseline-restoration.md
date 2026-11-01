# Issue 984: restore baseline workspaces before mutation

The successful baseline now enters `RunPhase::BaselineRestoration` and emits the existing owned `ResetWorker(0)` operation. Only its matching `WorkerReset` acknowledgement releases ordinary analysis or, for ordered verification, the remaining-budget observation. Baseline output delivery remains independent: either acknowledgement may arrive first. The reset restores the preflight snapshot before any candidate is applied, including runs that ultimately discover no candidates. A failed baseline does not request this reset.

The restoration phase retains the workspace copy grant and uses the existing reset task, retirement, integrity-check and cleanup ownership paths. Cancellation, deadline and disk stop retire the pending reset; a late acknowledgement cannot dispatch. Reset/output failures use the existing infrastructure-failure path. Wrong worker, wrong completion kind and duplicate acknowledgements remain rejected. Baseline stage metrics include restoration, while the recorded baseline process duration and automatic mutant timeout calculation retain their existing meaning.

No reset, mutation-write, hardlink-detachment or filesystem cleanup implementation was changed. Same-content external hardlinks remain shared after reset and mutation still rejects them with `workspace.mutation.shared`. The new real-process sentinel regression checks unchanged external bytes and mode and exactly one process launch (the baseline).

## Saved-result compatibility

Execution fingerprint schema is **13**, and the package release floor is **0.13.0**. Published v0.12.0 used schema12. Without this epoch change, an incomplete old run could reuse a conclusive killed result caused by baseline contamination even after its newly invoked workspace was correctly restored. The existing session-selection boundary now excludes genuine schema12 digests. No adapter, result conversion or database deletion was added.

When explicit resume finds only incompatible incomplete history, it reports `session.resume.incompatible` and preserves the database. Use another `--session PATH`, or deliberately start a new run without `--resume`. Compatible current histories remain resumable, including when newer incompatible history exists; completed old histories follow the existing fresh-run path. SQLite schema5, plan10, report8, ranking rules, candidate descriptors and mutant IDs are unchanged. Prior contaminated verdicts and scores may differ on a fresh execution.

The five package/lock files contain only the release-floor update; dependencies were not upgraded. Historical synthetic matching-digest storage fixtures remain storage-layer controls and do not represent legitimate cross-epoch CLI fingerprint matches.

## Model correspondence

`HoiminOracle.Model` adds `restoring` and `restoration`. A matching restoration completion emits the next ordinary effect; stop retires restoration just as ordinary work. Existing no-dispatch-after-stop, final-output ordering and accepted-result preservation proofs cover the new constructors. A direct matching-ACK lemma and a deliberately broken skip-restoration witness exercise the new gate.

Six new strict corpus scenarios cover restoration success, duplicate completion, wrong kind, cancellation with late completion, deadline with late completion, and cleanup ordering. The existing sixteen strict scenarios remain. The Rust adapter explicitly completes `VerifyOriginals` when normalizing a stop from restoration to the model's cleanup boundary. It does not pretend that core cleanup is immediate or skip integrity verification. The abstract model does not prove filesystem IO correctness, cancellation of blocking IO, BLAKE3 collision freedom, or operating-system scheduling.

## Validation

All Cargo commands used the isolated worktree, target `/tmp/hoimin-issues-984-989/target`, jobs2, no debug information or incremental builds, offline dependencies, and `TMPDIR=/tmp/hoimin-issues-984-989/tmp`. Expensive test suites used two test threads. `.venv` was read-only; Lean used this worktree's own `.lake` and the repository resource guard, bounded to Model/Proofs/Cases and the generator.

- Old-production RED: the core emitted no ResetWorker; generated/existing-state CLI cases produced `[killed, survived]`; selected-source changes produced `workspace.mutation.invalid`. Both frozen schema12 vectors still matched. Logs: `task5-red-core.log`, `task5-red-cli.log`, `task5-red-resume.log` under the task temporary directory.
- Stored-result sensitivity: temporarily reverting only fingerprint13 to12 made the conclusive historical fixture return `Resumed("contaminated")`; the rejection assertion failed. The original13 source was restored in a finally block. Log: `task5-red-conclusive-session.log`.
- `cargo test --locked -p hoimin-core -- --test-threads=2`: 508 passed, 2 existing ignored. Includes machine103, bounded ACK/stop schedules, empty-candidate/failure coverage, fingerprint vectors and strict Lean correspondence. Log: `task5-core-full.log`.
- `cargo test --locked -p hoimin-cli --lib --test run_e2e --test plan --test session_handler --test workspace_handler --test lean_workspace_oracle --test lean_session_oracle --test lean_resume_diagnostic_oracle --test resume_budget --test fingerprint_env -- --test-threads=2`: 1349 passed,30 existing ignored across the ten top-level suites (two nested child-fixture successes are not double counted). Includes library1054/28 ignored, plan114/1 ignored, run76, session51/1 ignored, workspace36 and resume/oracle controls18. Log: `task5-cli-full.log`.
- Contract-enabled `cargo test --locked -p hoimin-core --features contracts --test machine --test lean_oracle -- --test-threads=2`:109 passed (machine103 and oracle6), exit0. Log: `task5-core-contracts.log`.
- After extracting plan fixture setup, its focused full-versus-single regression passed again. Log: `task5-plan-final.log`. Workspace formatting and `git diff --check` passed. Normal hooks are required for the task commit; their exact results are retained in its implementation report.
- Scoped guarded `lake build +HoiminOracle.Proofs:o generate`, generator `--output corpus/state-machine.jsonl` and `--check corpus/state-machine.jsonl`: exit0. Build guard peak453984KiB; corpus-check peak57328KiB. Logs: `task5-lean-build.log`, `task5-lean-generate.log`, `task5-lean-check.log`.
- Release-floor/manifest tooling tests:4 passed,49 deselected. Log: `task5-release-tests.log`.

The task report contains the final CLI suite counts and hook outcomes. Tests ran on macOS. Native Windows hardlink/readonly behavior and Linux-specific runtime behavior require their platform CI; no such run is claimed here. Final branch-wide integration and independent review belong to the coordinator.

## Five requirements/design review passes

1. Matched the frozen issue's generated/existing/source and verify counterexamples to an unconditional successful-baseline reset; no-candidate analysis cannot bypass it.
2. Traced pending effect identity, worker0 ownership, output continuation and copy-grant lifetime. One typed phase is sufficient; no duplicated cleanliness flag is needed.
3. Traced stop/failure retirement and both acknowledgement orders. Retirement alone does not cancel filesystem IO; existing owned-task quiescence remains necessary.
4. Compared the model's cleanup abstraction to the real original-integrity checkpoint; required explicit adapter normalization instead of an immediate-cleanup claim.
5. Traced saved-result selection/reuse and published schema12 compatibility; chose fingerprint13 and floor0.13.0 without changing persisted shapes or historical storage goldens.

## Five plan review passes

1. Chose independent old-production core/CLI/fingerprint RED cases before changing production.
2. Preserved a pending-restoration fixture while adapting existing analysis fixtures to acknowledge reset explicitly.
3. Limited production edits to phase transition, exhaustive progress label, metrics grouping, fingerprint epoch and package floor.
4. Kept independent Lean generation and bounded model builds; selected existing ownership/session/resume regressions as controls.
5. Kept one Cargo process, read-only shared environment, separate root-owned canonical documentation, normal commit hooks and task-scoped staging.

## Five implementation review passes

1. Checked the full transition diff and effect acceptance against ownership; strengthened baseline-failure assertions to prohibit reset as well as mutant execution.
2. Added bounded generated jobs/ACK/repeated-stop schedules and explicit empty-analysis/top-budget checks; confirmed baseline process timing is unchanged.
3. Found and corrected a test reading nonexistent top-level report IDs; it now requires `candidate.id`, checks explicit selection and compares literal survived outcomes.
4. Replaced a synthetic reused-killed historical fixture with `Executed { Exit(1), Complete }` evidence; preserved rows and added completed-old/current-preference controls.
5. Full core checks exposed missing corpus vocabulary and an overly broad integrity normalization for late final-output stops; corrected both. Existing sixteen scenarios and the six restoration cases agree. Metrics stage tests use named generated AAA cases; plan setup stays in a test-only helper.

## Verification review passes

1. Read command exits and summed final core results; compile/fixture mistakes and intermediate oracle failures are recorded separately from valid semantic RED evidence.
2. Compared actual old-production failures with current regressions and executed the schema12 reversion sensitivity test; expected verdicts are independent literals.
3. Checked all strict corpus rows and explicit original-verification normalization; documented the model's filesystem/cryptographic limits.
4. Inspected the five version-only diffs and historical11/12/current13 vectors; confirmed no dependency upgrade or saved-format shape changes.
5. Final scope, CLI results, contract-enabled checks, ignored/platform limits and normal hook outcomes are recorded with the task commit report; root-owned canonical documentation is excluded from this task commit.

## Normal hook correction

The first normal commit hooks passed workspace/vendor formatting and vendor Clippy. Workspace Clippy rejected the new empty-Vec assertion style (`assert_is_empty`). The metrics and analogous core ACK assertions were changed to `assert_eq!` for value-bearing failure diagnostics; their covering tests were rerun. No production behavior changed and no hook was bypassed. The failed attempt is retained in `task5-commit-hooks.log`; the normal retry is recorded in `task5-commit-final.log`.
