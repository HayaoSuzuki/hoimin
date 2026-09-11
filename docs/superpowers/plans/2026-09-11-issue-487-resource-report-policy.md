# Issue 487 implementation plan

Spec: ../specs/2026-09-11-issue-487-resource-report-policy-design.md

Independent base4adf809. Controller owns docs/superpowers and docs/knowledge; one architecture implementer owns Rust, tests and current documentation. Do not merge, delegate or run cargo-mutants. Every Cargo invocation is serial with `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`. Use existing untracked .venv CPython3.14.7 and /private/tmp/issue487-* logs.

## Plan self-review

1. Use actual fresh and naturally incomplete resumed execution; existing tests that hand-edit complete flags cannot supply the requested reproduction.
2. Inject the selected typed policy before StartRequested for All, Explicit and Ordered paths. Resolve public constructor semantics deliberately, avoiding an optional initialization path that silently continues to fabricate hard provenance.
3. Keep current synthetic reuse distinct from real execution and stored historical records. Cover cancellation and stopped NotRun, mechanism mapping and unchanged actual process observations; no OS enforcement claim from unit inputs.

## OKF self-review

1. Trace resource backend selection and reporting concepts against actual core ResourceControl fields and current output/session contracts.
2. Add the issue design with its exact heading and source hash; preserve historical evidence revisions and distinguish current policy from stored history.
3. Validate16-page metadata, source-footnote pairs, links, reachability, hashes and161 actual/index/display design entries after final updates.

## Task 1: Carry selected resource policy into run output

Read the spec, /private/tmp/issue487-preflight-notes.md and issue487 in /private/tmp/hoimin-bug-issues.json. Worktree /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-487; branch fix/issue-487-resource-report-policy; base4adf809. Own relevant Rust/backend/core/shell/report tests and current README/development docs. Controller owns docs/superpowers and docs/knowledge. Do not delegate, push or create PRs. Send concrete evidence and smallest sound alternative before a material design expansion.

- [x] Capture public JSON and JSONL RED on actual best-effort fresh execution and timeout+killed natural-incomplete session followed by resume, DB outside project. Preserve saved result provenance without SQL edits.
- [x] Inspect selected backend variants/mode, ProcessHandler access, RunState constructors/selection variants, RunStarted::minimal and all synthetic output call sites. Define one typed description with stable nonempty mechanism derived from actual backend, not approval flags or a diagnostic sentence.
- [x] Supply the description to core before StartRequested and use it for run header and reused/stopped/cancelled synthetic output. Make missing-backend constructor semantics explicit; no fabricated hard default on any production path. Avoid a serialization-only correction or needless new asynchronous phase. Do not change enforcement or #488 scope.
- [x] Preserve real process resource_mode and synthetic termination/output null, zero elapsed, statuses/counts/IDs and session history. Verify the current resumed policy is used without pretending reused work executed again.
- [x] Add meaningful core controls for both modes, all selection paths and cancelled/not-started results. Public fresh/resume JSON+JSONL tests must observe baseline, actual timeout, reused killed and mechanism. Hard-native capability is separate from supplied-policy unit evidence; no captured skip counted as native proof.
- [x] Assess whether Lean adds independent transition evidence. If used, derive policy/output expectations and compare actual Rust transitions; state supplied-backend premise and no OS proof. Guard Lean serial30seconds/2048MiB/250ms, -j1 and -DElab.async=false. Exact generator/CI inventory changes require full tests/test_ci_workflow.py.
- [x] Run focused resource/report/resume/core tests, full cargo test --workspace --all-features, fmt and Clippy all targets/features. IDE MCP does not have hoimin open; use Cargo without inspecting unrelated projects.
- [x] Perform three implementation and three test self-reviews. Record RED/GREEN commands, exact totals, decisions and limitations in .superpowers/sdd/2026-09-11-issue-487-resource-report-policy/task-1-report.md. Commit only owned files and stop for review; no repeated green suites without new evidence or changes.

## Controller API decision

Require existing ResourceControl in all RunState constructors and RunStarted::minimal, using actual backend description from shell before StartRequested. This is a deliberate source-API migration (about62 repository callers, mostly tests, and external Rust callers must supply policy), not a new report schema or enforcement change. It avoids omitted-policy states and guessed defaults. Backend identifiers portable/linux_cgroup_v2/windows_job_object carry no aggregate-scope promise. Root approved after constructor/call-site trace; implementation remains owned by the one worker. No Lean model planned because actual Rust/core and public process observations provide direct correspondence.

## Controller implementation and PR preparation reviews

