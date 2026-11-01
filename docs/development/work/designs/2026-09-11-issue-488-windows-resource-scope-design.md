# Issue 488: Make Windows per-root resource limits explicit and test the native caps

Issue: https://github.com/tokyogas-tech/hoimin/issues/488

## Public contract and implementation boundary

Choose the issue-authorized per-root contract already implemented by Windows nested Job Objects. Each baseline or mutant root process and its descendants share the full configured max-memory and max-processes limit. With jobs=2, two concurrent roots can together consume more than one configured limit; the Windows backend does not impose an aggregate run cap. Preserve #340 root-specific attribution instead of adding aggregate notifications that can blame a healthy sibling.

The outer run job retains kill-on-close ownership and cleanup. Each inner root job enforces ProcessLimits with JOB_OBJECT_LIMIT_JOB_MEMORY and JOB_OBJECT_LIMIT_ACTIVE_PROCESS. The latter counts the root process itself. Memory is committed memory accounted to the job, not RSS. Parent jobs imposed by a host can make effective limits stricter; do not promise every environment can consume the full configured allowance.

Remove the unused RunLimits constructor parameter and stale error documentation. Native tests must set each actual ProcessLimits request explicitly, then read back the configured Job Object fields through QueryInformationJobObject. A constructor argument that was ignored is no evidence about the tested boundary. Keep production API and ownership changes narrow; use private unit access or test-only Python ctypes from children rather than exposing private native handles publicly.

## User-facing explanation and cost

Align CLI help and README resource tables, examples and report-field interpretation. Report resource_mode hard describes native enforcement strength, not aggregation scope. On Windows, normalized_config max-memory/max-processes are limits per root tree. Audit every statement that says jobs does not multiply these limits and qualify it by backend. Other platforms retain their existing enforcement behavior. Replace blanket aggregation wording with the actual platform-specific facts: Linux hard cgroup limits are aggregate, Windows limits are per-root, and portable Unix retains its documented per-process RLIMIT_AS/best-effort policy (macOS memory is not enforced). The Hoimin CLI itself is outside descendant-memory accounting; do not repeat the old analyzer-sharing wording that contradicts that existing scope.

This branch starts at4adf809 independently of #487; it does not claim to fix the separate missing reporting provenance. A new report schema field or duplicate backend-policy propagation is not required merely to explain existing fields. If native investigation shows a field is necessary, provide evidence and a compatibility design before expansion.

Material choice: this patch does not implement a new aggregate Windows cap. Concurrent consumption can approach jobs times the configured root limits (and external parent constraints can lower it). This is an explicit issue-authorized contract choice and must appear in the PR and final user report. Its benefit is truthful limits without changing root attribution or cleanup semantics; its operational cost is that users must size concurrency and per-root limits together.

## Native acceptance evidence

Add finite, bounded Windows jobs=2 barrier tests for memory and process scope. Each root stays below its own configured cap while both together exceed a single cap. Suggested memory fixture: each root holds and touches96MiB payload under160MiB cap, with measured interpreter overhead margin and simultaneous barrier observations. Suggested process fixture: each root plus one child is2 active processes under3 cap while combined count is4. Do not use unbounded allocation or spawn loops. Capture real native limit flags/values, concurrent occupancy and process outcomes; then verify child cleanup.

Prefer an actual CLI jobs=2 fixture so configured values flow through core requests to native jobs. A child can query its immediate job using QueryInformationJobObject(NULL, JobObjectExtendedLimitInformation, ...), whose documented nested-job meaning is the immediate assigned job. Use exact ctypes layouts, return-size/error checks and explicit assertion failures. Baseline must not wait for a two-mutant barrier. Synchronization markers live outside copied workspaces, with bounded waits and cleanup. Successful surviving mutants can make CLI exit1; this is not a failed process outcome.

Retain separate offender-plus-healthy-sibling tests for both resource types and assert the correct offender status, healthy sibling completion and cleanup. Fix the existing misleading128MiB/3 and160MiB fixtures to pass their limits through each real request. Read back outer flags to confirm no aggregate cap and inner flags/values to confirm per-root caps. A local Linux/macOS build or a passing test that silently skips native setup does not count as Windows evidence.

## CI and verification

Existing manual Windows full-suite jobs stop at unrelated base failures before resource integration tests. Add an independent manual Windows resource-scope job that runs the relevant native unit/integration targets directly with pinned existing setup actions and a bounded job timeout. It must not depend on failing quality or unrelated Rust targets. Keep existing automatic/manual shared-job equality assertions intact and add an exact separate workflow contract for the new job. Run the entire tests/test_ci_workflow.py module; do not merely validate YAML or loosen the job-set assertion.

Push the reviewed branch and dispatch this job to obtain actual Windows execution before claiming native success. If capability is unavailable, preserve explicit unverified/failure evidence; do not count captured setup skips. Fix task-related failures and repeat only affected checks. Do not repair unrelated Windows base issues as part of this patch.

Run focused platform-neutral contracts, full cargo workspace all features, fmt and all-target/all-feature Clippy locally. Native implementation branches require actual Windows CI checks. No new Lean model is planned: a finite arithmetic proof of two per-root caps would not establish native Job Object configuration or behavior, while native readback and bounded process correspondence directly test this issue. Existing shutdown/resource oracles remain part of full validation.

