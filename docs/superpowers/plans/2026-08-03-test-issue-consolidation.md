# Test-Issue Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the folded and independent test issues with the smallest non-duplicative acceptance coverage, one reviewed and merged pull request per issue that needs repository changes.

**Architecture:** Audit each issue against current production-boundary tests before adding coverage. Work sequentially where test modules overlap, use issue-specific worktrees based on the latest `origin/main`, and close evidence-only issues without empty pull requests. Keep platform-specific remainders explicit instead of weakening assertions or adding production-only test seams.

**Tech Stack:** Rust 1.88+, Tokio, proptest, rusqlite, GitHub Actions, GitHub CLI, Python test fixtures.

## Global Constraints

- Use one worktree, branch, design/plan note, commit series, and pull request per issue that changes repository files.
- Base every worktree on the latest `origin/main`; merge overlapping test-module pull requests sequentially.
- Preserve the root checkout's untracked `.idea/`, `.serena/`, and `tests/fixtures/projects/basic/uv.lock`.
- Add `.venv -> ../../.venv` only as an untracked worktree setup link and unlink it before worktree removal.
- Run a focused RED test before implementation and focused GREEN tests after implementation.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, and `git diff --check` before merging Rust changes.
- Do not claim Windows, Job Object, cgroup, OOM, or process-limit coverage on a runner that cannot enforce the relevant behavior deterministically.
- Do not add a shared test-fixtures crate or mechanically annotate every exact assertion.
- For Tasks 9, 10, 11, 14, and 15, configure every proptest block with exactly 128 cases via `ProptestConfig::with_cases(128)`. Keep proptest's default source-parallel failure persistence enabled, commit every generated regression seed file before merge, and compute expected results with a test-only model or encoder that does not call or reproduce the production algorithm under test.

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    // The task-specific properties follow; do not override failure_persistence.
}
```

- Treat the acceptance-coverage audit as the closure ledger. A scoped issue may close only when every one of its audit rows is `covered`; a row that remains `missing` or `platform remainder` must name and link a still-open narrowed follow-up before the parent issue closes. One follow-up may cover multiple rows only when its body enumerates and links each row separately.
- A backend-dependent row may become a remainder only after the issue PR records the runner capability probe and why deterministic enforcement is unavailable. When the required delegated cgroup-v2 job can enforce the behavior, implement and run the acceptance test there; do not relabel the row as a blanket platform remainder. Keep the original issue open, or split and link the row to a narrowed open backend follow-up, until that evidence exists.

---

### Task 1: Build and publish the acceptance-coverage audit

**Files:**
- Create: `docs/superpowers/reports/2026-08-03-test-issue-consolidation-audit.md`

**Interfaces:**
- Consumes: issue bodies #153–#168 and tests on the latest `main`.
- Produces: one row per acceptance criterion with `covered`, `missing`, or `platform remainder`, plus exact test and PR links.

- [ ] **Step 1: Query the issue and related-PR state**

Run:

```bash
gh issue view 153 --json number,state,title,body,url
```

Repeat for #154, #155, #156, #157, #158, #159, #160, #161, #162, #163, #164, #165, #166, #167, and #168.

- [ ] **Step 2: Map current regression tests to criteria**

Run:

```bash
rg -n "second_sigint|same_termination|schema_v2|abnormal_exit|root_generation|stable.*identity|changed_lines|documentation_contract" crates tests
```

- [ ] **Step 3: Write the audit table**

Use rows with this exact shape:

```markdown
| Issue | Criterion | Status | Evidence or missing test |
| ---: | --- | --- | --- |
| #155 | Session/sessionless termination parity | covered | `run_e2e::fresh_session_and_sessionless_results_preserve_the_same_termination` |
```

- [ ] **Step 4: Self-check the audit against every issue-body bullet**

Run:

```bash
rg -n "#(153|154|155|156|157|158|159|160|161|162|163|164|165|166|167|168)" docs/superpowers/reports/2026-08-03-test-issue-consolidation-audit.md
git diff --check
```

- [ ] **Step 5: Commit the audit**

```bash
git add docs/superpowers/reports/2026-08-03-test-issue-consolidation-audit.md
git commit -m "docs: audit consolidated test issues"
```

### Task 2: Resolve #155 session-report parity coverage

**Files:**
- Modify if missing: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-155-session-parity.md`

**Interfaces:**
- Consumes: `env!("CARGO_BIN_EXE_hoimin")`, the real-run readiness-marker fixture, `run_fixture_options`, and `run_fixture_with_session`.
- Produces: field-for-field fresh-session/sessionless parity and cross-process live-run refusal evidence.

- [ ] **Step 1: Retain the already-covered parity criterion**

Link `run_e2e::fresh_session_and_sessionless_results_preserve_the_same_termination` and retain its exact `termination == {"Exit":1}` assertion. Do not add a duplicate parity test.

- [ ] **Step 2: Write the missing real-process ownership test**

Bind `let session = coordinator.path().join("session.sqlite3")`. Start the first `CARGO_BIN_EXE_hoimin run --format json --session` process with `&session` and a mutation command that atomically renames a temporary readiness file after the session row exists, then blocks. After observing the marker and exactly one incomplete `runs` row, start a second `CARGO_BIN_EXE_hoimin run --format json --session` process with `&session`, `--resume`, and the same fingerprint. Both invocations must overlap in wall-clock time; calling `SessionHandler` directly does not satisfy this row.

Assert the second process and the eventual first-process cancellation exactly:

