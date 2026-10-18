# Task 6 report

## Requirements/design reviews (five distinct passes)
1. Ownership: actual Clap declaration must move to plan; shared raw normalization remains singular. Explicit tuple is sufficient; no generic wrapper.
2. Persistence: nested limits need deny_unknown_fields; discovery required; old schema headers checked before fields. Historical fixtures unchanged.
3. Deadline: preserve complete-target timer, finished-at comparison and detached resource ownership; positive bounded discovery constructor and semantic validation.
4. Identity/version: 10/8/12, session 5, candidate 1/ranking 4 unchanged; floor 0.12.0. Old incomplete fingerprint errors remain explicit.
5. Secondary boundaries: raw-plan Lean adapter must combine normalization, corpus/proofs actually checked, fuzz unsupported 999, golden consumers/current schemas and five version files migrated.

Design baseline local score 5/10 per approved handoff; task 6 addresses discovery ownership, strict boundaries and execution identity. Fixed analyzer settings belong to task 7.

## Implementation self-reviews (five distinct passes)
1. Owner/API review: one shared mutation normalizer retained; plan-only option moves to RawPlanArgs; into_configs tuple and explicit create/discover parameters. VerifiedPlan remains execution-only. Improved CLI conversion docs.
2. Saved-boundary review: both raw/normalized runtime limits reject unknown keys; saved discovery required and positive, with bounded semantic validation. Added missing/null/unknown/zero/oversized and obsolete-field cases.
3. Identity review: runtime encoder removes only analyzer duration; retained operational-irrelevance and verdict-sensitivity tests. Existing schema-11 goldens must now differ, not be rewritten as compatibility promises. New two-plan identity test compares schema-12 fingerprints and candidates.
4. Caller/fixture review: compiler caught oracle fields and core-internal crate path; fixed them. Diff review caught a script deleting neighboring test mutations; restored all unrelated mutation functions. Migrated review fixtures as well as integration consumers. Preserved historical fixture bytes, added schema-7 rejection.
5. Formal/documentation review: renamed Lean Input/accepts to PlanInput/acceptsPlan; mode plan is generated, not hand-edited. Targeted model/proofs/cases checked, corpus generated/check/sensitivity guarded. Updated current docs and superseding compatibility notice; historical notices retained. Task 10 owns OKF reconciliation.

## Verification self-reviews (five distinct passes)
1. RED/GREEN audit: initial run flag test failed because run accepted the flag; nested runtime JSON test failed because obsolete timeout was accepted. Deserialized oversized create test failed with project-resolution error before the early validation fix, then passed. Logs retain exact failures. Existing create/verify timeout tests were adapted and retained; their behavior was already implemented before this task.
2. Deadline/lifetime audit: 11 analyzer unit tests pass, including new two-target shared-deadline case, entry-armed deadline under worker queueing, and detached resource retention/recovery. The two-target hook is cfg(test) only; it calls once per analyzed target and arms only once. No parser preemption/RSS enforcement is claimed.
3. Saved formats/identity audit: plan 105 and report 36 pass; schema-9 plans and historical schema-2–7 reports reject. Runtime report comparisons exclude discovery. Missing/null/zero/unknown/oversized discovery and obsolete runtime keys are covered. Two adequate discovery settings preserve ordered candidate values/IDs and current execution fingerprints, while saved bytes differ. Exact schema-12 hashes for untracked/captured fixtures are pinned in resume_policy, with schema-11 historical hash vectors separately retained as incompatible. Bounded canonical-edit properties and individual verdict/operational policies remain passing.
4. Session and corpus audit: 47 session tests pass (1 preexisting ignored). Current SQLite DDL/payload equals schema-5 fixture storage apart from explicitly asserted fingerprint version 11 versus 12. Previous fingerprint rejection leaves DB schema 5 and rows unchanged. Lean plan-scoped 15-case corpus is generated/check/sensitivity verified, and all 4 Rust oracle tests pass. A matching mock fingerprint in older fixtures is still governed by the existing matching-history branch; no blanket unreadable/fresh-fallback claim.
5. Verification scope audit: all named final failures were resolved by updating affected contracts/fixtures; no production expectation was weakened. Core-current fixture consumers and CLI review/artifact consumers were discovered by full source search. Python schema suites pass 15 + 49 tests and release contracts pass 8. Native Windows argv/process execution was not available on this macOS host; Windows-only argv/process branches were neither compiled for a Windows target nor executed. Unix/macOS native-argument coverage passed. Final integrated broad suites remain controller-owned. No main-worktree Lean cache link/write, no dependency installation, no unrelated Django fact-index fix.

