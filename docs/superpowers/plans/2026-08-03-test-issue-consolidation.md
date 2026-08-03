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
- Consumes: `run_fixture_options`, `run_fixture_with_session`, and the live-run ownership tests.
- Produces: field-for-field fresh-session/sessionless parity and cross-process live-run refusal evidence.

- [ ] **Step 1: Audit the two requested criteria**

Confirm that `fresh_session_and_sessionless_results_preserve_the_same_termination` compares complete mutant documents modulo volatile fields, and that a subprocess ownership test holds a live run while another handler attempts resume.

- [ ] **Step 2: Add only a missing cross-process assertion**

If the second criterion is absent, add a test whose core assertion is:

```rust
assert_eq!(second.exit_code, 2);
assert!(second.stderr.contains("already owned"), "{}", second.stderr);
assert_eq!(result_count(&database), 0);
```

- [ ] **Step 3: Run focused verification**

```bash
cargo test -p hoimin-cli --test run_e2e fresh_session_and_sessionless_results_preserve_the_same_termination
cargo test -p hoimin-cli --test session_handler a_live_run_cannot_be_resumed_by_another_handler
```

- [ ] **Step 4: Close evidence-only or commit, PR, and merge**

For evidence-only closure, comment with both test links and close #155. Otherwise commit with `test: complete session report parity coverage`, create a PR with `Closes #155`, wait for CI, review, and merge.

### Task 3: Resolve #153 real-signal cancellation coverage

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-153-real-signals.md`

**Interfaces:**
- Consumes: `spawn_second_sigint_fixture`, `wait_for_jsonl_kind`, and Unix `libc::kill` support.
- Produces: a real first-SIGINT report test and retained second-SIGINT escalation test.

- [ ] **Step 1: Write the Unix first-signal test**

Add a `#[cfg(unix)]` test whose terminal assertions are:

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

Factor only test helpers needed to send one or two signals. Keep `second_sigint_forces_130_while_session_finish_is_blocked` intact.

- [ ] **Step 4: Verify both real-signal paths**

```bash
cargo test -p hoimin-cli --test run_e2e first_sigint_finishes_a_parseable_incomplete_session
cargo test -p hoimin-cli --test run_e2e second_sigint_forces_130_while_session_finish_is_blocked
```

- [ ] **Step 5: Document Windows as covered or retained**

Do not emulate `GenerateConsoleCtrlEvent`. If a stable Windows console-group test is unavailable, record it as the sole remainder in #153 and keep or split the issue accordingly.

- [ ] **Step 6: Commit, PR, review, and merge**

Commit with `test: exercise cancellation through real signals`; the PR closes only the Unix acceptance portion and must state the Windows disposition.

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

Assert that stdout parses as one JSON document and a deliberately emitted diagnostic appears only on stderr.

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
- Produces: `hoimin progress` acceptance of two real reports and the oldest supported report.

- [ ] **Step 1: Generate two reports with the compiled binary**

Write each stdout document to a temporary file and validate both runs succeeded.

- [ ] **Step 2: Invoke real progress**

```rust
let output = Command::new(env!("CARGO_BIN_EXE_hoimin"))
    .args(["progress", "--format", "json"])
    .arg(&before)
    .arg(&after)
    .output()
    .unwrap();
assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok());
```

- [ ] **Step 3: Add the oldest/current mixed comparison**

Pass `schema-v2-original.json` and a current generated report together and assert the failure mode is semantic eligibility, never deserialization.

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

The source must contain independently mutable operators in both functions; only one function is changed after the initial commit.

- [ ] **Step 2: Add `run --changed` acceptance**

```rust
assert!(mutants.iter().all(|m| m["candidate"]["symbol"] == "changed"));
assert!(mutants.iter().all(|m| m["candidate"]["path"] == "src/calc.py"));
```

- [ ] **Step 3: Add `plan --changed --diff-base <base>` acceptance**

Assert the manifest candidates equal the changed function's candidates and exclude the untouched function.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #156**

Run the two exact integration tests and commit `test: exercise changed selection through the CLI`.

### Task 7: Resolve #161 session contention matrix

**Files:**
- Modify: `crates/hoimin-cli/tests/session_handler.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-161-session-contention.md`

**Interfaces:**
- Consumes: `SessionHandler`, a second rusqlite connection, and explicit transaction barriers.
- Produces: bounded outcomes for `begin`, `persist`, `lookup`, `finish`, and `load` under contention.

- [ ] **Step 1: Add a table-driven operation enum**

```rust
enum ContendedOperation { Begin, Persist, Lookup, Finish, Load }
```