```rust
assert_eq!(second.exit_code, 2);
let second_document: serde_json::Value = serde_json::from_str(&second.stdout)?;
assert_eq!(second_document["summary"]["complete"], false);
assert!(second.stderr.contains("session.resume.active"), "{}", second.stderr);
assert!(second.stderr.contains("active in another process"), "{}", second.stderr);
send_sigint(first.id());
assert_eq!(first.wait().await?.code(), Some(130));
```

- [ ] **Step 3: Prove database integrity and absence of duplicates**

After both children exit, open the database and require all of the following: `PRAGMA integrity_check` returns exactly `ok`; `PRAGMA foreign_key_check` returns zero rows; `SELECT count(*) FROM runs` is exactly `1`; and the result-row count equals `count(DISTINCT mutant_id)` for that sole run. Also assert every result joins to exactly one candidate, so the refused process neither creates a second run nor corrupts or duplicates the first process's records.

- [ ] **Step 4: Run focused verification**

```bash
cargo test -p hoimin-cli --test run_e2e fresh_session_and_sessionless_results_preserve_the_same_termination
cargo test -p hoimin-cli --test run_e2e concurrent_real_cli_runs_refuse_live_session_ownership
```

- [ ] **Step 5: Commit, PR, review, merge, and close #155**

Commit with `test: cover real CLI session ownership contention`, create a PR with both audit-row links and `Closes #155`, wait for CI, review, and merge.

### Task 3: Resolve #153 real-signal cancellation coverage

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-153-real-signals.md`

**Interfaces:**
- Consumes: `spawn_second_sigint_fixture`, `wait_for_jsonl_kind`, and Unix `libc::kill` support.
- Produces: a real first-SIGINT report test and retained second-SIGINT escalation test.

- [ ] **Step 1: Write the Unix first-signal test**

Add a `#[cfg(unix)]` test that spawns the real binary in JSON mode, waits for
the existing readiness files, sends SIGINT, and whose terminal assertions are:

```rust
assert_eq!(status.code(), Some(130));
let report: serde_json::Value = serde_json::from_str(&stdout)?;
assert_eq!(report["summary"]["complete"], false);
assert_eq!(session_complete(&session), 0);
assert!(descendant.wait_until_stops(Duration::from_secs(5)).await);
```

- [ ] **Step 2: Run it before implementation support changes**

```bash
cargo test -p hoimin-cli --test run_e2e first_sigint_finishes_a_parseable_incomplete_session -- --exact
```

Expected: FAIL if the existing fixture cannot produce final JSON after the first signal.

- [ ] **Step 3: Reuse the existing process fixture without adding a production test API**

Factor only test helpers needed to prepare the same project and send one or
two signals. The first-signal fixture uses readiness files rather than JSONL
stdout so its final stdout remains one parseable JSON report. Keep
`second_sigint_forces_130_while_session_finish_is_blocked` intact.

- [ ] **Step 4: Verify both real-signal paths**

```bash
cargo test -p hoimin-cli --test run_e2e first_sigint_finishes_a_parseable_incomplete_session
cargo test -p hoimin-cli --test run_e2e second_sigint_forces_130_while_session_finish_is_blocked
```

- [ ] **Step 5: Document Windows as covered or retained**

Do not emulate `GenerateConsoleCtrlEvent`. If a stable Windows console-group test is unavailable, create or retain a narrowed open Windows follow-up whose body quotes and links the audit row for both first and second console-control interrupts. Link that follow-up in #153 before closing #153; never mark the Windows row covered by the Unix tests.

- [ ] **Step 6: Commit, PR, review, and merge**

Commit with `test: exercise cancellation through real signals`; the PR closes only the Unix acceptance portion and must state the Windows disposition. Apply the global closure invariant before closing #153.

### Task 4: Resolve #158 real-binary stream routing

