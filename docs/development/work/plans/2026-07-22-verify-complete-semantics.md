# Verify JSON `complete` Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the final JSON and session `complete` value false whenever mutant results or run-level failures make the run inconclusive, so it cannot conflict with exit code 4.

**Architecture:** Centralize `RunState` outcome construction in one private `exit_policy` method, derive `complete` from that policy, and reuse both methods for exit status, session finalization, and the final report. Preserve report schema version 2 because the JSON shape is unchanged.

**Tech Stack:** Rust, serde JSON reports, the hoimin state machine, Cargo tests, Markdown documentation.

## Global Constraints

- Keep `REPORT_SCHEMA_VERSION` and the JSON Schema version at `2`.
- Do not add or rename public JSON fields.
- Do not change timeout calculation, process classification, or exit-code precedence.
- A normal zero-candidate run remains complete.
- Use test-driven development: observe the regression test failing before modifying production code.

---

### Task 1: Unify State-Machine Completion Policy

**Files:**
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-core/src/machine.rs:297`
- Modify: `crates/hoimin-core/src/machine.rs:838`

**Interfaces:**
- Consumes: `ExitPolicy::from_summary(&MutationSummary)` and `exit_code_for(ExitPolicy)`.
- Produces: private `RunState::exit_policy(&self) -> ExitPolicy` and `RunState::complete(&self) -> bool`, shared by `exit_code`, `FinishSession.complete`, and `RunSummary.complete`.

- [ ] **Step 1: Add the failing timeout/session regression test**

Add the imports `CleanupFinished`, `OriginalsVerified`, `SessionFinished`, and `WorkerReset` to `crates/hoimin-core/tests/machine.rs`. Add a test named
`timeout_marks_the_session_and_final_report_incomplete` next to the session lifecycle tests. Drive one session-backed candidate through these existing state-machine effects in order:

```rust
let (state, effects) = waiting_for_session_candidate(false);
// CandidateLoaded(Some) -> StoredResultLoaded(None) -> MutationApplied
// -> OutputEmitted(MutantStarted) -> MutantFinished(Timeout)
// -> ResultPersisted -> OutputEmitted(MutantFinished) -> WorkerReset
// -> CandidateLoaded(None) -> OriginalsVerified -> CleanupFinished
```

Use the IDs and payloads carried by each emitted effect, as existing tests do. At finalization, assert the public behavior before acknowledging each effect:

```rust
let RunEffect::FinishSession(finish) = find_effect(&effects, |effect| {
    matches!(effect, RunEffect::FinishSession(_))
}) else {
    unreachable!()
};
assert!(!finish.complete);

let (state, effects) = transition(
    state,
    RunEvent::SessionFinished(SessionFinished {
        id: finish.id,
        run_id: finish.run_id.clone(),
        complete: finish.complete,
    }),
)
.unwrap();
let RunEffect::EmitOutput(output) = find_effect(&effects, |effect| {
    matches!(effect, RunEffect::EmitOutput(value)
        if matches!(&value.event, OutputEvent::RunFinished(_)))
}) else {
    unreachable!()
};
let OutputEvent::RunFinished(summary) = &output.event else {
    unreachable!()
};
assert!(!summary.complete);
assert_eq!(summary.exit_code, 4);
assert_eq!(summary.counts.timeout, 1);
```

Build the candidate/session portion with the same event payloads already exercised by
`session_result_is_persisted_before_finished_output_and_reset`: load `fixture_candidate(1)`, return
`StoredResultLoaded { result: None }`, acknowledge `MutationApplied` and mutant-started output,
return `MutantFinished(process_finished(mutant_id, ProcessTermination::Timeout))`, acknowledge the
matching `ResultPersisted`, mutant-finished output, and `WorkerReset`, then return
`CandidateLoaded { candidate: None, next_offset: 1 }`. Finish finalization with the effect payloads
the state machine emits:

```rust
let RunEffect::VerifyOriginals(verify) = find_effect(&effects, |effect| {
    matches!(effect, RunEffect::VerifyOriginals(_))
}) else {
    unreachable!()
};
let (state, effects) = transition(
    state,
    RunEvent::OriginalsVerified(OriginalsVerified {
        id: verify.id,
        checkpoint: verify.checkpoint.clone(),
    }),
)
.unwrap();
let RunEffect::Cleanup(cleanup) = find_effect(&effects, |effect| {
    matches!(effect, RunEffect::Cleanup(_))
}) else {
    unreachable!()
};
let (state, effects) = transition(
    state,
    RunEvent::CleanupFinished(CleanupFinished {
        id: cleanup.id,
        released_reservations: cleanup.reservations.clone(),
    }),
)
.unwrap();
```

- [ ] **Step 2: Run the regression test and verify RED**

Run:

```console
cargo test -p hoimin-core --test machine timeout_marks_the_session_and_final_report_incomplete -- --exact
```

Expected: FAIL at `assert!(!finish.complete)` because the timeout exists only in `MutationSummary`, while the current session completion expression ignores it.

- [ ] **Step 3: Centralize policy construction and completion**

Replace the current `exit_code` implementation in `crates/hoimin-core/src/machine.rs` with:

```rust
fn exit_policy(&self) -> ExitPolicy {
    let summary_policy = ExitPolicy::from_summary(&self.summary);
    ExitPolicy {
        infrastructure_error: self.flags.outcome.infrastructure_error
            || summary_policy.infrastructure_error,
        baseline_failed: self.flags.outcome.baseline_failed,
        incomplete: self.flags.outcome.incomplete || summary_policy.incomplete,
        survivors: summary_policy.survivors,
        interrupted: self.flags.report.interrupted,
    }
}