Implementation review1 traced actual backend→ProcessHandler→all selection constructors→core header/synthetic output. Required inputs eliminate omitted-policy defaults and the actual process observation path stays unchanged. Review2 inspected24 real Rust transition scenarios, including observed Hard process results with a supplied BestEffort policy to detect accidental overwriting. Review3 inspected public JSON/JSONL natural resume, requested nonvacuous stored killed-row/ID assertions, and distinguished legitimate timeout row replacement from reused-history preservation. No new production defect found.

Three PR preparation reviews:1 compare final scope with issue and independent #488, explicitly disclose constructor source-API migration and unchanged schema;2 separate actual best-effort macOS observations, supplied-policy core tests and native-hard limits, and retain intermediate test-harness corrections;3 validate OKF source hashes/reachability and161 actual/index/display designs, exclude .venv/ignored scratch, and preserve full versus later focused validation provenance. Independent reviews follow the owned implementation and documentation commits.

The controller requested a2second mutant limit/6second finite sleeper in the new resume fixture after earlier hosted1second fixture-readiness failures in other PRs. This changes only the new test's load margin: the intended process still genuinely times out and is reexecuted, while healthy killed startup has more room. No production deadline or assertion was weakened.

## Persisted implementation and verification report

# Issue 487 task 1 implementation report

Implementation commit: `3024d51` (`fix: report selected resource backend throughout runs`). All authorized implementation work is complete and stopped for controller task review.

## Result and boundary

The actual selected `ResourceBackend` exposes `resource_control()`, using its `mode()` and stable variant identifiers `portable`, `linux_cgroup_v2`, and `windows_job_object`. `ProcessHandler` forwards this typed description. The shell supplies it to all three candidate-selection constructors before `StartRequested`. Core retains it for `RunStarted` and synthetic reused/cancelled/deadline NotRun output. Real process observations remain untouched.

`RunState::new`, `with_fingerprint`, `with_candidate_filter`, `with_ordered_candidate_filter`, and `RunStarted::minimal` require `ResourceControl` as their last argument. This deliberately changes the Rust source API (62 preexisting repository call sites migrated); external callers must supply a description. There is no optional initialization path, default Hard, or guessed policy from `allow_best_effort_memory`. Existing `ResourceControl` is reused, including singular `mechanism: String`; report schema shape/version and event phases are unchanged. The boundary assumes callers supply the selected backend truthfully; it is not an OS enforcement proof.

