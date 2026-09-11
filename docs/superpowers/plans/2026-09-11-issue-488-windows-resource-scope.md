# Issue 488 implementation plan

Spec: ../specs/2026-09-11-issue-488-windows-resource-scope-design.md

Independent base4adf809, no stacking on #487. Controller owns docs/superpowers and docs/knowledge, native workflow dispatch, PR and ledger. One architecture implementer owns Windows/backend/test/current docs and exact CI contracts. All Cargo commands serial with `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`. No cargo-mutants or unrelated Windows base fixes. Logs /private/tmp/issue488-*; .venv CPython3.14.7 remains untracked.

## Plan self-review

1. Separate the issue-authorized per-root contract from a new aggregate limiter. Explain concurrency cost explicitly and preserve #340 offender attribution and root ownership.
2. Correct actual ProcessLimits requests before interpreting native boundaries. Query native flags/values, synchronize finite loads and verify both roots overlap; sleeps alone cannot prove aggregate occupancy.
3. Require Windows native execution from an independent manual job, preserve exact existing workflow equality checks, and distinguish local cfg compilation from actual native evidence. Do not count setup skips as successful acceptance.

## OKF self-review

1. Compare lifecycle and reporting claims with outer versus inner Job Object configuration and core full-value requests.
2. Add the exact issue design source and index heading; preserve historical native/formal audit revisions and explain selected scope and costs.
3. Validate16-page metadata, source-footnote pairs, hashes, local links/reachability and161 actual/index/display designs. Refresh hashes after evidence-led design refinements.

## Task 1: Align Windows scope and prove native request correspondence

Read spec, /private/tmp/issue488-preflight-notes.md and issue488 body in /private/tmp/hoimin-bug-issues.json. Worktree /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-488; branch fix/issue-488-windows-resource-scope; base4adf809. Own relevant Rust Windows/backend/test files, README/CLI help/current docs, .github/workflows/non-linux-ci.yml and tests/test_ci_workflow.py. Controller owns docs/superpowers and docs/knowledge. Do not delegate, push, dispatch or create PRs. Send concrete evidence before material scope expansion.

- [x] Reproduce the public contract/request-value mismatch with focused checks before edits. Trace every WindowsBackend constructor/test helper and actual ProcessLimits path, outer kill-only job and inner cap configuration. Inspect existing #340 attribution and cleanup tests.
- [x] Remove unused RunLimits constructor parameters and misleading validation documentation; set explicit actual ProcessLimits in each affected native fixture. Add native readback for inner JobMemoryLimit/ActiveProcessLimit/flags and outer kill-only/no-cap flags without a public native-handle API.
- [x] Align CLI help, README resource scope/explanations and report-field interpretation with per-root Windows caps. Describe jobs multiplication and committed memory/root-inclusive process count. Preserve other platform semantics and leave separate #487 reporting propagation independent.
- [x] Add bounded actual CLI jobs=2 barrier fixtures for memory and process scope: each root below its cap, simultaneous sum above a single cap, correct native values, successful processes/survivor CLI semantics and cleanup. Native query NULL denotes immediate assigned job; exact ctypes types/layout/error handling if using child inspection. Baseline bypasses mutant barrier; shared markers outside copied workspaces, bounded waits/allocations/spawns.
- [x] Preserve offender-plus-healthy-sibling cases for memory/process and verify attribution and descendant cleanup. Fix formerly unbounded or misleading relevant fixtures with finite loads. Fail or explicitly report unavailable capability; no captured skip interpreted as native execution.
- [x] Add independent manual Windows resource-scope CI job with pinned setup, bounded timeout and exact focused native test commands. Preserve existing shared manual/automatic steps equality; assert the dedicated job separately and run full tests/test_ci_workflow.py. No broad assertion relaxation or dependencies on known unrelated base failures.
- [x] Run local focused contracts, workspace all features, fmt and all-target/all-feature Clippy serially. No new Lean model unless a real unresolved state-machine claim needs derived expectations and actual Rust correspondence; native configuration/load evidence is essential here.
- [x] Record three implementation and three test self-reviews, exact commands/results and local/native distinction in .superpowers/sdd/2026-09-11-issue-488-windows-resource-scope/task-1-report.md. Commit owned files and stop for task review. Controller will push reviewed branch, dispatch native CI and return concrete failures if any; native acceptance remains pending until actual Windows results.

## Task 2: Native acceptance and PR completion (controller)

- [x] Review committed implementation and exact workflow before push. Dispatch non-linux-ci.yml on this branch and observe the independent Windows resource-scope job.
- [x] Obtain real memory/process scope, native caps, sibling status and cleanup results. Return task-related failures to implementer; rerun affected checks only. Keep unrelated base Windows failures separate and report them accurately.
- [ ] Persist full report, native evidence, contract choice/cost and all three stage reviews. Final OKF/fmt-diff validation, documentation commit, whole-branch review and PR publication. Keep worktree; do not merge.

