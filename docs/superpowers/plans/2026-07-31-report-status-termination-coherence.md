# Report Status/Termination Coherence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject `MutantFinished` report events whose concrete process termination classifies to a different mutation status.

**Architecture:** Keep the invariant at the existing `ReportSequence` trust boundary and reuse `classify_mutant` as the sole status classifier. A missing termination remains valid for synthetic, reused, legacy, and infrastructure-error results; when termination is present, validation returns a typed error before advancing sequence or mutant lifecycle state.

**Tech Stack:** Rust, `thiserror`, Hoimin core report model, Cargo test/Clippy, optional `contracts` feature

## Global Constraints

- Do not change `REPORT_SCHEMA_VERSION` or the serialized `MutantFinished` shape.
- Validate only `termination: Some(_)`; `termination: None` is a supported representation and must remain accepted.
- Preserve current error precedence: terminal/run identity and mutant identity/lifecycle errors precede coherence, and coherence precedes monotonic sequence validation.
- Reuse `classify_mutant`; do not duplicate its termination-to-status mapping.
- A rejected finish must not consume the active mutant or advance `ReportSequence.last`.
- Issue #151 owns carrying and persisting termination for session-backed fresh results; Issue #114 only validates whatever termination is present.

---

## File Structure

- Modify `crates/hoimin-core/src/report.rs`: define the typed mismatch error and enforce coherence inside `ReportSequence`.
- Modify `crates/hoimin-core/tests/report_policy.rs`: cover mismatch rejection, all valid classifications, absent termination, state preservation after rejection, and the contracts build.

No CLI, session schema, machine, or public JSON-schema file changes belong in this issue.

### Task 1: Enforce status/termination coherence at the report sequence boundary

**Files:**

- Modify: `crates/hoimin-core/src/report.rs:335-510`
- Test: `crates/hoimin-core/tests/report_policy.rs:130-480`

**Interfaces:**

- Consumes: `classify_mutant(ProcessTermination) -> MutationStatus`, `MutantFinished { candidate, status, termination, .. }`, and the existing `ReportSequence::observe(&OutputEvent) -> Result<(), ReportSequenceError>` validation order.
- Produces: `ReportSequenceError::MutantStatusTerminationMismatch { mutant_id: String, mutant_sequence: u64, status: MutationStatus, termination: ProcessTermination, expected_status: MutationStatus }`; `ReportSequence::observe` rejects a coherent-lifecycle `MutantFinished` when `termination.is_some()` and `status != classify_mutant(termination)`.

- [ ] **Step 1: Add a focused event helper and failing runtime regression tests**

In `crates/hoimin-core/tests/report_policy.rs`, retain `finished_event` as the common killed/exit-1 fixture, but route it through a configurable helper:

```rust
fn finished_event(sequence: u64, candidate: MutationCandidate) -> OutputEvent {
    finished_event_with(
        sequence,
        candidate,
        MutationStatus::Killed,
        Some(ProcessTermination::Exit(1)),
    )
}

fn finished_event_with(
    sequence: u64,
    candidate: MutationCandidate,
    status: MutationStatus,
    termination: Option<ProcessTermination>,
) -> OutputEvent {
    OutputEvent::MutantFinished(MutantFinished {
        schema_version: 1,
        sequence,
        run_id: "run-1".to_owned(),
        candidate,
        status,
        termination,
        elapsed_ms: 2,
        resource_mode: ResourceMode::Hard,
        output: None,
    })
}
```

Add a default-feature regression that starts mutant `m1`, submits `Survived + Exit(1)`, asserts the exact typed error, then submits a corrected finish at the same event sequence and completes the run. Reusing sequence `3` proves the rejected event changed neither `last` nor the active-mutant set:

```rust
#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_rejects_status_that_disagrees_with_termination_without_advancing() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();

    let inconsistent = finished_event_with(
        3,
        candidate("m1", 7),
        MutationStatus::Survived,
        Some(ProcessTermination::Exit(1)),
    );
    assert_eq!(
        sequence.observe(&inconsistent),
        Err(
            hoimin_core::ReportSequenceError::MutantStatusTerminationMismatch {
                mutant_id: "m1".to_owned(),
                mutant_sequence: 7,
                status: MutationStatus::Survived,
                termination: ProcessTermination::Exit(1),
                expected_status: MutationStatus::Killed,
            }
        )
    );

    sequence
        .observe(&finished_event(3, candidate("m1", 7)))
        .unwrap();
    sequence.observe(&run_finished_event(4)).unwrap();
}
```

Add a table-driven acceptance regression for every classifier output, plus an explicit missing-termination case. Give each case a distinct mutant identity and monotonically increasing report sequence:

