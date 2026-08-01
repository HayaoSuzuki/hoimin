# Ordered Candidate Mid-Collection Cancellation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans, plus superpowers:test-driven-development.

**Goal:** Finish an interrupted ordered-candidate run cleanly when cancellation or deadline arrives before candidate collection reaches EOF, without resuming spool reads or raising a false `SelectedCandidateMissing` error.

**Architecture:** Keep interruption handling at the existing `RunState::begin_stopped_mutant_drain` boundary. When ordered state exists, move every already-materialized selected candidate from both `ready` and `discovered` into the stopped queue in requested-ID order, mark the candidate stream exhausted, and discard unavailable requested IDs whose candidate metadata was never read. Existing retired-effect handling remains responsible for late completions from the cancelled read.

**Tech Stack:** Rust 1.88, pure `hoimin-core` state machine tests, optional contracts feature.

## Constraints

- Preserve ordered requested-ID order and stable deduplication.
- Emit one synthetic `MutantStarted`/`MutantFinished(NotRun)` pair for every selected candidate whose full `MutationCandidate` was discovered before interruption.
- Do not emit an event for a requested ID not yet discovered; no candidate metadata exists from which to build a valid public event.
- Do not validate missing selected IDs after cancellation/deadline. Selection completeness is an EOF invariant, and EOF was intentionally not reached.
- Do not resume candidate spool reads after interruption.
- Preserve `SelectedCandidateMissing` for uninterrupted ordered and explicit-candidate EOF validation.
- Preserve existing `RetiredEffect` behavior for a late `CandidateLoaded` completion whose read was retired by the stop event.
- Preserve current behavior for unordered runs and ordered runs whose collection already completed.
- Do not change public events, report schema versions, or CLI code.

## Files

- Modify `crates/hoimin-core/src/machine.rs`
- Modify `crates/hoimin-core/tests/machine.rs`

## Task 1: Reproduce cancellation during ordered collection

- [ ] Add `ordered_candidate_cancellation_mid_collection_drains_discovered_selection_without_resuming_reads`.
- [ ] Start an ordered run requesting `[third, first, fourth]` with multiple workers available.
- [ ] Feed `first`, unselected `second`, and `third`, but not EOF. Leave the next `ReadCandidate` in flight.
- [ ] Send `CancellationRequested` and assert:
  - no `ReadCandidate` is returned;
  - stopped output begins with `third`, then `first`;
  - `fourth` is not emitted because it was not discovered;
  - the machine reaches one incomplete `RunFinished`, exit 130, with exactly two `not_run` results;
  - no `SelectedCandidateMissing` occurs.
- [ ] Submit the retired in-flight `CandidateLoaded` completion and assert existing retired-completion semantics without state corruption.

Use existing helpers such as `waiting_for_ordered_analysis`, `find_effect`, and `complete_mutant_started`. Add only a focused driver helper if needed.

- [ ] Run RED:

```bash
cargo test -p hoimin-core --test machine ordered_candidate_cancellation_mid_collection_drains_discovered_selection_without_resuming_reads -- --exact --nocapture
```

Expected RED: cancellation returns a new `ReadCandidate`; continuing the old path eventually reaches a false missing-selection error.

## Task 2: Drain incomplete ordered state at the stop boundary

- [ ] In `begin_stopped_mutant_drain`, handle every `ordered_candidates` value, not only completed collection.
- [ ] Iterate `ordered.ids` and take each candidate from `ordered.ready` or `ordered.discovered`.
- [ ] Avoid duplicates if an ID appears in both stores.
- [ ] Append materialized selections as `StoppedCandidate::NotStarted`.
- [ ] Clear both ordered stores and set `candidate_exhausted = true` before `next_stopped_candidate_effects`.
- [ ] Keep the existing final requested-order sort across worker and stored candidates.
- [ ] Do not call `ensure_selected_candidates_discovered` on the stop path.

Prefer a private `drain_ordered_candidates_for_stop` helper if it makes ownership and deduplication explicit. Keep normal `CandidateLoaded` EOF validation unchanged.

- [ ] Run GREEN:

```bash
cargo test -p hoimin-core --test machine ordered_candidate_cancellation_mid_collection_drains_discovered_selection_without_resuming_reads -- --exact --nocapture
cargo test -p hoimin-core --test machine ordered_candidate_cancellation_drains_remaining_candidates_in_requested_order -- --exact
```

## Task 3: Cover deadline and missing-ID preservation

- [ ] Prove both `CancellationRequested` and `DeadlineReached` stop incomplete ordered collection without another read.
- [ ] Assert cancellation retains exit 130 and deadline retains existing incomplete timeout policy.
- [ ] Preserve tests proving uninterrupted ordered EOF returns `SelectedCandidateMissing` for an absent requested ID.
- [ ] Preserve explicit-candidate partial-match missing validation.
- [ ] Use jobs greater than one to prove discovered ordered candidates are not orphaned when workers reset.

Run:

```bash
cargo test -p hoimin-core --test machine ordered_candidate -- --nocapture
cargo test -p hoimin-core --test machine explicit_candidate_filter_rejects_missing -- --nocapture
```

## Task 4: Verification and review

- [ ] Run fresh gates:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
git diff --check
```

- [ ] Confirm the production diff is confined to ordered stop draining in `machine.rs`.
- [ ] Confirm no public type/schema changes.
- [ ] Request independent review focused on undiscovered-ID policy, requested order, no post-stop reads, cancellation/deadline policy, missing-ID preservation, and retired completions.
- [ ] Commit implementation after review fixes.

## Acceptance Criteria

- Mid-collection cancellation and deadline never resume spool reads.
- Discovered selected candidates are reported exactly once as `NotRun`, in requested order.
- Undiscovered requested IDs cause no event and no missing-selection error on the interrupted path.
- The run reaches a single incomplete `RunFinished`; cancellation exits 130.
- Uninterrupted missing selections still produce `SelectedCandidateMissing`.
- Full repository gates and independent review pass.