fn complete(&self) -> bool {
    let policy = self.exit_policy();
    !policy.infrastructure_error
        && !policy.baseline_failed
        && !policy.incomplete
        && !policy.interrupted
}

#[must_use]
pub fn exit_code(&self) -> i32 {
    exit_code_for(self.exit_policy())
}
```

In `final_report_effects`, replace the three-condition expression assigned to
`RunSummary.complete` with:

```rust
complete: self.complete(),
```

Replace `post_cleanup_effects` with the complete method below so completion is computed before the
session effect is constructed:

```rust
fn post_cleanup_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
    self.phase = RunPhase::Finalize;
    let complete = self.complete();
    if let Some(run_id) = self.session_run_id.clone()
        && !self.flags.cleanup.session_finish_attempted
    {
        self.flags.cleanup.session_finish_attempted = true;
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::FinishSession(FinishSession {
            id,
            run_id,
            complete,
        })])
    } else {
        self.final_report_effects()
    }
}
```

Do not derive completion from `exit_code == 0 || exit_code == 1`; that would obscure the independent policy fields and make future precedence changes alter the JSON contract accidentally.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run:

```console
cargo test -p hoimin-core --test machine timeout_marks_the_session_and_final_report_incomplete -- --exact
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-core --test machine
```

Expected: all commands PASS. Existing killed/survived and zero-candidate tests continue to demonstrate `complete: true`; infrastructure, baseline, cancellation, and limit tests remain incomplete under the centralized policy.

- [ ] **Step 5: Format and commit the behavior change**

Run:

```console
cargo fmt --all
git add crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs
git commit -m "fix: align verify completion with mutant outcomes"
```

Expected: one commit containing the failing-first regression test and the minimal state-machine fix.

---

### Task 2: Document the Public `complete` Contract

**Files:**
- Modify: `README.md:142`
- Modify: `crates/hoimin-cli/tests/report_handler.rs:63`

**Interfaces:**
- Consumes: README text loaded by `documentation_contract`.
- Produces: an explicit user-facing definition that `complete` is false for every inconclusive mutant status and for run-level failure, without changing JSON schema 2.

- [ ] **Step 1: Add a failing documentation assertion**

Immediately after reading `README.md` in `documentation_contract`, add:

```rust
assert!(
    readme.contains(
        "`complete` is `false` when any mutant is inconclusive or the run fails or is interrupted"
    ),
    "README must define the machine-readable complete field",
);
```

- [ ] **Step 2: Run the documentation test and verify RED**

Run:

```console
cargo test -p hoimin-cli --test report_handler documentation_contract -- --exact
```

Expected: FAIL with `README must define the machine-readable complete field`.

- [ ] **Step 3: Add the precise README contract**

After the paragraph describing JSON/JSONL event kinds, add:

```markdown
The final summary's `complete` is `false` when any mutant is inconclusive or the run fails or is interrupted. It is `true` only when every selected mutant is `killed` or `survived` and no run-level failure occurred; a successful run with no candidates is also complete. Therefore an exit code of `4` always has `complete: false`.
```

Keep the existing status list and exit-code table unchanged.

- [ ] **Step 4: Run the documentation and schema tests**

Run:

```console
cargo test -p hoimin-cli --test report_handler documentation_contract -- --exact
cargo test -p hoimin-cli --test progress
```

Expected: both commands PASS, including the existing schema version 2 validation.

- [ ] **Step 5: Commit the documentation contract**

Run:

```console
git add README.md crates/hoimin-cli/tests/report_handler.rs
git commit -m "docs: define mutation report completion"
```

Expected: a second commit containing only the README contract and its regression assertion.

---

### Task 3: Verify the Complete Change

**Files:**
- Verify only; no source changes expected.

**Interfaces:**
- Consumes: all changes from Tasks 1 and 2.
- Produces: release-quality evidence that formatting, linting, contracts, and the workspace suite pass.

- [ ] **Step 1: Check formatting**

Run:

```console
cargo fmt --all -- --check
```

Expected: exit code 0 with no diff.

- [ ] **Step 2: Run strict linting**

Run:

```console
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit code 0 with no warnings.

- [ ] **Step 3: Run the full workspace test suite**

Run:

```console
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
```

Expected: every test passes, including the new timeout completion regression and README contract.

- [ ] **Step 4: Confirm scope and commit history**

Run:

```console
git diff --check main...HEAD
git status --short
git log --oneline main..HEAD
```

Expected: no whitespace errors, a clean status, and exactly the design commit plus the two implementation commits created by this plan.
