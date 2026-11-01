# Ordered Empty-Spool Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject ordered verification when selected candidates are absent and report truncated filtered discovery as incomplete without skipping retained candidates.

**Architecture:** Centralize the selected-candidate presence invariant inside `RunState` while preserving ordered manifest order and unordered `BTreeSet` behavior. Apply truncation to the existing outcome flag before filtered replay, then update the real CLI contract and user documentation to reflect exit code 4 for partial candidate discovery.

**Tech Stack:** Rust 2024, `hoimin-core` pure state machine, Tokio CLI integration tests, Markdown.

## Global Constraints

- Ordered verification with one or more requested IDs must return `MachineError::SelectedCandidateMissing` when analysis yields an empty spool.
- Empty ordered and unordered request collections remain valid internal states and may finalize without a missing-candidate error.
- Ordered missing-ID selection preserves manifest order and reports the first missing requested ID.
- Unordered missing-ID selection preserves the existing `BTreeSet` iteration order.
- A truncated filtered spool remains replayable; retained selected candidates still execute.
- Any truncated analysis marks the outcome incomplete, so incomplete exit code 4 takes precedence over survivors.
- Do not change CLI routing, public schemas, error codes, or session behavior.

---

### Task 1: Enforce Candidate Presence and Truncation in the State Machine

**Files:**
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-core/src/machine.rs`

**Interfaces:**
- Consumes: `RunState::candidate_filter`, `RunState::ordered_candidates`, `OrderedCandidateState::{ids,discovered}`, and `RunState::matched_candidate_ids`.
- Produces: `RunState::ensure_selected_candidates_discovered(&self) -> Result<(), MachineError>` and consistent missing-candidate validation at all spool completion points.

- [ ] **Step 1: Replace the test that pins successful ordered empty-spool finalization**

Replace `ordered_candidate_filter_preserves_empty_spool_finalization` with:

```rust
#[test]
fn ordered_candidate_filter_rejects_an_empty_spool() {
    let requested = fixture_candidate(1);
    let requested_id = requested.id.clone();
    let (state, effects) = waiting_for_ordered_analysis(vec![requested.id]);
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));

    let error = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "empty-ordered".to_owned(),
                records: 0,
            }),
            truncated: false,
        }),
    )
    .unwrap_err();

    assert_eq!(
        error,
        MachineError::SelectedCandidateMissing(requested_id)
    );
}
```

- [ ] **Step 2: Add a failing truncated ordered-replay test**

Add beside the empty-spool tests:

```rust
#[test]
fn ordered_candidate_filter_marks_truncated_analysis_incomplete_but_replays() {
    let requested = fixture_candidate(1);
    let (state, effects) = waiting_for_ordered_analysis(vec![requested.id]);
    let analysis_id = effect_id(find_effect(&effects, |effect| {
        matches!(effect, RunEffect::AnalyzeFile(_))
    }));

    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis_id,
            spool: Some(CandidateSpoolRef {
                token: "truncated-ordered".to_owned(),
                records: 1,
            }),
            truncated: true,
        }),
    )
    .unwrap();

    assert_eq!(state.phase(), RunPhase::Mutants);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RunEffect::ReadCandidate(_)))
    );
    assert_eq!(state.exit_code(), 4);
}
```

- [ ] **Step 3: Run both tests and verify RED**

Run:

```console
cargo test -p hoimin-core --test machine \
  ordered_candidate_filter_rejects_an_empty_spool -- --exact
cargo test -p hoimin-core --test machine \
  ordered_candidate_filter_marks_truncated_analysis_incomplete_but_replays -- --exact