**Files:**
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-158-real-binary-streams.md`

**Interfaces:**
- Consumes: `env!("CARGO_BIN_EXE_hoimin")` and the basic fixture.
- Produces: real-process stdout, stderr, and exit-code assertions.

- [ ] **Step 1: Add real help/version tests**

```rust
for argument in ["--help", "--version"] {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .arg(argument)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
```

- [ ] **Step 2: Add a real JSON run routing assertion**

Run the invalid-syntax fixture through `CARGO_BIN_EXE_hoimin run --format json`. Assert exit 4; stdout contains exactly one newline-terminated JSON document whose summary has `complete:false`; stderr contains a JSON diagnostic with `code == "analyzer.invalid_syntax"`; the stdout document does not contain that diagnostic text; and stderr does not contain a report object with `"summary"`.

- [ ] **Step 3: Run focused tests**

```bash
cargo test -p hoimin-cli --test cli_config real_binary
cargo test -p hoimin-cli --test run_e2e real_binary_json_report
```

- [ ] **Step 4: Commit, PR, review, merge, and close #158**

Use commit `test: verify real binary stream routing`.

### Task 5: Resolve #154 real run-to-progress workflow

**Files:**
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-154-real-progress-workflow.md`

**Interfaces:**
- Consumes: the checked `schema-v2-original.json` fixture and real `hoimin run` reports.
- Produces: an exact computed regression from two real reports while retaining the oldest-supported-report evidence.

- [ ] **Step 1: Generate a known killed-to-survived pair with the compiled binary**

Use the same one-candidate source tree for both `CARGO_BIN_EXE_hoimin run --format json --operators binary_add_sub --max-mutants 1` invocations. The before command must kill the candidate and exit 0; the after command must let the same stable candidate ID survive and exit 1. Write each stdout document to a temporary file, assert each contains exactly one mutant, assert the IDs are equal, and assert statuses `killed` then `survived` before invoking progress.

- [ ] **Step 2: Invoke real progress**

```rust
let output = Command::new(env!("CARGO_BIN_EXE_hoimin"))
    .args(["progress", "--format", "json"])
    .arg(&before)
    .arg(&after)
    .output()
    .unwrap();
assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
let progress: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
assert_eq!(progress["latest"]["state"], "regressing");
assert_eq!(progress["comparisons"][0]["common"], 1);
assert_eq!(progress["comparisons"][0]["added"], 0);
assert_eq!(progress["comparisons"][0]["removed"], 0);
assert_eq!(progress["comparisons"][0]["improvements"], 0);
assert_eq!(progress["comparisons"][0]["regressions"], 1);
assert_eq!(progress["comparisons"][0]["previous_score"], 1.0);
assert_eq!(progress["comparisons"][0]["current_score"], 0.0);
assert_eq!(progress["comparisons"][0]["score_delta"], -1.0);
```

- [ ] **Step 3: Retain the already-covered oldest-schema criterion**

Link `progress::input_accepts_the_oldest_schema_v2_normalized_config` and `report_handler::original_schema_v2_report_fixture_matches_the_published_schema`. Do not add a weaker mixed-input parsing assertion.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #154**

Run `cargo test -p hoimin-cli --test progress real_run` and commit `test: exercise real run reports through progress`.

### Task 6: Resolve #156 changed-selection CLI wiring

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-156-changed-cli.md`

**Interfaces:**
- Consumes: temporary Git repository helpers and JSON run/plan output.
- Produces: full CLI candidate intersection for `--changed` and `--diff-base`.

- [ ] **Step 1: Create a two-function committed fixture and edit one function**

Commit `src/calc.py` with `changed` and `untouched`, each returning `a + b`, record the base revision, then edit only line 2 to `    return a + b  # changed`. Invoke both commands with `--operators binary_add_sub`; the independent expected set is exactly one tuple:

```rust
let expected = BTreeSet::from([(
    "src/calc.py", 2_u64, 13_u64, "binary_add_sub", "+", "-", "changed",
)]);
```

The tuple fields are path, one-based line, zero-based column, operator, original, replacement, and symbol. This pins the edited line and does not derive the oracle from either command's output.

- [ ] **Step 2: Add `run --changed` acceptance**

```rust
assert_eq!(mutants.len(), 1);
assert_eq!(candidate_tuples(mutants), expected);
```

- [ ] **Step 3: Add `plan --changed --diff-base` acceptance**

Bind `let base_revision = git_rev_parse(&project, "HEAD")` before editing the file. Invoke `plan --changed --diff-base` with `&base_revision` and `--operators binary_add_sub`, assert `manifest.candidates.len() == 1`, and assert `candidate_tuples(&manifest.candidates) == expected`. Also compare the run and plan stable candidate-ID sets for exact equality. Empty output, universal predicates over possibly empty output, symbol-only checks, and assertions derived from either actual output are not accepted.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #156**

Run the two exact integration tests and commit `test: exercise changed selection through the CLI`.

### Task 7: Resolve #161 session contention matrix

**Files:**
- Modify: `crates/hoimin-cli/tests/session_handler.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-161-session-contention.md`

**Interfaces:**
- Consumes: `SessionHandler`, a second rusqlite connection, and explicit transaction barriers.
- Produces: the complete two-lock-mode by five-operation matrix with coordinated, bounded, typed outcomes.

- [ ] **Step 1: Define the exact 2×5 matrix**

```rust
enum ContendedOperation { Begin, Persist, Lookup, Finish, Load }
enum HeldLock { BeginImmediate, ReadTransaction }

const CELLS: [(HeldLock, ContendedOperation); 10] = [
    (HeldLock::BeginImmediate, ContendedOperation::Begin),
    (HeldLock::BeginImmediate, ContendedOperation::Persist),
    (HeldLock::BeginImmediate, ContendedOperation::Lookup),
    (HeldLock::BeginImmediate, ContendedOperation::Finish),
    (HeldLock::BeginImmediate, ContendedOperation::Load),
    (HeldLock::ReadTransaction, ContendedOperation::Begin),
    (HeldLock::ReadTransaction, ContendedOperation::Persist),
    (HeldLock::ReadTransaction, ContendedOperation::Lookup),
    (HeldLock::ReadTransaction, ContendedOperation::Finish),
    (HeldLock::ReadTransaction, ContendedOperation::Load),
];
```

- [ ] **Step 2: Coordinate every cell without sleeps**

For each cell, seed a valid independent run and result as required by that operation. Use barriers/channels for `blocker transaction established`, `operation dispatched`, and `blocker released`; do not use sleep as readiness. The blocker executes either `BEGIN IMMEDIATE` or `BEGIN; SELECT ...` to retain a read snapshot, the operation starts while that transaction is held, and the test releases the blocker no later than 1 second after dispatch, below the configured 5-second SQLite busy timeout.

- [ ] **Step 3: Assert an exact typed success for all ten valid requests**

Each cell must complete within 6 seconds of dispatch and return its operation's exact success variant: `SessionStarted` with the requested run ID, `ResultPersisted` with the requested mutant ID, `StoredResultLoaded` with the seeded result, `SessionFinished` with the requested completeness, or `SessionLoaded` with the seeded resume run. For `BEGIN IMMEDIATE`, additionally assert `begin`, `persist`, and `finish` are still pending at the pre-release barrier. Any `EffectFailed`, raw `DatabaseBusy`/`DatabaseLocked`, panic, or timeout fails the cell; the test must print `(lock, operation)` on failure.

- [ ] **Step 4: Prove all cells ran, then commit, PR, review, merge, and close #161**

Maintain a visited set and assert it equals `CELLS`, then run `cargo test -p hoimin-cli --test session_handler contention_matrix`. Commit `test: cover session operations under contention`; close #161 only after all seven audit rows link to this matrix test.

### Task 8: Resolve #162 platform process-backend fault coverage

**Files:**
- Modify if missing: `crates/hoimin-cli/src/resource/windows.rs`
- Modify if missing: `crates/hoimin-cli/src/resource/linux.rs`
- Modify if missing: `crates/hoimin-cli/tests/process_handler.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-162-process-backend-faults.md`

**Interfaces:**
- Consumes: existing abnormal-exit, root-generation, and cgroup notification tests.
- Produces: separate evidence or linked open follow-ups for all three Linux rows and all three Windows remainders.

- [ ] **Step 1: Preserve all three Windows remainders explicitly**

The current direct-state tests do not cover any of these acceptance boundaries: (1) a real abnormal root exit crossing Job Object completion-port notification and bounded classification, (2) a real root exiting while descendants remain assigned and bounded cleanup, and (3) a real exited/recycled PID whose old generation is cleaned without signaling the reused numeric PID. Implement each only with a deterministic Windows process fixture. Otherwise create or retain narrowed open Windows follow-ups whose bodies quote and link their individual audit rows; never mark them covered by `record_notification` or synthetic-PID tests.

- [ ] **Step 2: Add the delegated-cgroup abnormal-root classification scenario**

On the required delegated cgroup-v2 job, spawn and attach a runtime root that raises `SIGABRT` after an atomic ready marker. Assert the observed termination is exactly `ProcessTermination::Exit(128 + libc::SIGABRT)`, never `Timeout`, and `handler.handle(...)` plus `handler.close()` complete within 6 seconds. A launcher that fails before target execution does not satisfy this row.

- [ ] **Step 3: Add the delegated-cgroup root-before-descendant scenario**

Spawn an attached root that creates a long-lived descendant, atomically publishes the descendant PID, and exits 0 before the descendant. Assert the root result is `ProcessTermination::Exit(0)`, the descendant was alive after the root exit was observed, `handler.close()` completes within 6 seconds, the descendant is no longer alive, and the run cgroup directory is removed. Timeout-driven cleanup while the root is still supervised does not satisfy this row.

- [ ] **Step 4: Instrument the stale-numeric-PID cleanup boundary**

Use a test-only cleanup-call recorder inside the private Linux backend, not a production fault-injection API. Exercise the successful `cgroup.kill` path after the root has exited and assert exactly:

```rust
assert_eq!(signals.numeric_pid_calls(), 0);
assert_eq!(signals.process_group_calls(), 0);
assert_eq!(kernel_group.kill_calls(), 1);
```

- [ ] **Step 5: Record backend evidence or link a remainder per Linux row**

Run the three Linux tests in the delegated cgroup-v2 CI job and attach its URL to each audit row. If any scenario cannot be enforced deterministically despite that job, record the capability probe and create a narrowed open backend follow-up quoting that row; do not close #162 while a row lacks either passing evidence or its own linked open follow-up. Do not add timing-based PID-reuse loops.

- [ ] **Step 6: Verify, merge, and apply the closure invariant**

Run `cargo test -p hoimin-cli --test process_handler cgroup_v2` on the delegated runner and the targeted `resource::linux` tests locally. Use commit `test: cover process backend fault boundaries`. Close #162 only if all six audit rows are covered, or after every remaining Windows/backend row is linked to a specifically narrowed open follow-up.

### Task 9: Resolve #165 hostile Git diff properties

**Files:**
- Modify: `crates/hoimin-cli/src/target/git.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-165-git-diff-properties.md`

**Interfaces:**
- Consumes: private `parse_diff`, `decode_git_quoted`, and proptest.
- Produces: hostile zero-context diff parsing and the missing representable UTF-8 Git C-quote round trip.

- [ ] **Step 1: Add a generated diff-section strategy**

Bias generated content toward `++ b/evil.py`, `-- a/evil.py`, `@@ -1 +1 @@`, and `Binary files a/x.py and b/y.py differ`.

- [ ] **Step 2: Assert parser results against generated edit ranges**

```rust
let expected_changes = generated_hunks
    .iter()
    .map(|hunk| (hunk.destination.clone(), hunk.new_start..hunk.new_start + hunk.new_len))
    .collect::<BTreeMap<_, _>>();
prop_assert_eq!(parse_diff(rendered.as_bytes())?, expected_changes);
```

`render_unified0_diff` and the range oracle are test-only data-model renderers: they may serialize generated hunk fields, but they must not call `parse_diff`, production header classifiers, or production path decoding while computing `expected_changes`.

- [ ] **Step 3: Retain totality and add only the missing quoted-path property**

```rust
proptest!(|(path in arbitrary_representable_utf8_git_path())| {
    prop_assert_eq!(decode_git_quoted(&git_c_quote(&path))?, path);
});
```

Link and retain the already-covered `target::git::quoted_path_decoding_is_total`; do not duplicate its arbitrary-input no-panic property. Implement `git_c_quote` as an independent test-only encoder over representable UTF-8 path bytes, including spaces, tabs, quotes, backslashes, and non-ASCII characters; it must not call the production decoder or share its escape parser.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #165**

Run `cargo test -p hoimin-cli target::git` and commit `test: property test hostile Git diff parsing`.

### Task 10: Resolve #168 progress-comparison properties

**Files:**
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-168-progress-properties.md`

**Interfaces:**
- Consumes: `compare_reports`, arbitrary `MutantFinished` vectors, and all mutation statuses.
- Produces: a seven-status stable-ID model for self-comparison, reversal antisymmetry, order invariance, and duplicate-content regressions.

- [ ] **Step 1: Generate reports and an independent stable-ID oracle**

Generate zero or more unique stable candidate IDs, force at least one conclusive candidate in every non-empty report, and draw remaining statuses from all seven `MutationStatus` variants: `Killed`, `Survived`, `Timeout`, `OutOfMemory`, `ProcessLimit`, `Error`, and `NotRun`. Deliberately allow different IDs to share identical path/original/replacement/operator/symbol content. Build the expected comparison by joining a test-only `BTreeMap<candidate_id, status>` and counting transitions; the oracle must not call `compare_reports`, `candidate_set_eligibility`, or production score/state helpers.

- [ ] **Step 2: Assert self-comparison**

```rust
prop_assert_eq!(comparison.added, 0);
prop_assert_eq!(comparison.removed, 0);
prop_assert_eq!(comparison.improvements, 0);
prop_assert_eq!(comparison.regressions, 0);
prop_assert_eq!(comparison.score_delta, if report.is_empty() { None } else { Some(0.0) });
prop_assert_eq!(
    comparison.state,
    if report.is_empty() { ProgressState::Indeterminate } else { ProgressState::Stalled },
);
```

- [ ] **Step 3: Assert reversal antisymmetry and order invariance**

Swap before/after and assert `forward.improvements == reverse.regressions`, `forward.regressions == reverse.improvements`, `forward.added == reverse.removed`, `forward.removed == reverse.added`, and `forward.score_delta == reverse.score_delta.map(|delta| -delta)`. Apply independent generated permutations to both mutant vectors and assert the complete `Comparison` is unchanged.

- [ ] **Step 4: Pin killed-to-survived by stable ID across duplicate content**

Generate a before/after pair with identical unique ID sets, choose one ID as `Killed` before and `Survived` after, give at least one other ID the exact same content key, and assign every other candidate any of all seven statuses. Assert the independent ID-join oracle and production comparison agree, the chosen ID contributes exactly one regression, and the duplicate content key contributes neither ambiguity nor a lost match.

- [ ] **Step 5: Run, commit, PR, review, merge, and close #168**

Run `cargo test -p hoimin-cli --test progress compare_property` and commit `test: add progress comparison properties`.

### Task 11: Resolve #164 state-machine schedule properties

**Files:**
- Modify: `crates/hoimin-core/tests/machine.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-164-machine-schedules.md`

**Interfaces:**
- Consumes: pure `transition`, pending effect IDs, and `ReportSequence::observe`.
- Produces: bounded adversarial schedules with one terminal output and consistent candidate accounting.

- [ ] **Step 1: Define a schedule action generator**

```rust
enum ScheduleAction { Complete(usize), Cancel, Deadline, Fail(usize) }
```

- [ ] **Step 2: Build a harness that always selects an existing pending effect**

Map generated indexes modulo the current pending-effect count; stop after a fixed step cap and drain deterministically. Inject cancellation, deadline, and effect failures as independent generated actions. Include explicit generated regression cases for ordered cancellation (#111), an empty ordered spool (#112), and persistence interruption (#113), and commit any source-parallel regression seeds proptest writes.

- [ ] **Step 3: Assert machine invariants**

Assert the final machine state is `Finished`; exactly one `RunFinished` is emitted; no effect ID is reused; every scheduled candidate ID has exactly one `MutantStarted` and one `MutantFinished`; and the summary's seven status counts equal a test-only ledger accumulated solely from generated completion classifications. Feed the complete emitted `OutputEvent` stream, in order, to a fresh `ReportSequence::observe` and require every call to succeed. The ledger and sequence validator are the independent oracles; do not implement a second copy of `transition` in the test.

- [ ] **Step 4: Run bounded properties and the full core suite**

```bash
cargo test -p hoimin-core --test machine adversarial_schedule
cargo test -p hoimin-core
```

- [ ] **Step 5: Commit, PR, review, merge, and close #164**

Use commit `test: explore adversarial machine schedules`.

### Task 12: Resolve #157 terminal-status end-to-end coverage

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-157-terminal-statuses.md`

**Interfaces:**
- Consumes: mutation-conditional Python commands and final JSON reports.
- Produces: full-stack timeout, OOM, process-limit, and survivor exit-policy proof for all four #157 audit rows.

- [ ] **Step 1: Add the timeout mutant fixture**

```rust
assert_eq!(run.exit_code, 4);
assert_eq!(run.statuses, ["timeout"]);
assert_eq!(run.document["summary"]["counts"]["timeout"], 1);
assert_eq!(run.document["summary"]["complete"], false);
```

- [ ] **Step 2: Add the missing survivor exit assertion**

Add `assert_eq!(run.exit_code, 1);` to `ty_reports_a_surviving_nullable_contract_mutant` while retaining its exact `statuses == ["survived"]` assertion.

- [ ] **Step 3: Audit enforceable OOM/process-limit backends**

On the required delegated cgroup-v2 CI job, run real `CARGO_BIN_EXE_hoimin run --format json` fixtures that deterministically exceed the configured memory limit and process limit. Require exactly one affected mutant in each report and assert:

```rust
assert_eq!(oom.exit_code, 4);
assert_eq!(oom.statuses, ["out_of_memory"]);
assert_eq!(oom.document["mutants"][0]["termination"], "OutOfMemory");
assert_eq!(oom.document["summary"]["counts"]["out_of_memory"], 1);
assert_eq!(oom.document["summary"]["complete"], false);

assert_eq!(process_limit.exit_code, 4);
assert_eq!(process_limit.statuses, ["process_limit"]);
assert_eq!(process_limit.document["mutants"][0]["termination"], "ProcessLimit");
assert_eq!(process_limit.document["summary"]["counts"]["process_limit"], 1);
assert_eq!(process_limit.document["summary"]["complete"], false);
```

The existing backend classification test plus the pure report-policy test do not satisfy these full-stack rows. If a fixture still cannot be deterministic on the delegated job, record the capability probe and link each unmet row to a narrowed open backend follow-up; do not relabel it as a blanket platform remainder or close #157 without those links.

- [ ] **Step 4: Run, commit, PR, review, and merge**

Run the timeout and survivor tests on every supported runner and the OOM/process-limit tests on the delegated cgroup-v2 job. Commit `test: cover terminal mutant statuses end to end`; close #157 only after all four audit rows have passing links or each remaining backend row has a specifically linked narrowed open follow-up.

### Task 13: Resolve #160 serialized golden corpus

**Files:**
- Move: `crates/hoimin-cli/tests/fixtures/reports/schema-v2-original.json` → `crates/hoimin-cli/tests/golden/reports/schema-v2-original.json`
- Create: `crates/hoimin-cli/tests/golden/reports/schema-v2-current.json`
- Create: `crates/hoimin-cli/tests/golden/events/schema-v2-original.jsonl`
- Create: `crates/hoimin-cli/tests/golden/events/schema-v2-current.jsonl`
- Create: `crates/hoimin-cli/tests/golden/sessions/schema-v1.sqlite3`
- Create: `crates/hoimin-cli/tests/golden/sessions/schema-v2.sqlite3`
- Create: `crates/hoimin-cli/tests/golden/sessions/schema-v3.sqlite3`
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Modify: `crates/hoimin-cli/tests/session_handler.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-160-golden-artifacts.md`

**Interfaces:**
- Consumes: supported report schema v2 and database migrations 1→2→3.
- Produces: checked original/current JSON and JSONL plus SQLite v1/v2/v3 artifacts in which every era-representable optional field is populated and semantically validated.

- [ ] **Step 1: Define and assert the all-optionals contract**

Add consumer-local `all_optional_report_events(era)` and `all_optional_session_rows(schema_version)` constructors. In both report eras, populate and assert `run.normalized_config`, nested `selection.diff_base`, nested `output.metrics`, nested `session`, `candidate.symbol`, `mutant.termination`, `mutant.output`, and `summary.counts.score`. In the current era also populate and assert `run.verification_selection` and `summary.verification_selection`; if those additive fields are absent from the published original shape, list them explicitly as not original-era-representable. For SQLite v1 and v2, populate and assert `candidates.symbol` plus `results.output_token`, `output_retained`, and `output_observed`; for v3 populate those fields plus `termination_kind='exit'` and a non-null `termination_exit_code`. Fields absent from an older schema are documented as not era-representable, not silently treated as populated.

- [ ] **Step 2: Check in both all-optionals JSON report eras**

Move the existing published fixture to the golden path, update `original_schema_v2_report()` to that path, populate its original-era optionals, and add the current schema-v2 shape at `schema-v2-current.json`. Generate both from the explicit all-optionals constructors. Normalize run IDs, output tokens, elapsed milliseconds, and version strings to fixed values. Parse each with `read_report`, assert it is `InputReport::Usable`, and assert every era-representable JSON pointer listed in Step 1 is present and non-null. Do not retain a duplicate at the old fixture path.

- [ ] **Step 3: Check in both all-optionals JSONL event-stream eras**

Write `schema-v2-original.jsonl` and `schema-v2-current.jsonl` as complete typed sequences from `RunStarted` through `RunFinished`. Parse every line as `OutputEvent`, pass the full sequence through `ReportSequence::observe`, and assert every era-representable run/config/verification, candidate symbol, termination/output, score, and summary verification optional from Step 1 is non-null and equals its fixed expected value.

- [ ] **Step 4: Check in all-optionals SQLite v1, v2, and v3**

Populate every database with one fingerprint, incomplete run, candidate with non-null symbol, result with a non-null output triple, and diagnostic. Populate v3 termination kind/code as specified in Step 1. Before migration, assert `PRAGMA user_version` is exactly 1, 2, or 3 and query the era's expected rows directly. Then open each through `SessionHandler`, assert migration to version 3, load/lookup the typed result, and assert the symbol, output triple, diagnostic, and every era-representable termination value survived unchanged.

- [ ] **Step 5: Add deterministic semantic regeneration checks**

Regenerate current JSON and JSONL in memory, normalize only run ID, output token, elapsed time, and version strings, parse to typed values, and compare them to the checked current artifacts. Regenerate a current SQLite database, compare `sqlite_master` table/index SQL, `PRAGMA user_version`, and ordered logical row values to `schema-v3.sqlite3`; never compare JSON text order, timestamps, SQLite bytes, page counters, or auto-generated row IDs.

- [ ] **Step 6: Run, commit, PR, review, merge, and close #160**

Run progress, report-handler, and session-handler integration suites; commit `test: check in serialized compatibility artifacts`.

### Task 14: Resolve #166 candidate-spool properties

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/store.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-166-candidate-spool-properties.md`

**Interfaces:**
- Consumes: `CandidateStore::{push, finish}` and replay offsets.
- Produces: push/replay round trips and exact record-size boundary coverage.

- [ ] **Step 1: Generate valid ordered candidates with Unicode-heavy fields**

- [ ] **Step 2: Assert complete and resumed replay**

```rust
prop_assert_eq!(replay_all(&spool)?, candidates);
prop_assert_eq!(CandidateStore::replay_one(&spool, terminal_offset)?, None);
for (index, offset) in observed_offsets.into_iter().enumerate() {
    prop_assert_eq!(replay_from(&spool, offset)?, candidates[index..]);
}
```

Record each `next_offset` only from successful replay calls, while the expected suffix comes from the original generated vector and index, not by replaying from zero again. This independently proves every intermediate offset is a valid resume point and complete replay ends in `Ok(None)`.

- [ ] **Step 3: Pin serialized boundaries**

Construct records whose serialized JSON payload lengths are exactly `MAX_SPOOL_RECORD_BYTES - 2`, `MAX_SPOOL_RECORD_BYTES - 1`, and `MAX_SPOOL_RECORD_BYTES`. Assert the first two pushes succeed and produce total newline-terminated record lengths of `MAX_SPOOL_RECORD_BYTES - 1` and `MAX_SPOOL_RECORD_BYTES`; assert the exact-limit payload returns `StoreError::RecordTooLarge { limit: MAX_SPOOL_RECORD_BYTES }` and writes no record. Derive padding from `serde_json::to_vec` length in a bounded test helper, then independently assert the final exact lengths before calling `push`.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #166**

Use `cargo test -p hoimin-cli analyzer::store` and commit `test: property test candidate spool replay`.

### Task 15: Resolve #167 fingerprint properties

**Files:**
- Modify: `crates/hoimin-core/tests/resume_policy.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-167-fingerprint-properties.md`

**Interfaces:**
- Consumes: `compatibility_fingerprint` and arbitrary normalized configurations.
- Produces: permutation/duplicate invariance and canonical-field sensitivity.

- [ ] **Step 1: Generate arbitrary valid fingerprint inputs**

Include native Unix and Windows argument byte flavors, paths, hashes, limits, operators, sources, targets, and fingerprint inputs.

Build a test-only canonical model using explicit tagged length-prefixed fields and `BTreeSet` normalization for the four deduplicated collections. It must not call `compatibility_fingerprint` or any production canonicalization helper; use it to establish whether a generated permutation/duplicate is semantically identical and whether a selected edit changes canonical bytes before comparing production fingerprints.

- [ ] **Step 2: Assert normalization invariance**

```rust
prop_assert_eq!(fingerprint(&value), fingerprint(&permuted_and_duplicated));
```

- [ ] **Step 3: Assert one-field edit sensitivity**

Use an edit-selector enum covering a hash byte, path component, numeric limit, argv byte/flavor, operator, source, target, and fingerprint input; assert every actual canonical edit changes the fingerprint.

For the argv flavor selector, use identical payload units encoded once as `CommandArg::Unix` and once as `CommandArg::Windows` and assert the independent tagged model differs before requiring different fingerprints. Reject/no-op generated edits with `prop_assume!` so sensitivity is never satisfied vacuously by an unchanged input.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #167**

Run `cargo test -p hoimin-core --test resume_policy` and commit `test: extend fingerprint properties`.

### Task 16: Resolve #159 without a shared fixture crate

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-159-adversarial-fixtures.md`

**Interfaces:**
- Consumes: adversarial fixtures and former-defect tests added by concrete issues.
- Produces: evidence that concrete subsystem fixtures supersede the proposed shared crate.

- [ ] **Step 1: Add the missing analyzer variants locally**

Extend the analyzer's existing table beside `rust_tests` with exact benign/adversarial pairs: an ordinary string without an operator, ordinary strings containing each mutable operator token, nested single/double/triple quoting, f/t-string literal content, and f/t-string interpolation expressions. Assert literal content produces no candidate while mutable interpolation expressions do, and assert the ordinary-string/nested-quote cases preserve the surrounding string bytes. This discharges the still-missing ordinary-string/nested-quote audit row without a shared fixture crate.

- [ ] **Step 2: Complete the consumer-local benign/adversarial matrix**

Record one benign and every mapped adversarial variant for each cited consumer:

| Consumer | Benign variant | Required adversarial variants |
| --- | --- | --- |
| Analyzer | ordinary operator-free string | operator-bearing ordinary string, nested quotes, f/t literal content, f/t interpolation expression |
| Git diff | ordinary zero-context hunk content | content beginning `++`, `--`, `@@`, and `Binary files ... differ` from Task 9 |
| Workspace | regular UTF-8 file/tree below the depth bound | FIFO, non-UTF-8 name, exact supported depth, excessive depth, and case collision on a case-insensitive destination |
| Report/session serialization | minimal records | original/current JSON and JSONL plus SQLite v1/v2/v3 with every era-representable optional populated from Task 13 |

Each constructor stays beside its only consumer. If two tests in the same subsystem use identical semantics, place the constructor in that subsystem's existing test module; do not create a cross-domain fixture crate.

- [ ] **Step 3: Prove all-optionals local constructors are actually consumed**

Require the golden-corpus `all_optional_report_events(era)` constructor to exercise every era-representable optional on `RunStarted` and its nested config, `MutationCandidate`, `MutantFinished`, and `RunSummary`, and `all_optional_session_rows(version)` to exercise every era-representable nullable candidate/result column. Assert the corresponding non-null JSON pointers and SQL columns after round trip; helper construction without consumption and validation does not satisfy the row.

- [ ] **Step 4: Reconcile every #159 row before closure**

Link the already-covered interpolated strings, `++`/`--`, FIFO, non-UTF-8, depth, and case-collision tests; link Task 9 for `@@`/`Binary files ... differ`; link Task 13 for all-optionals constructors; and link Steps 1–2 for ordinary strings, nested quotes, and the complete benign/adversarial table. Run targeted analyzer, Git, workspace, report-handler, and session-handler suites.

- [ ] **Step 5: Close #159 only after the umbrella is fully covered**

Comment with links to the concrete PRs and explain why a cross-domain fixture
crate would add coupling without improving coverage. Close #159 only when every
#159 audit row is covered. If any class remains missing, keep #159 open or link
that exact row to a narrowed open follow-up before closing it.

### Task 17: Resolve #163 with narrow provenance guidance

**Files:**
- Modify: `docs/development.md`
- Create: `docs/superpowers/plans/2026-08-03-issue-163-test-provenance.md`
- Modify only qualifying tests added in Tasks 2–15.

**Interfaces:**
- Consumes: former-defect tests added by concrete issues.
- Produces: a narrow `pins:` comment convention without repository-wide backfill.

- [ ] **Step 1: Document the exact provenance rule**

Add:

```markdown
Use `// pins: issue #NNN` only when an assertion intentionally preserves a
surprising policy or a former defect whose expected result is not self-evident.
Do not annotate ordinary exact assertions.
```

- [ ] **Step 2: Annotate only qualifying new tests**

Examples include ordered empty-spool policy, hostile diff header boundaries, and cancellation during persistence. Do not perform a repository-wide backfill.

- [ ] **Step 3: Verify docs and tests**

```bash
git diff --check
cargo test --workspace
```

- [ ] **Step 4: Commit, PR, review, merge, and close #163**

Use commit `docs: define test provenance guidance`.

### Task 18: Final reconciliation and cleanup

**Files:**
- Modify: `docs/superpowers/reports/2026-08-03-test-issue-consolidation-audit.md`

**Interfaces:**
- Consumes: all merged PRs and issue states.
- Produces: final closed/remainder report and a clean worktree set.

- [ ] **Step 1: Update every audit row with merged evidence**

Replace each `missing` row with a commit-pinned test/PR link, or retain `platform remainder` with a direct link to the narrowed open follow-up that owns that exact row. Recount all statuses and assert the table has no `missing` rows before proposing closure of the consolidation.

- [ ] **Step 2: Query remaining open issues**

```bash
gh issue list --state open --limit 200 --json number,title
```

Filter the 16 scoped issues and record every explicit Windows/backend remainder plus its narrowed follow-up number. Assert no closed issue has an unlinked `missing` or `platform remainder` audit row.

- [ ] **Step 3: Commit, review, and merge the final report branch**

Create and use the exact branch `docs/test-issue-consolidation-final-report` from the latest `origin/main`, then run:

```bash
git diff --check
git add docs/superpowers/reports/2026-08-03-test-issue-consolidation-audit.md
git commit -m "docs: complete test issue consolidation report"
```

Push the final-report branch, pass CI and independent review, merge it, and record the merge commit. Cleanup does not begin before this merge is visible on `origin/main`.

- [ ] **Step 4: Synchronize and verify from the root checkout after merge**

Run these commands from the root checkout, not from an issue or final-report worktree:

```bash
cd /Users/hayao/RustroverProjects/hoimin
git status --short
git fetch origin main
git switch main
git merge --ff-only origin/main
git merge-base --is-ancestor "$(gh pr view docs/test-issue-consolidation-final-report --json mergeCommit --jq '.mergeCommit.oid')" HEAD
cargo test --workspace
```

Before synchronization, record the root checkout's untracked entries and verify `.idea/`, `.serena/`, and `tests/fixtures/projects/basic/uv.lock` are unchanged afterward. The ancestry command must exit 0 and the queried merge-commit OID must be non-empty.

- [ ] **Step 5: Remove merged worktrees from the root checkout**

Still in `/Users/hayao/RustroverProjects/hoimin`, run `git worktree list --porcelain` and `git branch --merged origin/main`. For each non-root entry, bind its printed absolute path to `worktree_path`; require it to be beneath `/Users/hayao/RustroverProjects/hoimin/.worktrees/`, require `git -C "$worktree_path" status --porcelain --untracked-files=no` to be empty, and require its branch to appear in `git branch --merged origin/main`. If `$worktree_path/.venv` is a symlink whose `readlink` is exactly `../../.venv`, remove only that symlink with `unlink "$worktree_path/.venv"`; otherwise stop. Then run `git worktree remove "$worktree_path"` for that individually verified path. Never remove the root checkout or an unmerged/dirty worktree.

- [ ] **Step 6: Verify cleanup and publish the final state**

From the root checkout, require `git worktree list` to contain only `/Users/hayao/RustroverProjects/hoimin`, rerun `git status --short`, and confirm the recorded root untracked entries remain. Report closed issues separately from the still-open narrowed Windows/backend follow-ups.