## Native interpreter refinement

Root primary-source inspection found CPython Windows venvlauncher introduces an extra process and inner kill-only job. Use a verified direct base interpreter for exact native cap/readback/count fixtures, preserving P=3 and required root-plus-child occupancy2. Do not change production launch behavior or treat source inspection as native proof. The design records the source and three correspondence reviews; worker was informed before native acceptance implementation completed.

## Controller implementation review and native dispatch

Three implementation reviews:1 traced outer kill-only versus inner ProcessLimits configuration and all constructor/cfg migrations;2 inspected actual CLI marker/barrier, native ctypes layouts/return sizes, process-handle ownership and finite offender/sibling cleanup;3 checked exact dedicated workflow mapping and retained shared-job equality, verified windows-sys0.61.2 provides the native structure Default implementation, and separated local cfg compilation from native execution. No actionable code defect found. Direct-base interpreter correspondence is documented above and independently implemented by the worker.

Independent task reviewer review488_task approved4adf809..1f8f7ca without actionable findings. Native acceptance remained explicitly pending at that review. Controller pushed1f8f7ca and dispatched https://github.com/tokyogas-tech/hoimin/actions/runs/34611097571; dedicated job Manual Windows resource scope, ID103301509363. This records dispatch, not success.

Local result accounting:70 raw workspace summaries total1609 passes, but include two subprocess harness summaries (each1pass/600filtered). Top-level results are1607 passed,0failed,13ignored across68summaries. The raw worker report count is preserved with this distinction; no suite rerun is needed to clarify counting.

## Persisted local implementation report

# Issue 488 implementation report

Task implementation complete locally; native Windows acceptance remains pending controller dispatch/review.

## Contract and implementation

Retain the existing Windows per-root Job Object contract, not a new aggregate limiter. Each root tree receives its full requested memory/process caps; concurrent consumption can approach jobs times those caps. Outer run job only owns cleanup. Preserve attribution and ownership mechanisms from #340. Removed ignored WindowsBackend::new(RunLimits) argument and all cfg-gated callers/test fault helpers; callers now use new() and actual ProcessLimits. No #487 reporting API/schema changes.

CLI help and README now explain committed memory, root-inclusive process counts, jobs multiplication, parent-job restrictions and report enforcement-strength versus scope. Linux hard cgroup and portable policies remain distinct. README records Rust source API migration.

Native unit readback calls real prepare() then QueryInformationJobObject on private inner/outer handles: outer kill-only/zero caps; requested128MiB/3 and160MiB/16 inner flags/values. No public handle accessor.

New actualCLI Windows target has two tests (memory and processes), each two mutants/jobs2 with a sequential baseline bypass. External atomic JSON markers hold bounded96MiB payloads/one child behind a controller release. ctypes uses native field widths/layouts, declared Win32 signatures, API/return-size checks and NULL immediate-job lookup. Rust checks exact native caps/flags,2active processes per root, distinct root PIDs, live handles for both roots/children, sum above one cap, actual normalized_config, baseline/mutant Exit0, survived status/CLI1 and all handle-signaled cleanup. No missing-capability return or fabricated observation.

Both newCLI and corrected handler fixtures require .venv/pyvenv.cfg home/python.exe as a direct base interpreter, then descendants use sys.executable. This avoids Windows venvlauncher's extra process/deeper kill-only job; no fallback or production launch change. Native count/cap assertions must validate correspondence.

Existing sequential handler classification now requests actual128MiB/3 and finite allocation/spawn attempts. Old sleepy per-root success fixtures were replaced by stronger nativeCLI barriers. Separate concurrent memory/process offender tests hold a healthy sibling until classification, assert only offender's resource status and healthyExit0/output, and check all published descendants including the extra child admitted before P3 denial. Timeout sibling and close/reject-new-spawn tests remain.

Independent manual windows-resource-scope job has20-minute bound and pinned existing setup actions; scoped Windows Clippy --lib plus affected targets; direct all-feature Windows unit/process_handler/newCLI commands with nocapture. Existing automatic/manual shared-step equality remains exact, separate dedicated job contract compares full mapping.

## Three implementation self-reviews