Enforcement, process cleanup, session persistence implementation, result eligibility, schemas, and Windows aggregate-limit scope (#488) were not changed. README and development docs explain current reporting and constructor migration.

## Initial evidence and regression checks

All Cargo commands used this exact environment, serially:

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target
```

The worktree .venv links to the controlled CPython 3.14.7 environment. Disk check before large builds showed 13 GiB available. No cargo-mutants, Lean process, IDE unrelated project, delegation, push, PR, or merge was used.

Initial valid RED command:

```sh
cargo test -p hoimin-cli --test run_e2e selected_resource_policy_survives_fresh_and_natural_resume_reports -- --nocapture
```

`/private/tmp/issue487-red.log`: 0 passed, 1 failed, 59 filtered; 4.61 seconds. The log contains actual public JSON and JSONL fresh/resumed reports (four executions) with best_effort baseline/actual mutant results, Hard/empty run headers, and Hard reused killed output. It checks a real killed result and actual timeout, naturally incomplete summary, unchanged candidate IDs, null synthetic termination/output, zero synthetic elapsed, and saved killed-row preservation. No SQL mutations are performed; the session DB is outside the project.

An earlier fixture check incorrectly asserted all saved rows were immutable. Actual natural resume reuses the same session run_id and updates the timeout row after real execution; only the reused killed row must remain unchanged. The test was corrected before the valid RED. JSONL uses `kind`, not `event`, as its discriminator. This was fixture debugging, not a product fix or waived contract.

The core matrix exercises 24 cases: 2 supplied modes × 3 selection constructors × executed/reused/cancelled/deadline. Restoring the old synthetic Hard expression made it RED (0 passed, 1 failed, 74 filtered) with Hard versus BestEffort on reuse; `/private/tmp/issue487-core-red.log`. The production fix was restored. The separate initial public RED covers header provenance.

## Verification

- `cargo test --workspace --all-features`: 1,610 passed, 0 failed, 13 ignored across 68 summaries; exit 0, `/private/tmp/issue487-workspace.log`. Its only new warning was a fixture helper unused with the contracts feature; this was corrected with the matching cfg and verified by final all-feature Clippy. Later edits were test assertions/refactoring, test-only timeout margin, and documentation; affected tests were rerun.
- `cargo test -p hoimin-core --test machine --test report_policy --test resume_policy --test lean_report_sequence_oracle`: 135 passed, 0 failed, 0 ignored (75 + 37 + 18 + 5), `/private/tmp/issue487-core-suites.log`. Default-feature oracle correspondence runs in this command; this change adds no Lean model/corpus.
- `cargo test -p hoimin-cli --test plan --test process_handler --test report_handler`: 93 passed, 0 failed, 1 ignored (45 + 27 + 21), `/private/tmp/issue487-cli-suites.log`. Existing actual explicit-selection JSONL and ordered/top JSON tests now assert selected policy matches baseline and mutant output.
- Final matrix: 1 passed, 0 failed, 74 filtered, 0.01s. Final real fresh/resume: 1 passed, 0 failed, 59 filtered, 8.62s. Both exit 0; see `/private/tmp/issue487-core-final.log` and `/private/tmp/issue487-green-final.log`. The real fixture uses a 2-second mutant timeout and bounded 6-second sleeper to leave startup margin, still requiring an actual Timeout.
- `cargo fmt --all -- --check`: passed, exit 0, `/private/tmp/issue487-fmt.log`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed, exit 0, `/private/tmp/issue487-clippy.log`. Initial Clippy found the core test at 102/100 lines; redundant intermediate collection was removed and CLI setup extracted. No lint suppression was added.
- `git diff --check`: passed.

## Implementation self-reviews (three passes)

1. Traced origin and ownership: actual variant and delegated mode are used, not the permission flag or human diagnostic. Mechanism names identify backends without new aggregate claims. Existing ResourceControl is sufficient; no serialization-only correction or new asynchronous phase.
2. Audited constructor and event coverage: all four constructors require the description, shell All/Explicit/Ordered receive it before StartRequested, header and all synthetic call sites use stored current policy, and no missing-policy state is constructible through those APIs. Public constructor migration is explicit in development docs.
3. Audited negative scope and historical semantics: actual result paths still use result.resource_mode/value.resource_mode; synthetic null/zero/status/candidate fields remain unchanged; no session persistence or enforcement code changed. Reused killed provenance remains saved; real timeout observations legitimately update. Existing schema goldens pass.

## Test self-reviews (three passes)

1. Checked reproduction against real behavior: both output formats execute baseline/killed/timeout and natural resume, using an outside-project DB and no SQL edits. Corrected the overbroad immutability assertion from observed persistence behavior before valid RED. The RED logs capture the actual reporting contradiction.
2. Removed vacuous assertions: the history query must yield exactly one killed row and match run_id/candidate_id; output must contain exactly two mutants including killed and timeout. Core matrix requires exactly one header, two mutants and exact synthetic cardinalities 0/1/2, then checks status/null/zero/current mode. It intentionally supplies Hard observed execution under BestEffort input to detect overwriting real observations.
3. Reviewed selection, timing, and platform claims: strengthened existing explicit/ordered public verify tests; portable backend/process forwarding is tested directly; native Linux/Windows handler tests assert their stable identifiers when their existing native paths execute. Local macOS only establishes portable execution, not native hard enforcement. Raised only the test timeout margin to 2s/6s; bounded sleeper and actual Timeout assertion remain. Final affected tests rerun after Clippy refactor.

## Lean decision and limitations

No new Lean model was added. This change transports an already-selected typed input without adding a transition phase or scheduling rule; direct Rust transition tests observe the actual output events. A duplicate toy transport model would not independently establish backend selection or OS enforcement. Existing Lean-backed tests remain and their constructor fixtures were migrated. No generator or CI inventory changed.

Native Linux cgroup/Windows Job Object enforcement is not claimed from this macOS run. Added cfg-native mapping assertions are executed only where those existing tests have native capability; a capability skip is not hard-enforcement evidence. Synthetic policy denotes the current run and does not claim reused mutants executed again.

The task report is ignored scratch per controller instruction. Controller owns committed design/plan and OKF updates. Implementation commit includes only Rust, tests, README and development docs.

Final OKF validation:16 pages,310 source-footnote pairs,613 local links, all reachable and hashes valid;161 actual/indexed/displayed designs. Design, plan, OKF, implementation, tests and PR preparation each received at least three recorded self-reviews. Independent task review and final whole-branch review are separate from those self-reviews.

Independent task reviewer review487_task approved4adf809..3024d51 with no actionable correctness/spec findings. Reviewed constructor migration, real history/output assertions and all24supplied-policy cases without rerunning green suites or modifying files. Final whole-branch review follows this documentation commit.