```

Expected: the empty-spool test fails because transition returns `Ok` and
enters `Finalize`; the truncated test fails because `exit_code()` remains 0.

- [ ] **Step 4: Centralize the selected-candidate invariant**

Add this private method to `impl RunState` near
`collect_ordered_candidate`:

```rust
fn ensure_selected_candidates_discovered(&self) -> Result<(), MachineError> {
    let missing = if let Some(ordered) = &self.ordered_candidates {
        if ordered.collection_complete {
            None
        } else {
            ordered
                .ids
                .iter()
                .find(|candidate_id| !ordered.discovered.contains_key(*candidate_id))
        }
    } else {
        self.candidate_filter.as_ref().and_then(|candidate_filter| {
            candidate_filter
                .iter()
                .find(|candidate_id| !self.matched_candidate_ids.contains(*candidate_id))
        })
    };
    if let Some(candidate_id) = missing {
        return Err(MachineError::SelectedCandidateMissing(
            candidate_id.clone(),
        ));
    }
    Ok(())
}
```

Call `self.ensure_selected_candidates_discovered()?` at ordered collection
EOF before moving `ordered.discovered` into `ordered.ready`. Replace the
duplicated `CandidateLoaded(None)` missing-ID search with the same helper.
The `collection_complete` guard makes later ordered replay EOF a no-op because
ordered presence was already validated before `discovered` moved to `ready`.

In the filtered `AnalysisFinished` branch:

```rust
state.flags.outcome.incomplete |= value.truncated;
```

must run before matching `value.spool`. For a zero-record spool, call
`state.ensure_selected_candidates_discovered()?` before finalization. Do not
short-circuit nonempty truncated replay.

- [ ] **Step 5: Run focused and core verification**

Run:

```console
cargo test -p hoimin-core --test machine \
  ordered_candidate_filter_rejects_an_empty_spool -- --exact
cargo test -p hoimin-core --test machine \
  ordered_candidate_filter_marks_truncated_analysis_incomplete_but_replays -- --exact
cargo test -p hoimin-core --test machine
cargo test -p hoimin-core --features contracts
```

Expected: all commands pass with no warnings.

- [ ] **Step 6: Commit the state-machine fix**

```console
git add crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs
git commit -m "fix: reject empty ordered verification"
```

### Task 2: Align the Real CLI and Documentation

**Files:**
- Modify: `crates/hoimin-cli/tests/plan.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: the Task 1 rule that filtered truncated analysis continues replay with incomplete exit code 4.
- Produces: an end-to-end `verify --top` contract and documented truncated-plan semantics.

- [ ] **Step 1: Update the truncated-plan CLI expectation**

In `verify_top_executes_the_highest_ranked_retained_candidate`, change:

```rust
assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
```

to:

```rust
assert_eq!(code, 4, "stderr={}", String::from_utf8_lossy(&stderr));
```

After parsing `document`, add:

```rust
assert_eq!(document["summary"]["complete"], false);
```

Keep the marker, baseline, selection, mutant count, and candidate-ID
assertions unchanged so the test proves the retained candidate still runs.

- [ ] **Step 2: Run the focused CLI test on the Task 1 implementation**

The core RED tests in Task 1 establish the production failure before the
implementation. For this integration assertion, run on the Task 1
implementation:

```console
cargo test -p hoimin-cli --test plan \
  verify_top_executes_the_highest_ranked_retained_candidate -- --exact
```

Expected: PASS with exit 4, `summary.complete == false`, and one retained
candidate result.

- [ ] **Step 3: Document truncated verify results**

In the README paragraph beginning `plan discovers candidates`, replace the
sentences about truncated plans with:

```markdown
A manifest with `truncated` set to `true` contains only a partial candidate set, and `plan` exits 4; it cannot establish full coverage of the selected targets. On such a plan, `--top N` means the top N among retained candidates, not among candidates that discovery did not retain. `verify` still runs the retained selection, but its report remains incomplete and exits 4 because discovery was truncated.
```

Keep the following target/fingerprint integrity and session sentences.

- [ ] **Step 4: Run full verification**

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
git diff --check
```

Expected: every command passes with no warnings or formatting errors.

- [ ] **Step 5: Commit the CLI contract and documentation**

```console
git add crates/hoimin-cli/tests/plan.rs README.md
git commit -m "docs: explain truncated verify outcomes"
```