1. Traced core requests to prepare/create_limited_root_job and outer configure_run_job; verified only inner caps enforce resource limits. Removed every Windows constructor/fault-helper RunLimits use and narrowed Linux-only imports. No accidental aggregate enforcement or attribution change.
2. Reviewed Windows-only native ownership/FFI and fixture interpreter identity. Corrected temporary command/path borrows in async CLI harness; kept handle ownership private/RAII. Checked ctypes int64/DWORD/SIZE_T fields and full return-size checks. Native compilation/execution still required.
3. Audited README/help/report interpretation and workflow boundaries. Removed blanket run-wide/jobs-not-multiplied language; excluded in-process analyzer; documented source API migration and external-parent restrictions. Preserved unrelated workflow jobs and #487 independence.

## Three test self-reviews

1. Behavioral witnesses: aggregate cap or wrong request caps must fail simultaneous barriers/readback; merely sleeping or passing constructor numbers cannot satisfy tests. ActualCLI has two arithmetic mutants, baseline bypass and shared markers outside copied workspaces. Payload remains referenced through release; all four process handles must be live.
2. Reviewed finite failure/cleanup paths: bounded32x8MiB attempts, bounded spawns and deadlines; healthy root released only after offender classification. Added explicit PID cleanup for extra admitted descendant before process-limit denial. CLI kill-on-drop and native backend ownership bound failure paths; no setup skip counts as acceptance.
3. Reviewed assertions and test execution boundaries: exact cap flags/values, normalized config, both Exit0 outcomes, survivedCLI1 distinction and cleanup after completion. Full Python workflow module covers exact dedicated mapping and unchanged shared-job equality. Local cfg-skipped native test counts are reported separately; Windows CI must execute2newCLI cases and relevantunit/handler cases with nonzero counts.

## Verification evidence

All Cargo commands serial with CARGO_INCREMENTAL=0 and CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target. Host macOS/aarch64, Rust1.98.0, .venv CPython3.14.7; disk remained13GiB free.

RED before production edits:
- cargo test -p hoimin-cli --test cli_config run_help_explains_windows_per_root_resource_scope:0pass/1fail, missing Windows scope help (issue488-red-cli.log).
- .venv/bin/python -m unittest tests/test_ci_workflow.py:27tests,1failure plus1missing-job KeyError,25pass, absent dedicated native job (issue488-red-ci.log). Added an assertIn before exact mapping comparison for a cleaner future missing-job failure.
- Native tests were not run RED locally: Windows configuration/behavior tests supplement existing implemented per-root behavior; native evidence is pending, not claimed.

GREEN:
- cargo test -p hoimin-cli --all-features --test cli_config --test process_handler --test windows_resource_scope:82pass (56CLI,26handler),0fail; Windows-only target0tests on macOS (issue488-focused.log).
- cargo test --workspace --all-features:1607top-levelpass,0fail,13ignored across68top-leveltest/doc-test results (raw70summarylines sum1609pass because two subprocess harnesses each print1additional pass); exit0 (issue488-workspace.log).
- cargo fmt --all -- --check:exit0 after formatting.
- cargo clippy --workspace --all-targets --all-features -- -D warnings:exit0 (issue488-clippy.log).
- Full .venv/bin/python -m unittest tests/test_ci_workflow.py:27pass/0fail, final6.058s (issue488-green-ci.log).
- .venv/bin/python -m py_compile crates/hoimin-cli/tests/support/windows_resource_scope.py:exit0 (syntax only, not native Win32 execution).
- git diff --check:exit0.

The final edits after fullworkspace only strengthened cfg(windows) test cleanup/assertions and formatted code, so local macOS behavior was unchanged; final all-target Clippy/fmt include the final source. Windows compilation and native execution remain pending. No cargo-mutants/Lean expansion/cross-compile surrogate used; no push/dispatch/PR performed by implementer. Logs are under/private/tmp. Scratch report stays ignored, never force-added.

## Native CI compile correction

Windows run34611097571 quality job103301509705 exposed E0425 in the new offender cleanup: wait_until_process_stops is Linux-only. This is actual native compilation failure, not successful acceptance. Replaced the call with existing cross-platform wait_until_process_stops_before and one shared two-second absolute deadline outside the descendant loop, preserving a bounded total cleanup wait. Audited the newly referenced native_python/native_run_python/root_limits, FixtureChildGuard, native_arg, process_exists and wait helpers: Windows implementations or cross-platform definitions exist. No helper cfg expansion or native skip added. Native recompilation/execution still pending controller redispatch.

Three correction checks: (1) matched CI symbol and cfg declaration to root cause; (2) verified one deadline is shared by all descendant waits rather than renewed per child; (3) inspected other new helper cfg dependencies, preserving cleanup assertions and exact cap tests. Local fmt and all-target/all-feature Clippy rerun below; no full suite repetition because only cfg(windows) test reference changed.

