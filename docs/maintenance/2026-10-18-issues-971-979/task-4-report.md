# Task 4 / Issue 973 implementation evidence

## Scope and authorization

Read task-4-brief.md and /tmp/hoimin-design-audit-5d3aee95/impl-973.json first. Approved pure refactor: remove overwritten public constructor and helper branch, retain preflight-owned fingerprint state, update only the current constructor listing in docs/development.md. No compatibility wrapper, saved format, fingerprint algorithm or reuse policy change. Parent explicitly permits characterization-first tests passing before deletion and focused checks rather than repeated full suites. TDD and verification skills and writing-good-tests reference read; these task-specific instructions govern characterization and verification scope.

## Requirements/design self-reviews (five distinct passes)

1. API/caller review: machine.rs has one with_fingerprint constructor, tests/machine.rs one helper caller, development.md one current listing. Only those three tracked files need changes; historical records remain evidence.
2. Data-flow review: PreflightCompleted unconditionally assigns state.fingerprint before matching SessionMode; fresh reads it in begin_session_effects, resume reads it for LoadSession, SessionLoaded(Fresh) reuses it through begin_session_effects. Removing the field would break the fallback requirement.
3. Coverage review: existing session_load_outcomes_preserve_reasons_ids_and_completion_validation checks IDs/reasons/completions but no fingerprint payload. MissingFingerprint has no direct machine test. Existing helpers supply constructor and preflight the identical [7; 32], hiding constructor-only sourcing faults.
4. Test-design review: add bounded arbitrary 32-byte properties for fresh BeginSession and resume LoadSession/Fresh fallback; missing-evidence test uses named fresh/resume cases; disabled-evidence test checks progress without session effects; stopped preflight coverage reuses and strengthens the existing deadline-before-preflight test across named disabled/fresh/resume modes, inspecting all emitted effects; existing stop_cases coverage still checks all causes. Each test has one scenario and Arrange/Act/Assert sections. Wrong argument/default fingerprint, lost fallback state, removed missing-evidence validation or premature session dispatch must fail consumer-visible assertions. No grep/symbol tests.
5. Scope/risk review: extract the smallest pending-preflight helper so new tests control evidence and existing helper keeps its behavior. Characterize before deleting the constructor, then remove branch and constructor together. No new builder/API, no broad machine-suite rewrite; current session-report and runtime-lifecycle contracts reviewed for retained ownership and shutdown boundaries. Focused machine/session/contracts checks plus hook fmt/clippy are sufficient here; controller owns Task 10 integrated suite/docs/OKF.

## Operational notes

A read-only shell inspection initially used zsh special variable path in a loop, causing subsequent commands in that one subprocess to be unavailable. Re-ran with agent_file; no repository mutation or persistent environment change resulted.

## Characterization before production deletion

- cargo test --offline --locked -p hoimin-core --test machine -- --test-threads=2: 95 passed, no failures (task-4-characterization-machine.log). Production constructor still present; extracted pending helper deliberately retained its old constructor branch for this run.
- Strengthened disabled test to acknowledge RunStarted and assert actual worker materialization; cargo test --offline --locked -p hoimin-core --test machine preflight -- --test-threads=2: 5 passed (task-4-characterization-preflight.log), still before production deletion.
- The first fmt --check reported only formatting differences; cargo fmt --all applied them before test execution (task-4-characterization-fmt.log). This was not behavioral RED. No behavioral failure is claimed or manufactured for this pure refactor.

## Implementation self-reviews (five distinct passes)

1. Production delta review: removed exactly the with_fingerprint method (constructor plus assignment). RunState::new still initializes None; PreflightCompleted still assigns completion evidence, and both session consumers retain MissingFingerprint validation.
2. Helper review: pending_preflight_with now constructs only RunState::new, returning the real completion payload for test control. waiting_for_preflight_with remains the existing completion-driving entry point. Shared fixture_session_config contains only config setup; no new production test utility or compatibility builder.
3. Positive path review: two 64-case bounded property tests consume arbitrary full 32-byte fingerprints, with explicit effect payload comparisons. Fallback covers all six existing Fresh reasons with named cases, and checks both the initial LoadSession and subsequent BeginSession.
4. Negative/boundary review: enabled-missing test checks the concrete MissingFingerprint variant for named Fresh and Resume modes. Disabled test acknowledges run output and checks nonempty CreateWorker effects/Copy phase. Existing deadline test now traverses disabled/fresh/resume config and checks its complete observed effect stream for no begin/load/worker before preflight; retained reporting/cleanup assertions still apply.
5. Documentation/scope review: current development constructor listing drops only with_fingerprint. Session/report contracts, historical audit records, algorithm, DB schema, reuse conditions, unrelated code and controller plan/ledger are untouched. The expected production changes that tests catch remain wrong fingerprints, lost retained fallback evidence, ignored required evidence and premature session access.

## Verification self-reviews (five distinct passes)

