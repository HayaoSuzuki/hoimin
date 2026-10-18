# Task 7 implementation report (#972)

Base: cf210e5c0d6101b03396e603076eaa12e7b5d446. Completed in 787fc4f31d4d8d115386303425f789750fbcfad5 (`refactor(analyzer): fix session settings at construction (#972)`).

## Requirements/design reviews (completed before implementation)

1. Read brief/supplement and issue acceptance: constructor owns root, operators, profile and positive `NonZeroUsize`; no new settings framework, old API adapter or reconfiguration. Identified per-request limit divergence in store construction vs remaining-budget calculation.
2. Read approved discovery ownership handoff: Task6 timeout stays in `DiscoveryConfig` and complete-target discovery. Task7 changes only runtime handler requests; Task9 payload accounting excluded.
3. Traced ownership phases: `store: Option<CandidateStore>` is transfer state; pre-transfer cancellation/read error retains it; after `take()` worker/error/cancel cannot restore it. Worker spool-owner clone and lazy reader/store creation stay unchanged.
4. Inventoried current callers: shell setup/dispatch, analyzer internal cancellation/depth/lifetime tests, analyzer_handler integration helpers, bounded-discovery ordered-target adapter, core `analyze_next`. Existing baselines cover contiguous multi-file sequences and non-final truncation, but empty final target, fresh finalized session, constructor settings differences, accumulated read-error retention need explicit coverage.
5. Inspected serde boundary: `AnalyzeFile` is deserializable within exported `RunEffect`; removing only its field silently accepts old limit input under serde defaults. Add strict `deny_unknown_fields` with behavior regression test; no artifact version bump. Internal zero-prefix corpus remains below positive runtime handler constructor.

Design score before: 6.25/10 (5/8 diagnostics); failed simple interfaces, information hiding, newcomer-visible boundaries because each request repeats fixed settings. Target: 10/10 by moving the single source of truth into existing handler fields and documenting its session ownership; no extra abstraction.

## Verification evidence

All Cargo and hook processes used `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`; focused tests use `--offline --locked`, `--test-threads=2`. Parent explicitly owns final integrated checks and Task10 guide prose.

## Implementation reviews (five distinct passes)

1. Reviewed constructor and exported methods: fixed fields are private and `NonZeroUsize` excludes zero by type; no new settings wrapper, reconfigure method, repeated equality check, legacy overload or adapter. Constructor still only stores values.
2. Reviewed shell/core diff: shell copies fixed operators once when constructing its handler; dispatch sends request/cancellation only; `analyze_next` no longer copies any limit. `AnalyzeFile` now holds exactly id/target/final_target, rejects unknown fields, and does not trigger an external artifact schema change.
3. Traced all transfer paths against the original code: lazy store/reader initialization, exception project builder, `.take()`, error handling, success restoration and worker-held spool owner have unchanged placement. Worker clones owned operators because blocking work may outlive the borrow. Strengthened pre-transfer cancellation baseline with one accumulated candidate.
4. Read new integration assertions independently of production: initial run exposed two incorrect fixture expectations (5 default candidates instead of 1, and 10 instead of 2). Corrected fixture selection to explicit `binary_add_sub`; no production behavior changed to fit the test. Split normal-final and truncation restart into separately registered cases; both retain settings and reset sequence. Bounded property independently counts one candidate per equality.
5. Reviewed oracle boundary and comments: ordered-target handler limits convert to `NonZeroUsize`; the internal zero-prefix case remains outside this adapter; two terminal cases now have separate registered names. Handler comments describe fixed ownership instead of caller consistency. Discovery timeout code/config and Task9 payload accounting remain untouched.

## TDD and initial focused evidence

- RED behavior: `cargo test --offline --locked -p hoimin-core --test workspace_effects analysis_effect_rejects_obsolete_per_request_candidate_limit -- --test-threads=2` exited 101: `assertion failed: decoded.is_err()`, 0 passed/1 failed. Existing old request was accepted, demonstrating the missing strict boundary.
- RED API: `cargo test --offline --locked -p hoimin-cli --test analyzer_handler concrete_handler_restarts_finalized_sessions_with_the_same_fixed_settings -- --test-threads=2` exited 101: E0061 constructor expected 1 argument rather than fixed 4, handle expected 3 rather than request-only; E0063 request still required removed max_candidates. This is compile-time API evidence, not a claimed runtime failure.
- Initial GREEN attempt ran analyzer_handler: 27 passed/2 failed because new arithmetic fixtures used the much broader default selection; fixed fixtures to explicitly select one arithmetic operator, as recorded above.
- GREEN: `cargo test --offline --locked -p hoimin-cli --test analyzer_handler --test lean_bounded_candidate_discovery_oracle -- --test-threads=2` exited 0: analyzer_handler 30 passed; bounded oracle 5 passed, including actual complete-target and non-final truncation adapter execution. Property test uses 24 cases, 0..4 candidates per file, positive limits 1..6.