## Implementation and coverage notes
- Added core DiscoveryConfig (positive duration, existing MAX_TIMEOUT, strict required serde fields), PlanConfig.discovery, explicit plan creation/discovery parameter and early create validation. PlanArgs.into_configs retains one common raw mutation normalizer. VerifiedPlan and runtime serialization contain no discovery option. Fingerprint encoding removes only analyzer timeout.
- Versions: plan 10, report/event 8, fingerprint 12; SQLite session 5, ranking 4, candidate 1 and mutant-ID v1 unchanged. Own package metadata and five Cargo/Python lock/version files agree on 0.12.0.
- New fixtures: crates/hoimin-cli/tests/golden/reports/schema-v8-current.json; schema-v8-original.json; crates/hoimin-cli/tests/golden/events/schema-v8-current.jsonl; schema-v8-original.jsonl. All 30 previously tracked golden files (report/event/session) compared byte-for-byte against HEAD: zero changed. See task-6-fixture-version-audit.log.
- Current JSON Schemas reject run discovery and obsolete limits.analyzer_timeout, while progress's opaque config contract remains unchanged. Verify-preview's plan_schema_version const is 10; its own schema stays 2. Fuzz seeds use current 8 for valid-intent cases and unsupported 999; fuzz execution itself was not performed.
- Current usage/development/compatibility and generated CLI reference updated; historical notices retained. Task 10 owns OKF reconciliation.
- Local design score after Task 6: 8.75/10 (7/8 diagnostics): explicit discovery/config boundaries, strict saved inputs and execution-only fingerprints address this task. Constructor-fixed handler settings (remaining interface-simplicity diagnostic) belong to Task 7; no speculative framework introduced.

## Command/evidence ledger
All Cargo and commit commands inherit CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0. Cargo commands below use --offline --locked, and test runs use -- --test-threads=2. Working directory is the issue worktree. Logs are under /tmp/hoimin-design-audit-5d3aee95/ with task-6- prefixes.