```rust
#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_accepts_every_classified_status_and_an_absent_termination() {
    let cases = [
        (MutationStatus::Survived, Some(ProcessTermination::Exit(0))),
        (MutationStatus::Killed, Some(ProcessTermination::Exit(7))),
        (MutationStatus::Timeout, Some(ProcessTermination::Timeout)),
        (
            MutationStatus::OutOfMemory,
            Some(ProcessTermination::OutOfMemory),
        ),
        (
            MutationStatus::ProcessLimit,
            Some(ProcessTermination::ProcessLimit),
        ),
        (MutationStatus::NotRun, Some(ProcessTermination::Cancelled)),
        (MutationStatus::Error, None),
    ];
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)))
        .unwrap();

    let mut event_sequence = 2;
    for (index, (status, termination)) in cases.into_iter().enumerate() {
        let mutant_id = format!("m{index}");
        let mutant_sequence = u64::try_from(index).unwrap();
        sequence
            .observe(&OutputEvent::MutantStarted(MutantStarted::new(
                "run-1",
                event_sequence,
                &mutant_id,
                mutant_sequence,
            )))
            .unwrap();
        event_sequence += 1;
        sequence
            .observe(&finished_event_with(
                event_sequence,
                candidate(&mutant_id, mutant_sequence),
                status,
                termination,
            ))
            .unwrap();
        event_sequence += 1;
    }
}
```

- [ ] **Step 2: Run the focused tests and verify the new mismatch test fails**

Run:

```bash
cargo test -p hoimin-core --test report_policy
```

Expected: compilation fails because `ReportSequenceError::MutantStatusTerminationMismatch` does not exist yet. This is the RED phase; do not weaken the assertion.

- [ ] **Step 3: Add the typed error and minimal coherence check**

In `ReportSequenceError`, add:

```rust
#[error(
    "mutant {mutant_id} sequence {mutant_sequence} status {status:?} disagrees with termination {termination:?}; expected {expected_status:?}"
)]
MutantStatusTerminationMismatch {
    mutant_id: String,
    mutant_sequence: u64,
    status: MutationStatus,
    termination: ProcessTermination,
    expected_status: MutationStatus,
},
```

In the `OutputEvent::MutantFinished` arm of `ReportSequence::mutant_error`, keep stable-identity mismatch first and `MutantNotStarted` second. Only after the active key has been confirmed, validate a present termination:

```rust
let key = (value.candidate.id.clone(), value.candidate.sequence);
if !self.active_mutants.contains(&key) {
    Some(ReportSequenceError::MutantNotStarted {
        mutant_id: value.candidate.id.clone(),
        mutant_sequence: value.candidate.sequence,
    })
} else if let Some(termination) = value.termination {
    let expected_status = classify_mutant(termination);
    (value.status != expected_status).then(|| {
        ReportSequenceError::MutantStatusTerminationMismatch {
            mutant_id: value.candidate.id.clone(),
            mutant_sequence: value.candidate.sequence,
            status: value.status,
            termination,
            expected_status,
        }
    })
} else {
    None
}
```

Do not add another mapping, mutate the event, synthesize termination from status, or reject `None`. The existing early return in `observe` ensures a mismatch cannot update `last` or remove the active mutant.

- [ ] **Step 4: Run the core report suite and verify the runtime behavior passes**

Run:

```bash
cargo test -p hoimin-core --test report_policy
```

Expected: all `report_policy` tests pass with default features.

- [ ] **Step 5: Add a contracts-feature regression for the new invariant**

Add beside the existing contract tests:

```rust
#[cfg(feature = "contracts")]
#[test]
#[should_panic(expected = "report.sequence.invariant")]
fn incoherent_mutant_finish_trips_the_ci_contract() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();
    let _ = sequence.observe(&finished_event_with(
        3,
        candidate("m1", 7),
        MutationStatus::Survived,
        Some(ProcessTermination::Exit(1)),
    ));
}
```

- [ ] **Step 6: Run focused default and contract verification**

Run:

```bash
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-core --features contracts --test report_policy
```

Expected: both invocations pass. The contract invocation must panic only in tests annotated with `#[should_panic]`.

- [ ] **Step 7: Run repository-wide quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
git diff --check
```

Expected: every command exits zero. Confirm no snapshots, schema files, or session code changed.

- [ ] **Step 8: Commit the implementation**

```bash
git add crates/hoimin-core/src/report.rs crates/hoimin-core/tests/report_policy.rs
git commit -m "fix: validate report status termination coherence"
```

## Dependency and Integration Notes

- #114 can compile and land independently of #151 because `MutantFinished.termination` is already optional. Before #151, session-backed fresh results remain `None` and therefore retain their existing behavior.
- Once #151 makes freshly executed session results emit `Some(ProcessTermination)`, this invariant validates them without further #114 changes. #151 fixtures must pair status with termination through `classify_mutant`; legacy and reused rows may remain `None`.
- If #151 lands first, rebase this branch before implementation. The expected overlap is limited to tests/fixtures unless #151 changes `crates/hoimin-core/tests/report_policy.rs`; do not move persistence or decoding checks into this issue.
- `MutationStatus::Error` has no corresponding `ProcessTermination` classification. It is valid with `None` and must be rejected with any `Some(...)` value.