All commands above were prefixed with the four resource environment variables stated in Verification evidence. `cargo fmt --all` ran successfully under the same environment.

## Focused verification results

Every Cargo command below used `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0` and the worktree's shared `target`. No parallel Cargo process was launched.

- `cargo test --offline --locked -p hoimin-core --test machine --test workspace_effects -- --test-threads=2`: exit 0, machine 95/95 and workspace_effects 5/5, including the formerly red obsolete-limit rejection.
- `cargo test --offline --locked -p hoimin-cli --lib analyzer:: -- --test-threads=2`: exit 0, 323 passed, 0 failed, 3 ignored, 658 filtered out. Existing ignored release benchmark harnesses: `benchmark_candidate_line_positions`, `benchmark_long_single_line_candidate_columns`, `benchmark_adversarial_ast_fact_indexes`. Includes non-empty pre-transfer cancellation retention, depth-error handler reuse, detached worker spool-owner lifetime, shared whole-discovery deadline and existing internal bounded candidate fixture/oracle tests.
- `cargo test --offline --locked -p hoimin-cli --test exception_hierarchy -- --test-threads=2`: exit 0, 22/22, including `hierarchy_exclusion_and_import_roots_apply_to_index_and_fingerprint` and actual runtime `hierarchy_run_uses_same_candidates_and_mutants_change_python_behavior`.
- `cargo test --offline --locked -p hoimin-cli --test run_e2e shell_context_construction_performs_no_project_io -- --test-threads=2`: exit 0, 1/1; 71 unrelated cases filtered out.
- `git diff --check`: exit 0. Initial `target` 2.5 GiB, after focused tests 2.7 GiB; initial free space 118 GiB.

Total focused passing registered tests: 481 (30 handler + 5 bounded oracle + 95 machine + 5 effect + 323 analyzer units + 22 exception integration + 1 shell construction), plus 3 intentionally ignored benchmark cases. No failing tests remain in these commands. Final workspace-wide/integrated tests are delegated to the controller as explicitly requested, rather than claimed run here.

## Verification reviews (five distinct passes)

1. Compared core machine results to effect change: ordered multi-target discovery and final/truncated state transitions pass; strict serde regression now passes. No core API setting copy remains.
2. Compared handler and bounded-oracle outputs to required baselines: 30 handler tests and 5 corpus/adapter tests pass; new normal-final and truncated-final sessions restart with independent spool tokens/sequence one; empty final target returns prior candidates; property checks cross-file cap/truncation/sequences with only positive constructor limits.
3. Checked interruption evidence: analyzer unit suite passes non-empty pre-transfer retention, post-transfer depth-error release/reuse, cancellation returning while detached worker retains managed spool owner, and shared discovery timeout. Three skipped tests are existing release benchmarks, not skipped correctness coverage.
4. Checked exception/lazy-I/O consumers: 22 hierarchy integration tests preserve configured import roots and runtime discovery behavior, and the shell missing-project constructor test proves no project/session I/O at setup. `.with_exception_project` and managed owner builder placement are unchanged.
5. Re-read full production diff and all constructor/request references after test migration; no per-call/per-effect settings, compatibility forwarding, timeout ownership regression, extra dependencies or Task9 payload work. `git diff --check` passes. Hook results will be recorded below.

## Files changed

- `crates/hoimin-cli/src/analyzer/mod.rs`: fixed private settings, reduced handle methods, accurate session comments, migrated and strengthened cancellation tests.
- `crates/hoimin-cli/src/shell.rs`: construct handler from normalized config once; dispatch request/cancellation only.
- `crates/hoimin-core/src/effect.rs`: file intent only, strict serde fields.
- `crates/hoimin-core/src/machine.rs`: stop copying candidate limit into each effect.
- `crates/hoimin-cli/tests/analyzer_handler.rs`: migrate callers and add required behavioral coverage.
- `crates/hoimin-cli/tests/lean_bounded_candidate_discovery_oracle.rs`: positive fixed limits, separately registered terminal cases.
- `crates/hoimin-core/tests/workspace_effects.rs`: obsolete per-request limit rejection regression.