- RED: cargo test -p hoimin-cli --test cli_config run_rejects_plan_only_analyzer_timeout (red-cli.log): expected accepted-flag failure. cargo test -p hoimin-core --test config_json runtime_limits_reject_obsolete_discovery_timeout (red-config.log): expected obsolete-field acceptance failure.
- RED: cargo test -p hoimin-cli --lib create_rejects_oversized_saved_discovery_before_project_work (red-create-boundary.log): expected project-access-before-validation failure. GREEN included in final plan unit run.
- cargo check --workspace --tests (check.log): final successful check after constructor migration; later mandatory all-target hook checks cover final edits.
- cargo test -p hoimin-core --test config_json --test plan_config --test target_policy --test resume_policy --test lean_timeout_limit_oracle --test changed_selection --test operator_selection (core.log): first five suites pass, then expected schema-11 golden failure stops remaining suite. Changed tests followed by cargo test -p hoimin-core --test resume_policy --test target_policy --test mutation_outcome (core-followup.log): all pass. Frozen schema-12 captured-vector recording failure is fingerprint-record.log, not a production regression or passing test count.
- cargo test -p hoimin-cli --test cli_config --test plan --test report_handler --test session_handler (cli.log): config passes; plan comparison still included discovery, fixed and rerun. cargo test -p hoimin-cli --test plan --test report_handler --test session_handler (cli-followup.log): plan passes; preview schema const 9 caused two report failures. cargo test -p hoimin-cli --test report_handler --test session_handler (report-session.log): report passes; historical session fixture expected current fingerprint version and was corrected without editing fixture bytes.
- cargo test -p hoimin-cli --test session_handler --test progress --test lean_progress_input_oracle --test lean_progress_decision_oracle --test fingerprint_env_plan --test import_roots --test target_handler --test lean_exception_hierarchy_oracle (secondary.log): all pass after session fixture assertions updated.
- cargo test -p hoimin-cli --lib plan:: (plan-unit.log): 58 pass/3 ignored; rerun after removing misleading unused timeout parameter from shared test Project helper (plan-unit-final.log): same 58/3. The first review/artifacts/progress unit attempts failed compilation because nested test helper call sites still passed the old argument; fixed all seven call sites. Subsequent review-unit-final.log: 78 pass. artifacts-unit-final.log: old duration duplicate fixture searched for removed analyzer secs=30; changed duplicate-key probe to baseline secs=60. artifacts-unit-green.log: 12 pass. progress-unit-final.log: 2 pass.
- cargo test -p hoimin-cli --lib analyzer::tests:: (analyzer-unit.log): 11 pass after multi-target control/test addition. cargo test -p hoimin-core --lib fingerprint_env:: (env-unit.log): 4 pass.
- cargo run -p hoimin-cli --example generate_cli_reference (cli-reference.log) generated docs; -- --check (cli-reference-check.log) passes; cargo test -p hoimin-cli --example generate_cli_reference (cli-reference-test.log): 7 pass. cli_config suite includes native Unix argv, help and completions tests.
- .venv/bin/python -m pytest -q tests/test_session_mode_schema.py tests/test_changed_selection_schema.py (python-schema.log): 15 pass; other schema suites mutation_outcome/environment_snapshot/review_reference/verification_selection (python-other-schema.log): 49 pass. test_release_version_contracts.py (release-tests.log): 8 pass. Ruff on the two edited Python test files (python-lint.log): passes.
- Lean: python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats <task-6-lean-*-stats.json> -- <command>. Commands serially: lake build HoiminOracle.TimeoutLimitModel HoiminOracle.TimeoutLimitProofs HoiminOracle.TimeoutLimitCases; lake env lean --run TimeoutLimitAuditMain.lean --output corpus/timeout-limit.jsonl; same --check corpus/timeout-limit.jsonl; same --sensitivity. All exit 0; initial sandbox process sampling was unavailable (exit 126), then authorized escalation allowed ps. Own .lake build cache only. Aggregate measured peaks: build 673984 KiB/4107 ms; output 652240 KiB/1940 ms; check 675392 KiB/2221 ms; sensitivity 679152 KiB/575 ms. No whole-library build or inferred proof success.

### Consolidated observed suite counts (includes explicitly labeled reruns)