Correction validation: cargo fmt --all -- --check exit0; cargo clippy --workspace --all-targets --all-features -- -D warnings exit0 (issue488-native-fix-clippy.log,0.89s); git diff --check exit0. Windows native compilation/execution remains pending.

## Native compile feedback and second dispatch

First Windows quality job103301509705 failed with E0425 in the new process_handler fixture because wait_until_process_stops is Linux-only. Controller retrieved the completed job log directly through the GitHub API while the overall workflow was still running; /private/tmp/issue488-windows-quality.log retains the failure. This was task-related, not the known base import failure. Controller cancelled obsolete run34611097571 once the compile blocker was confirmed; it supplies no native acceptance success.

Worker fix7e7d08e uses existing cross-platform wait_until_process_stops_before with one shared two-second deadline. Root inspected the definition/call and reviewer review488_task approved1f8f7ca..7e7d08e without findings. Local fmt/Clippy/diff checks passed; no unrelated suite rerun or assertion weakening. Controller pushed the fix and dispatched https://github.com/tokyogas-tech/hoimin/actions/runs/34611937206, dedicated job103304326195. Native compilation/execution is pending as of this entry.

Second-run Windows quality job103304326332 fails only unchanged workspace_recovery.rs unused WorkspacePlan import (zero diff4adf809..7e7d08e), matching the known base blocker. Saved /private/tmp/issue488-windows-quality-second.log. This is separate from the fixed E0425; dedicated scoped Clippy/native execution remains pending.

## Completed native acceptance and final PR self-review

The preceding pending statements preserve the chronology of local reporting and dispatch; native acceptance is now complete at implementation7e7d08e.

Implementation7e7d08e passed the dedicated [Manual Windows resource scope job](https://github.com/tokyogas-tech/hoimin/actions/runs/34611937206/job/103304326195) on Windows Server2025 (windows-2025-vs2026), Rust1.98.0 x86_64-pc-windows-msvc and CPython3.14.7 x64. Scoped native Clippy passed. Actual native tests:25 Job Object unit tests,5 process-handler tests and2 public CLI tests;32 passed,0 failed,0 ignored, with no capability/setup skips in the nocapture log.

The memory barrier observed two live roots, each with a live child, exact167772160-byte memory caps and100663296-byte retained payloads. Combined payload192MiB exceeded one160MiB cap; native peak job memory was111456256 and111927296 bytes. The process barrier observed2 active processes in each root job under cap3, for4 simultaneous processes overall. Both jobs reported native flags8712 and exact requested caps. The CLI assertions confirmed normalized configuration, baseline/mutant Exit0, two survivors, CLI exit1 and all retained root/child handles signaled after cleanup. Separate handler tests passed memory/process offender attribution, healthy sibling completion, timeout isolation and close/reject-new-spawn cleanup.

This proves the bounded native fixtures on that runner, not universal host/interleaving or notification delivery properties. First native run34611097571 exposed a Windows-only test helper compile error and was cancelled;7e7d08e corrected the helper and shared cleanup deadline before the successful second run. The second overall manual workflow still failed unchanged base checks: unused WorkspacePlan import in workspace_recovery, six Lean shell-fixture contract tests on Windows, and the target/git literal-backslash fixture (624 passed,1 failed,8 ignored before later integration targets). All macOS jobs and Windows core purity passed. Dedicated acceptance directly executed the relevant native targets despite those independent base failures.

Logs: /private/tmp/issue488-windows-resource-scope.log, issue488-windows-quality-second.log, issue488-windows-wheel.log and issue488-windows-rust.log. The unchanged target/git and workspace_recovery files have zero diff against4adf809; only the separate PlatformExecutionPolicyContractTests mapping was added to tests/test_ci_workflow.py, leaving LeanAuditWorkflowContractTests unchanged.

Three final PR self-reviews:
1. Checked the entire branch against issue488's permitted contract choices, actual ProcessLimits and native results. Explicitly disclose per-root caps, potential jobs multiplication and WindowsBackend::new() source API migration; no new aggregate limiter or #487 provenance claim.
2. Matched each reported test count and native observation to logs and successful job steps. Separate the first task-related compile failure/fix, local cfg-skipped target and unrelated manual Windows failures. Native32 passed with no hidden skips; no universal proof claim.
3. Checked all changed files, exact workflow contracts, three source hashes,161-entry design catalog and preserved historical OKF metadata. Keep .venv and SDD scratch untracked. The final documentation commit is followed by whole-branch independent review, push and PR creation; publication/CI outcome is recorded in the controller ledger and PR.

Contract costs accepted: existing per-root semantics require users to size jobs and caps together; removing the ignored constructor argument requires Rust caller migration. Native CI adds a bounded Windows job and direct-interpreter fixture maintenance. No unresolved task-review findings or enforcement waivers.