## Task10 guide follow-up (controller confirmed ownership)

- `docs/development.md:109` old constructor signature; `:114` caller duty to keep settings consistent. Replace with immutable constructor settings and same-settings fresh sessions.
- `docs/knowledge/design/analyzer.md:962` old constructor signature/initialization description; `:964` historical migration paragraph should be scoped to its historical issue; `:983-989` current caller-consistency/per-request limit explanation must reflect constructor ownership. Preserve dated evidence as historical.
- `docs/knowledge/design/runtime-lifecycle.md:383-396` shell-coordination entry point contains general lifetime guidance rather than an explicit old handler signature; reconcile with the new handler boundary while preserving detached-worker lifetime distinction and existing evidence scope.

No Lean model/corpus was changed; Rust correspondence adapters were executed, not inferred from proofs. No Windows host verification was performed.

## Pre-commit correction

The first authorized `git commit` attempt exited 1 with no commit created. Required workspace fmt, vendor fmt and vendor clippy hooks passed; workspace all-target/all-feature clippy rejected two newly introduced `assert!(vec.is_empty())` statements (`clippy::assert_is_empty`, analyzer_handler.rs:898/909). Root-cause review compared their assertion form with the existing equality-to-empty-Vec pattern; changed exactly those two assertions to `assert_eq!(..., Vec::<MutationCandidate>::new())` so failures show actual candidates. No production behavior changed and no lint allowance/hook bypass was added. `systematic-debugging` skill applied to this failure. Focused rerun and second hook attempt follow.

## Final hook evidence and commit

After the two assertion corrections, reran `cargo test --offline --locked -p hoimin-cli --test analyzer_handler -- --test-threads=2` under all four resource variables: exit 0, 30/30.

Authorized index mutations used `sandbox_permissions=require_escalated`: staged only the seven listed task files, then staged the two corrected assertions. No `.superpowers` file was force-added.

The second exact commit command was:

```sh
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true git commit -m 'refactor(analyzer): fix session settings at construction (#972)'
```

Exit 0. All four normal required hooks passed: workspace fmt; vendored-parser fmt; workspace all-target/all-feature Clippy; vendored-parser Clippy. Hooks were not bypassed. Commit `787fc4f31d4d8d115386303425f789750fbcfad5` contains exactly seven files, 448 insertions/196 deletions; no push. `git diff HEAD^ HEAD --check` exited 0, and `git status --short` produced no output after commit. The report remains ignored (confirmed by `git check-ignore`). Final target is 2.7 GiB and available storage remains 118 GiB.

## Completion reviews (five distinct passes)

1. Re-read task brief/supplement and issue acceptance against the committed implementation: fixed root/operators/profile/positive candidate limit, reduced effect/handle, fresh stores after finalization, unchanged interruption ownership, lazy I/O and preserved exception metadata all implemented. Every acceptance item maps to focused verification above.
2. Re-read final constructor/handle/effect/shell source after commit: private settings have one source of truth; store cap and remaining-budget calculation share `self.max_candidates`; worker ownership clone is still needed and retained. There is no forwarding old API or new state/settings framework. Final scoped design score: 10/10 (8/8 diagnostics); previously failed simple-interface, information-hiding and boundary-understanding diagnostics now pass.
3. Rechecked lifetime boundaries in committed diff: `Option<CandidateStore>` ownership transfer, worker owner capture, `.with_exception_project`, lazy reader and failure restoration conditions retain their original order. Tests establish pre-transfer state retention, post-transfer failures/reuse and detached owner lifetime; cancellation return is explicitly not worker exit.
4. Audited evidence for overclaims: all 481 focused registered correctness tests pass; three ignored benchmark cases are named; actual Rust bounded-discovery correspondence ran; no new Lean proofs, native Linux/Windows tests, or full integrated workspace suite are claimed. First red serde test, API compile-red, fixture mistakes, and first hook failure are recorded rather than omitted. Only assertion form changed after the main tests, with relevant 30-test rerun and full all-target/all-feature Clippy succeeding afterward.
5. Reviewed commit scope and handoff: exactly the seven authorized source/test files committed; working tree clean; ignored full report available; no push, dependency install, `.venv` write, main target mutation or concurrent Cargo. Task10 owns the explicitly listed guide reconciliation, and controller owns final integrated validation. No outstanding correctness concern found. Cargo slot is released after this report write.

Status: DONE. Limitations: verification executed on the supplied macOS host only; native Linux/Windows coverage and final integrated checks remain with the controller.