```text
Unique Rust test identities with passing evidence: 755; ignored: 5
core: core/changed_selection: test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
core: core/config_json: test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
core: core/lean_timeout_limit_oracle: test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
core: core/operator_selection: test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
core: core/plan_config: test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
core: core/resume_policy: test result: FAILED. 24 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
core-followup: core/mutation_outcome: test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
core-followup: core/resume_policy: test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
core-followup: core/target_policy: test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
cli: cli/cli_config: test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s
cli: cli/plan: test result: FAILED. 104 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 8.85s
cli-followup: cli/plan: test result: ok. 105 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 9.30s
cli-followup: cli/report_handler: test result: FAILED. 34 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.53s
report-session: cli/report_handler: test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.79s
report-session: cli/session_handler: test result: FAILED. 45 passed; 2 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.56s
secondary: cli/fingerprint_env_plan: test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.61s
secondary: cli/import_roots: test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
secondary: cli/lean_exception_hierarchy_oracle: test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.56s
secondary: cli/lean_progress_decision_oracle: test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s
secondary: cli/lean_progress_input_oracle: test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.89s
secondary: cli/progress: test result: ok. 99 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.65s
secondary: cli/session_handler: test result: ok. 47 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.54s
secondary: cli/target_handler: test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.57s
plan-unit-final: cli/lib: test result: ok. 58 passed; 0 failed; 3 ignored; 0 measured; 923 filtered out; finished in 0.12s
analyzer-unit: cli/lib: test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 973 filtered out; finished in 1.10s
review-unit-final: cli/lib: test result: ok. 78 passed; 0 failed; 0 ignored; 0 measured; 906 filtered out; finished in 1.37s
artifacts-unit-green: cli/lib: test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 972 filtered out; finished in 0.09s
progress-unit-final: cli/lib: test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 982 filtered out; finished in 0.00s
env-unit: core/lib: test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 0.00s
cli-reference-test: cli/example:generate_cli_reference: test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Unique passing identities above deduplicate package/suite/test names across initial attempts and focused reruns; they are not a claim that an integrated full suite was run. Final semantic production edit was early discovery validation; subsequent changes were fixture/test migration, test helper cleanup, documentation, and formatting.

## Completion self-reviews (five distinct passes)
1. Requirements audit: actual run Clap rejection, plan-only default/validation, saved explicit discovery and verify reuse, runtime serialization/fingerprint removal, positive bounded core type and strict nested decoding all traced to implementation and behavioral evidence. No compatibility defaults/aliases or runtime timeout introduced.
2. Change-scope audit: inspected production diff; deadline/finished-at/cancellation ownership unchanged outside cfg(test) control. Session schema, ranking and candidate identity domains untouched. No unrelated Django fact-index changes. The changed helper signature removes an ignored plan timeout argument instead of perpetuating no-op test setup.
3. Secondary-interface audit: current report/event fixtures, review/artifact/core current-fixture consumers, preview schema's plan reference, fuzz seed schema collision, own package versions, generated reference and current user/developer docs covered. Historical fixtures byte-equal; historical notices retained. OKF is controller Task 10.
4. Evidence audit: 755 unique Rust test identities have passing evidence, with five existing ignored cases; 72 Python tests pass. Initial/focused failures and reruns are named above, rather than counted as fresh independent tests. Lean proof/model/case checks and output/check/sensitivity actually executed with recorded wall/RSS statistics. Windows native runtime and integrated whole-workspace test execution remain explicit scope limitations.
5. Final commit gate: diff whitespace check passes; report is ignored and will not be staged. All four mandatory enabled hooks will run on the commit with the required Cargo environment. Final commit/hash and actual hook outcome are appended below after the commit attempt.

First commit attempt: three hooks passed; workspace Clippy rejected unqualified Default::default in the new identity test (default_trait_access). Replaced it with hoimin_core::OutputConfig::default; behavior unchanged. No test rerun added solely for this name qualification. Mandatory hooks are rerun by the next commit attempt. See task-6-commit.log for the first result.

## Final completion evidence

Status: DONE. Commit: 32f50de06a3828f8f9cf00bb961adcfd2e1c2639 — fix(config): make discovery timeout plan-owned (#979).
Final mandatory hooks all passed: workspace fmt, vendored parser fmt, workspace all-target/all-feature Clippy with warnings denied, vendored parser Clippy. See task-6-commit-final.log. The final qualification-only Clippy correction changed no behavior; there was no extra passing-test rerun.
Post-commit git status --porcelain is empty: True. Report remains ignored and is absent from the commit; no forced add, push, main-worktree cache change or dependency installation. Cargo slot released.

### Files changed in the commit

```text
Cargo.lock
Cargo.toml
crates/hoimin-cli/src/analyzer/mod.rs
crates/hoimin-cli/src/artifacts/run_report.rs
crates/hoimin-cli/src/cli.rs
crates/hoimin-cli/src/lib.rs
crates/hoimin-cli/src/plan.rs
crates/hoimin-cli/src/plan/review_tests.rs
crates/hoimin-cli/src/plan/validation_tests.rs
crates/hoimin-cli/src/progress/compare.rs
crates/hoimin-cli/src/review/tests.rs
crates/hoimin-cli/tests/cli_config.rs
crates/hoimin-cli/tests/exception_hierarchy.rs
crates/hoimin-cli/tests/exception_hierarchy_scopes.rs
crates/hoimin-cli/tests/fingerprint_env_plan.rs
crates/hoimin-cli/tests/golden/events/schema-v8-current.jsonl
crates/hoimin-cli/tests/golden/events/schema-v8-original.jsonl
crates/hoimin-cli/tests/golden/reports/schema-v8-current.json
crates/hoimin-cli/tests/golden/reports/schema-v8-original.json
crates/hoimin-cli/tests/import_roots.rs
crates/hoimin-cli/tests/lean_exception_hierarchy_oracle.rs
crates/hoimin-cli/tests/lean_progress_decision_oracle.rs
crates/hoimin-cli/tests/lean_progress_input_oracle.rs
crates/hoimin-cli/tests/plan.rs
crates/hoimin-cli/tests/progress.rs
crates/hoimin-cli/tests/progress_compare_heap.rs
crates/hoimin-cli/tests/progress_details_heap.rs
crates/hoimin-cli/tests/progress_heap.rs
crates/hoimin-cli/tests/progress_jsonl_heap.rs
crates/hoimin-cli/tests/ranking_heap.rs
crates/hoimin-cli/tests/report_handler.rs
crates/hoimin-cli/tests/session_handler.rs
crates/hoimin-cli/tests/target_handler.rs
crates/hoimin-core/src/config.rs
crates/hoimin-core/src/fingerprint_env.rs
crates/hoimin-core/src/report.rs
crates/hoimin-core/src/resume.rs
crates/hoimin-core/tests/changed_selection.rs
crates/hoimin-core/tests/config_json.rs
crates/hoimin-core/tests/lean_timeout_limit_oracle.rs
crates/hoimin-core/tests/mutation_outcome.rs
crates/hoimin-core/tests/operator_selection.rs
crates/hoimin-core/tests/plan_config.rs
crates/hoimin-core/tests/resume_policy.rs
crates/hoimin-core/tests/target_policy.rs
docs/cli-reference.md
docs/compatibility.md
docs/development.md
docs/json-schema/run-event.schema.json
docs/json-schema/run-result.schema.json
docs/json-schema/verify-preview.schema.json
docs/usage.md
formal/HoiminOracle/HoiminOracle/TimeoutLimitCases.lean
formal/HoiminOracle/HoiminOracle/TimeoutLimitModel.lean
formal/HoiminOracle/HoiminOracle/TimeoutLimitProofs.lean
formal/HoiminOracle/TimeoutLimitAuditMain.lean
formal/HoiminOracle/corpus/timeout-limit.jsonl
fuzz/Cargo.lock
fuzz/seeds/report_sequence/diagnostic-before-run
fuzz/seeds/report_sequence/diagnostic-run-id-mismatch
fuzz/seeds/report_sequence/run-started
fuzz/seeds/report_sequence/run-with-diagnostic
fuzz/seeds/report_sequence/unsupported-schema
pyproject.toml
tests/test_changed_selection_schema.py
tests/test_session_mode_schema.py
uv.lock
```

## Task 6 fix round 1 — review resolution

Base: 32f50de06a3828f8f9cf00bb961adcfd2e1c2639. Read task-6-review.md verbatim. No diagnostic-priority production change: the controller ruling accepts ordering between independently invalid options; selector/argv requirements, individual flag diagnostics and pre-I/O discovery validation remain unchanged.

### Findings mapped to changes
1. docs/development.md now distinguishes unchanged field-13 environment framing from the deliberately incompatible complete schema-12 execution fingerprint. Historical schema-11 vectors assert difference; current schema-12 vectors pin bytes. Corrected adjacent SQLite storage schema from 4 to 5.
2. Replaced three finite loops with seven separately registered tests: run flag separate tokens / equals spelling; malformed / zero / oversized discovery duration; untracked / captured schema-12 hash. Each instance exercises one named scenario, uses existing helpers/AAA structure and reports its registered variant on failure. Exact hash values are unchanged. No dependency added.
3. Reworked only the shared-deadline test. Tokio's existing test-util supports a paused clock. A channel handshake holds the worker at each target; yield-based entry waiting does not advance virtual time. Advance 750 ms after target one enters, release it and wait for target two with time frozen, then explicitly advance the remaining 250 ms. The actual discovery timer must expire with target two still paused; resetting it would leave 750 ms outstanding. Five-second real channel receives serve only as deadlock watchdogs, replacing the former 250-ms successful-setup scheduling margin. Production discovery timer, finished-at comparison and control implementation are untouched.

### Five focused self-review passes
1. Contract reading: checked each review finding against current source and the controller's accepted diagnostic-ordering ruling. All three findings are localized; no compatibility layer, normalization-order change, new dependency or broader redesign is needed.
2. Documentation consistency: compared the corrected paragraph with FINGERPRINT_SCHEMA_VERSION=12, SQLite schema 5 and both historical/current vector tests. It no longer promises old whole-fingerprint bytes.
3. Scenario granularity: inspected registered test names and focused output: two spellings, three invalid-duration categories and two environment states each run independently. Shared helpers contain the Arrange/Act/Assert phases; hash vectors remain unchanged and separate.
4. Clock/lifecycle correctness: examined the first paused-clock attempt's failure before changing it. Blocking worker activity prevents Tokio automatic clock advancement, so an awaited virtual timeout alone left the worker waiting until its real watchdog fired. Replaced reliance on automatic advancement with explicit remaining-time advancement. The corrected single test passes in 0.01 s. No production path or deadline comparison changed.
5. Scope/evidence completion: diff contains only the requested documentation and three test sources. Eight covering test instances pass; broad suites, Python suites and Lean proofs were not rerun, and their existing 755-test baseline evidence remains separately recorded. Whitespace and formatting checked. Mandatory enabled commit hooks and final commit/clean status are recorded below.

### Exact verification commands and results
All commands ran in the issue worktree with CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0. Logs: /tmp/hoimin-design-audit-5d3aee95/task-6-fix1-*.log.

- cargo fmt --all — exit 0 (fmt.log).
- cargo test --offline --locked -p hoimin-cli --test cli_config run_rejects_plan_only_analyzer_timeout -- --test-threads=2 — 2 passed, 0 failed, 80 filtered out (cli-spellings.log).
- cargo test --offline --locked -p hoimin-cli --test cli_config discovery_timeout_with_flag_attribution -- --test-threads=2 — 3 passed, 0 failed, 79 filtered out (cli-invalid.log).
- cargo test --offline --locked -p hoimin-core --test resume_policy schema_12_execution_fingerprint -- --test-threads=2 — 2 passed, 0 failed, 25 filtered out (fingerprint.log).
- cargo test --offline --locked -p hoimin-cli --lib analyzer::tests::discovery_deadline_is_shared_across_targets -- --test-threads=2 — first paused-clock attempt failed because virtual time was not automatically advancing while a blocking worker remained active; worker recv_timeout guard expired (deadline.log). Following explicit advance, 1 passed, 0 failed, 983 filtered out, 0.01 s test duration (deadline-green.log).

These are eight focused passing instances, with only the failed deadline test rerun after its correction. They are not added to the old 755 as independent broad-suite coverage. Native Windows and integrated-suite limitations remain unchanged.

Fix round 1 completed: commit cf210e5c0d6101b03396e603076eaa12e7b5d446 — test(config): name timeout cases and control discovery clock. All four mandatory hooks passed (workspace/vendor fmt and workspace/vendor Clippy); commit log task-6-fix1-commit.log. Post-commit git status --porcelain empty: True. Only the four requested files committed; ignored report not staged. Cargo slot released.
