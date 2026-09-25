# Issue 609 Progress Keys Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline; the root agent performs independent review and publication. User authorized execution through PR without additional approval pauses.

**Goal:** Eliminate candidate text copies from progress comparison indexes and auxiliary sets.

**Architecture:** Borrow the existing five-field identity tuple and stable IDs from each report. Keep eligibility, counting and state transitions unchanged.

**Tech Stack:** Rust 2024, standard HashMap/HashSet, existing allocator integration-test support.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-609-progress-keys-design.md`

## Global Constraints

- Rust minimum remains 1.88; no dependencies added.
- Full tuple equality and matching-ID precedence must remain unchanged.
- One Cargo job, debug info disabled, dedicated batch-progress target.
- Root coordinates any Lean run: one process, 20 seconds, 2 GiB.

## Review Focus

- Duplicate content on one or both sides must count once and exclude all corresponding unique entries.
- Inconclusive content appearing on both sides must count once even with different IDs.
- Optional symbol None and Some("") are distinct.
- Equal text in distinct allocations must match; each of five content fields must independently distinguish keys.
- Stable IDs must still distinguish equal-content candidates; changed IDs must break stall chains and retain diagnostics.

### Task 1: Allocation regression and borrowed keys

**Files:** create `crates/hoimin-cli/tests/progress_compare_heap.rs`; modify `crates/hoimin-cli/src/progress/compare.rs`.

**Interfaces:** consumes public `compare_reports(&[InputReport], NonZeroUsize) -> ProgressResult`; public output unchanged.

- [ ] Build small and large pairs before measurement, keeping candidate count fixed. Measure unique, duplicate and inconclusive fallback plus matching IDs in one isolated allocator test. Assert literal counters and `large_peak <= small_peak + 64 * 1024`.
- [ ] Run `cargo test -p hoimin-cli --test progress_compare_heap -- --nocapture`; expect allocation-bound failure on owned keys and record peak values.
- [ ] Replace owned key fields with `&Utf8Path`, `&str`, `Option<&str>`, derive Copy and propagate report lifetimes. Replace key clones with copy/dereference and `.cloned()` with `.copied()`.
- [ ] Run the same command; expect all cases pass with bounded peak growth.

### Task 2: Semantic and public-path regressions

**Files:** modify `crates/hoimin-cli/tests/progress.rs`.

**Interfaces:** existing `usable`, `mutant_with_id`, `run_progress` helpers; no production API changes.

- [ ] Add table-driven fallback tests altering each identity field separately. Expect common=0, added=1, removed=1; a control with only ID/location changes expects common=1 and indeterminate. Check None versus empty symbol.
- [ ] Add duplicate/inconclusive unions with distinct IDs and literal full Comparison expectations, including scores.
- [ ] Add public run→progress test for both JSON and JSONL, with unchanged saved run outputs and a leading-comment source shift. Expect common candidates retained, all survived, indeterminate and changed-ID warning.
- [ ] Run progress integration tests and existing Lean progress oracle consumers; expect existing semantics preserved.

### Task 3: Review, workspace verification and commits

**Files:** create `docs/reviews/2026-09-25-issue-609-progress-keys.md`; update this checklist.

- [ ] Perform three implementation and three test self-reviews, each recording findings, changes and remaining limits.
- [ ] Run `cargo test --workspace`, `cargo fmt --all -- --check`, vendor fmt, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and parser clippy; expect success. Run Python CI formatting/lint and pytest where available and report environment failures accurately.
- [ ] Review diff and commit implementation/tests/evidence. Root handles independent review and publication.

## Plan self-reviews

1. Coverage: original heap test measured history, so it could not isolate body cloning. Added a separate comparison-only binary and explicit small/large acceptance bound.
2. Ordering and independence: code must follow a real allocation RED; semantic characterizations can pass before the ownership change. Setup and parsing stay outside measurement; one test prevents allocator cross-talk.
3. Integration: synthetic fixtures alone would miss reader/serializer interactions. Added public unedited JSON/JSONL run reports with changed source offsets; preserve existing history, warning and Lean oracle tests. No production interfaces shared between tasks beyond unchanged public compare_reports.