- [ ] **Step 2: Hold `BEGIN IMMEDIATE` across each operation**

Release the lock before the configured timeout and assert the operation succeeds rather than surfacing `DatabaseBusy`.

- [ ] **Step 3: Hold a read snapshot across read-then-write operations**

Assert each write either obtains its immediate transaction after release or returns the documented typed failure; never accept raw `DatabaseBusy` text.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #161**

Run `cargo test -p hoimin-cli --test session_handler contention` and commit `test: cover session operations under contention`.

### Task 8: Resolve #162 platform process-backend fault coverage

**Files:**
- Modify if missing: `crates/hoimin-cli/src/resource/windows.rs`
- Modify if missing: `crates/hoimin-cli/src/resource/linux.rs`
- Modify if missing: `crates/hoimin-cli/tests/process_handler.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-162-process-backend-faults.md`

**Interfaces:**
- Consumes: existing abnormal-exit, root-generation, and cgroup notification tests.
- Produces: evidence that stale roots cannot signal raw recycled PIDs and abnormal roots classify without timeout.

- [ ] **Step 1: Audit current Windows unit tests against all three scenarios**

Map `abnormal_exit_marks_only_its_root_while_other_roots_remain_active` and the root-generation tests to crash, descendants, and stale-PID criteria.

- [ ] **Step 2: Audit Linux cleanup/accounting tests**

Confirm kernel kill paths use the opened cgroup handle and never fall back to a numeric PID or process group.

- [ ] **Step 3: Add only a missing deterministic backend test**

The safety assertion must be equivalent to:

```rust
assert_eq!(signals.numeric_pid_calls(), 0);
assert_eq!(kernel_group.kill_calls(), 1);
```

- [ ] **Step 4: Verify native and cross-compiled suites**

Run targeted Linux tests locally and rely on Windows CI for `resource::windows` tests. Do not add timing-based PID-reuse loops.

- [ ] **Step 5: Close with evidence or merge a focused PR**

Use commit `test: cover process backend fault boundaries` only if code changes are required.

### Task 9: Resolve #165 hostile Git diff properties

**Files:**
- Modify: `crates/hoimin-cli/src/target/git.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-165-git-diff-properties.md`

**Interfaces:**
- Consumes: private `parse_diff`, `decode_git_quoted`, and proptest.
- Produces: hostile zero-context diff parsing and total quoted decoder properties.

- [ ] **Step 1: Add a generated diff-section strategy**

Bias generated content toward `++ b/evil.py`, `-- a/evil.py`, `@@ -1 +1 @@`, and `Binary files a/x.py and b/y.py differ`.

- [ ] **Step 2: Assert parser results against generated edit ranges**

```rust
prop_assert_eq!(parse_diff(rendered.as_bytes())?, expected_changes);
```

- [ ] **Step 3: Extend quoted decoder coverage**

```rust
proptest!(|(quoted in arbitrary_quoted_git_path())| {
    let result = std::panic::catch_unwind(|| decode_git_quoted(&quoted));
    prop_assert!(result.is_ok());
});
```

- [ ] **Step 4: Run, commit, PR, review, merge, and close #165**

Run `cargo test -p hoimin-cli target::git` and commit `test: property test hostile Git diff parsing`.

### Task 10: Resolve #168 progress-comparison properties