1. Baseline/after comparison: characterization machine run passed 95 tests with the constructor still present; strengthened preflight subset passed 5 before deletion. After deletion, the contracts-enabled machine suite passed the same 95 tests, preserving existing outcomes and adding concrete payload/error assertions. Logs are direct command outputs, not reconstructed summaries.
2. Related-contract review: post-change --offline --locked cargo test selected machine, session_mode, resume_policy, report_resume, contracts with --features contracts and -- --test-threads=2. All selected suites completed successfully; per-suite counts recorded below from their complete log. This checks required runtime preconditions plus existing normalization/reuse/report contracts, without claiming a new formal proof or DB format change.
3. Feature-mode review: post-change default-feature preflight subset explicitly rerun to check the five affected characterization/boundary scenarios independently of contract instrumentation (task-4-final-preflight.log). Both property tests use 64 bounded arbitrary byte arrays; resume fallback traverses six named reasons per generated fingerprint.
4. API/source review: current source/documentation search confirms no RunState::with_fingerprint definition/call/listing; unrelated read_report_with_fingerprint identifiers remain correctly untouched. Fingerprint field, None initializer, preflight assignment, BeginSession and LoadSession reads all remain. Symbol search is a manual audit, not a test.
5. Hygiene/scope review: cargo fmt --all -- --check passed with empty output (task-4-fmt.log); git diff --check passed. Tracked diff is restricted to machine production source, machine integration tests and one development.md listing. No lockfile, saved schema, fixture corpus, knowledge/OKF, dependency or historical-record change. Mandatory repository hooks remain enabled for commit; controller owns integrated Task 10 verification.

### Contracts-enabled suite evidence

```text
     Running tests/contracts.rs (target/debug/deps/contracts-955c40b82e68d971)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/machine.rs (target/debug/deps/machine-79a1d297aee8a078)
test result: ok. 95 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
     Running tests/report_resume.rs (target/debug/deps/report_resume-cc465b514c7328e2)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/resume_policy.rs (target/debug/deps/resume_policy-41cc014efcc9db66)
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running tests/session_mode.rs (target/debug/deps/session_mode-ddbf3ac8e308a3fa)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
```

### Hook diagnosis and correction

The first commit attempt could not create .git/worktrees/.../index.lock in the sandbox (task-4-commit.log). Retried the same authorized commit with filesystem escalation, retaining all hooks and resource bounds. Automatic review allowed it; there was no review rejection.

The first hook run passed workspace/vendor fmt and vendor clippy but rejected one new assertion at tests/machine.rs:2578 under clippy::assert_is_empty (task-4-commit-hooks.log). Read systematic-debugging skill and full diagnostic, traced it to the new nonempty-vector assertion, and compared the existing Vec::new assertion style. Hypothesis: assert_ne!(effects, Vec::new()) preserves the nonempty contract while satisfying the diagnostic's requirement to display the value on failure. Applied this single test-expression correction; no production/behavior change. Re-run affected preflight tests and all mandatory hooks below. This lint failure is recorded explicitly, not described as behavioral RED.

A retry initially still had the old staged assertion because non-escalated git add could not create the shared index lock. prek correctly stashed/restored the unstaged fix and reported the same lint against the old index (task-4-commit-hooks-final.log); this was a staging mistake, not a new code failure. Explicit escalated git add then succeeded, git diff --cached showed assert_ne!, and the next commit's four hooks all passed. No hooks were skipped.

## Completion self-reviews (five distinct passes)

1. Acceptance reconciliation: meaningless constructor and helper branch gone; actual fingerprint retained; arbitrary fresh BeginSession payload exact; resume LoadSession and every Fresh fallback BeginSession use the same evidence; both enabled modes reject absent evidence; disabled mode reaches worker materialization without evidence; preflight-before-deadline stop never emits session requests across disabled/fresh/resume. Each acceptance item has a concrete source check or behavioral test.
2. Committed artifact review: git show --name-only HEAD contains only crates/hoimin-core/src/machine.rs, crates/hoimin-core/tests/machine.rs and docs/development.md. Commit 4e99249c5158462ddbbafbf45afc2310256ea4e5 is isolated on the supplied worktree/base. No controller plan/ledger or other task implementation was edited.
3. Final verification review: after the final assertion correction, default-feature preflight subset passed 5/5 (task-4-final-preflight-after-lint.log). Contracts-enabled focused suites passed 133 total (95 machine, 25 resume_policy, 6 session_mode, 4 report_resume, 3 contracts); that broader run preceded the assertion spelling-only correction. Mandatory commit hooks then passed workspace fmt, vendor fmt, workspace all-targets/all-features clippy and vendor clippy (task-4-commit-hooks-success.log). Initial lint/staging failures and recovery are retained above.
4. Compatibility/evidence review: git diff base..HEAD -- Cargo.lock is empty. Cargo test commands used --offline --locked; hook Cargo invocations are the repository's configured commands, unchanged, with CARGO_NET_OFFLINE=true. Every Cargo/commit invocation had CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0. Worktree .venv remains the existing ignored symlink and was not modified. No source-symbol assertion, manufactured behavioral RED or additional formal guarantee was introduced.
5. Handoff/hygiene review: post-commit git status --porcelain=v1 is empty and git diff HEAD --check reports no errors. Broad exact-word docs/crates search found only the controller plan and two historical Issue 487 records; these historical records are intentionally preserved. This ignored report is not tracked/force-added. Task 10 integrated full-suite and knowledge/OKF review remain controller responsibilities, not unresolved work within Task 4.

## Outcome

DONE — 4e99249c5158462ddbbafbf45afc2310256ea4e5. No outstanding task-specific concerns. Direct logs: /tmp/hoimin-design-audit-5d3aee95/task-4-*.log. Report remains ignored for controller archival.