## Primary sources

- https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information
- https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information
- https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs
- https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-queryinformationjobobject

These specify flags, job memory/accounting, nested parent constraints and NULL immediate-job lookup. Completion-port notifications are not a universal delivery proof; report the tested attribution outcomes without generalizing to every host/interleaving.

## Design self-review

1. Compared the ignored constructor, outer kill-only job, inner full ProcessLimits caps and core full-value requests with the issue's two permitted contract choices. Selected the existing per-root behavior and recorded aggregate cost explicitly.
2. Distinguished actual request values, native readback, committed memory, active process counts and simultaneous root barriers. Included healthy-sibling attribution and cleanup so a superficial help-only fix cannot satisfy acceptance.
3. Traced known Windows base failures and exact shared workflow assertions. Require independent native execution, no success through silent skips, no unrelated fixes and no duplicate #487 reporting API.

## Native CI refinement and API migration

The dedicated job also runs Windows Clippy on the library and explicitly selected affected integration targets. This checks changed cfg(windows) code while avoiding the unrelated workspace_recovery test import that blocks the existing all-target Windows quality job. Preserve the existing manual quality job and local all-target Clippy; do not weaken either assertion. Constructor removal requires checking cfg-gated test helpers/imports and the Windows shell selector for now-unused parameters.

Removing WindowsBackend::new's unused RunLimits argument deliberately changes that Rust source API; callers create the ownership backend without misleading limit values and supply limits on each actual process request. This migration is distinct from the independent #487 ResourceControl constructor change. No aggregate cap, report schema expansion or new production native-handle accessor is part of this design.

Three refinement checks:1 traced all WindowsBackend::new and test helper call sites;2 identified Windows-only unused imports/helpers that a macOS compile cannot check;3 retained exact existing CI contracts while adding scoped native quality plus non-skipping acceptance. Native execution remains required before final success claims.

## Direct interpreter correspondence

CPython3.14's Windows venv redirector creates its own kill-on-close Job Object, starts the base interpreter as a child, assigns that child to the new job, and waits. See https://github.com/python/cpython/blob/3.14/PC/venvlauncher.c, launch function. Using .venv/Scripts/python.exe can therefore add a live process and an inner job: QueryInformationJobObject(NULL) may describe the redirector's job rather than Hoimin's configured root job, and a nominal root-plus-child fixture may contain more than two native processes.

The scope/readback fixture must launch a verified direct base interpreter, resolved from the controlled .venv/pyvenv.cfg home/python.exe (the redirector's own resolution), from sys._base_executable outside the supervised test, or from the explicit setup-python base executable. Children use that direct interpreter's sys.executable. Validate paths and actual native counts; do not silently fall back to a launcher, raise the requested process cap or weaken the readback assertion. Existing corrected P=3 handler fixtures likewise need deliberate executable selection or honest wrapper accounting. Production command launching is unchanged; this is a correspondence requirement for the bounded native test.

Three evidence-led fixture reviews:1 checked CPython's actual CreateJobObject/CreateProcess/AssignProcessToJobObject/wait sequence;2 distinguished immediate-job readback from inherited parent enforcement and actual root-inclusive count;3 retained the chosen cap and exact occupancy assertion while isolating launcher machinery in the test fixture. Local source inspection is not counted as the eventual native acceptance result.

## Completed Windows acceptance

Implementation7e7d08e passed the dedicated [Manual Windows resource scope job](https://github.com/tokyogas-tech/hoimin/actions/runs/34611937206/job/103304326195) on Windows Server2025 (windows-2025-vs2026), Rust1.98.0 x86_64-pc-windows-msvc and CPython3.14.7 x64. Scoped native Clippy passed. Actual native tests:25 Job Object unit tests,5 process-handler tests and2 public CLI tests;32 passed,0 failed,0 ignored, with no capability/setup skips in the nocapture log.

The memory barrier observed two live roots, each with a live child, exact167772160-byte memory caps and100663296-byte retained payloads. Combined payload192MiB exceeded one160MiB cap; native peak job memory was111456256 and111927296 bytes. The process barrier observed2 active processes in each root job under cap3, for4 simultaneous processes overall. Both jobs reported native flags8712 and exact requested caps. The CLI assertions confirmed normalized configuration, baseline/mutant Exit0, two survivors, CLI exit1 and all retained root/child handles signaled after cleanup. Separate handler tests passed memory/process offender attribution, healthy sibling completion, timeout isolation and close/reject-new-spawn cleanup.

This proves the bounded native fixtures on that runner, not universal host/interleaving or notification delivery properties. First native run34611097571 exposed a Windows-only test helper compile error and was cancelled;7e7d08e corrected the helper and shared cleanup deadline before the successful second run. The second overall manual workflow still failed unchanged base checks: unused WorkspacePlan import in workspace_recovery, six Lean shell-fixture contract tests on Windows, and the target/git literal-backslash fixture (624 passed,1 failed,8 ignored before later integration targets). All macOS jobs and Windows core purity passed. Dedicated acceptance directly executed the relevant native targets despite those independent base failures.