**Files:**
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-168-progress-properties.md`

**Interfaces:**
- Consumes: `compare_reports`, arbitrary `MutantFinished` vectors, and all mutation statuses.
- Produces: self-comparison, reversal symmetry, and order invariance.

- [ ] **Step 1: Generate valid reports with unique stable IDs and duplicate content keys**

- [ ] **Step 2: Assert self-comparison**

```rust
prop_assert_eq!(comparison.added, 0);
prop_assert_eq!(comparison.removed, 0);
prop_assert_eq!(comparison.improvements, 0);
prop_assert_eq!(comparison.regressions, 0);
```

- [ ] **Step 3: Assert reversal symmetry and order invariance**

Swap before/after, compare improvements with regressions and added with removed, and negate the score delta. Shuffle mutants within each report and assert the complete comparison is unchanged.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #168**

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

Map generated indexes modulo the current pending-effect count; stop after a fixed step cap and drain deterministically.

- [ ] **Step 3: Assert machine invariants**

Assert one `RunFinished`, no reused effect IDs, one started/finished pair per scheduled candidate, summary counts equal finished statuses, and every emitted record passes `ReportSequence::observe`.

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
- Produces: timeout status/count/completeness/exit-code proof and survivor exit-code proof.

- [ ] **Step 1: Add the timeout mutant fixture**

```rust
assert_eq!(run.exit_code, 4);
assert_eq!(run.statuses, ["timeout"]);
assert_eq!(run.document["summary"]["counts"]["timeout"], 1);
assert_eq!(run.document["summary"]["complete"], false);
```

- [ ] **Step 2: Add the missing survivor exit assertion**

Assert `ty_reports_a_surviving_nullable_contract_mutant` exits with code 1.

- [ ] **Step 3: Audit enforceable OOM/process-limit backends**

Add real end-to-end cases only on a runner with delegated cgroup-v2 enforcement. Otherwise document those two criteria as backend remainders and leave a narrowed follow-up open.

- [ ] **Step 4: Run, commit, PR, review, and merge**

Use `cargo test -p hoimin-cli --test run_e2e timeout_mutant` and commit `test: cover terminal mutant statuses end to end`.

### Task 13: Resolve #160 serialized golden corpus

**Files:**
- Create: `crates/hoimin-cli/tests/golden/reports/schema-v2-original.json`
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
- Produces: checked historical artifacts and semantic drift checks.

- [ ] **Step 1: Move the original report fixture into the golden layout**

Update `original_schema_v2_report()` to the canonical golden path.

- [ ] **Step 2: Generate all-optionals-populated current JSON and JSONL fixtures**

Normalize volatile run IDs, output tokens, and elapsed milliseconds to fixed values before writing the checked fixtures.

- [ ] **Step 3: Create SQLite schema-era fixtures**

Populate each database with one run, fingerprint, candidate, result, and diagnostic valid for that era. Open every fixture through `SessionHandler` and assert migration plus preserved rows.

- [ ] **Step 4: Add semantic regeneration tests**

Parse JSON/JSONL into typed values and compare them; query SQLite schema and rows rather than comparing database bytes.

- [ ] **Step 5: Run, commit, PR, review, merge, and close #160**

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
for offset in observed_offsets {
    prop_assert_eq!(replay_from(&spool, offset)?, expected_suffix(offset));
}
```

- [ ] **Step 3: Pin serialized boundaries**

Construct records whose JSON payload lengths are `MAX_SPOOL_RECORD_BYTES - 2`, `-1`, and exactly `MAX_SPOOL_RECORD_BYTES`; assert the two documented accepted/rejected sides including newline accounting.

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

- [ ] **Step 2: Assert normalization invariance**

```rust
prop_assert_eq!(fingerprint(&value), fingerprint(&permuted_and_duplicated));
```

- [ ] **Step 3: Assert one-field edit sensitivity**

Use an edit-selector enum covering a hash byte, path component, numeric limit, argv byte/flavor, operator, source, target, and fingerprint input; assert every actual canonical edit changes the fingerprint.

- [ ] **Step 4: Run, commit, PR, review, merge, and close #167**

Run `cargo test -p hoimin-core --test resume_policy` and commit `test: extend fingerprint properties`.

### Task 16: Resolve #159 without a shared fixture crate

**Files:**
- Create: `docs/superpowers/plans/2026-08-03-issue-159-adversarial-fixtures.md`

**Interfaces:**
- Consumes: adversarial fixtures and former-defect tests added by concrete issues.
- Produces: evidence that concrete subsystem fixtures supersede the proposed shared crate.

- [ ] **Step 1: Map every cited blind spot to a concrete test**

Record the analyzer, Git diff, serialization, workspace, and session tests that
now exercise the adversarial input classes cited by #159.

- [ ] **Step 2: Verify fixture locality**

Confirm each adversarial constructor lives beside its only consumer. If two
consumers have identical semantics, extract only that constructor into the
existing test module shared by those consumers, not a new crate.

- [ ] **Step 3: Close #159 as a superseded umbrella**

Comment with links to the concrete PRs and explain why a cross-domain fixture
crate would add coupling without improving coverage. No code PR is required
when all cited classes are covered.

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

- [ ] **Step 2: Query remaining open issues**

```bash
gh issue list --state open --limit 200 --json number,title
```

Filter the 16 scoped issues and record any explicit Windows/backend remainder.

- [ ] **Step 3: Run final verification on current main**

```bash
git fetch origin main
git switch main
git merge --ff-only origin/main
cargo test --workspace
```

- [ ] **Step 4: Remove merged worktrees**

Unlink each worktree-local `.venv`, remove only merged worktrees, and confirm `git worktree list` contains the root checkout only.

- [ ] **Step 5: Commit and merge the final report**

Use commit `docs: complete test issue consolidation report`, pass CI, merge, and report closed issues and retained platform-specific work separately.
