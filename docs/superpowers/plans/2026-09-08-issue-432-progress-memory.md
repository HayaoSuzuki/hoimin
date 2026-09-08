# Issue #432 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound retained mutant memory independently of history length.
**Architecture:** A rolling adjacent pair feeds shared comparison logic; compact metadata feeds deferred rendering.
**Tech Stack:** Rust 1.98, MSRV 1.88, serde JSON, existing test allocator.
**Spec:** docs/superpowers/specs/2026-09-08-issue-432-progress-memory-design.md

## Global Constraints

- Preserve public progress types/functions and JSON schema 1.
- Preserve warning order, unusable-gap resets, and late-error atomicity.
- Rust MSRV remains 1.88; no new dependencies.
- Add no source comments unless required by safety/lint contracts.
- Only this Issue worktree is writable for tracked work; no unrelated fixes.

### Task 1: incremental progress processing with bounded heap regression

**Files:** `crates/hoimin-cli/src/progress/{mod,input,compare,render}.rs`; `crates/hoimin-cli/tests/progress.rs`; new `crates/hoimin-cli/tests/progress_heap.rs`; optionally extract allocator from `tests/report_heap.rs` into `tests/support/heap_tracking.rs`; record `docs/superpowers/reports/2026-09-08-issue-432-progress-memory.md`.
**Interfaces:** Preserve `compare_reports(&[InputReport], NonZeroUsize) -> ProgressResult`. Private compact input metadata stores path and optional unusable reason. Private accumulator owns ProgressResult, advances from a borrowed adjacent pair and returns/caches its CandidateSetEligibility only when usable. Renderer takes compact metadata and cached eligibility instead of decoded reports.

- [x] Before production changes, add a real allocation-tracked progress regression. Generate a valid 2,000-mutant report using the golden schema-v3 document, distinct candidate IDs/sequences and matching counts/summary sequence. Prepare paths before measuring and call `progress::run` with sink writers. One test per allocation-tracking binary avoids harness concurrency noise. Expected bound:

```rust
let short_peak = measure_history(&path, 2);
let long_peak = measure_history(&path, 16);
assert!(long_peak <= short_peak + 512 * 1024,
    "short={short_peak}, long={long_peak}");
```

- [x] Run `cargo test --offline -p hoimin-cli --test progress_heap -- --test-threads=1`; record old history-retention failure. If extracting existing allocator into support, prove existing `report_heap` still passes.
- [x] Add/retain public output regressions. For both json/human, valid + unusable + malformed-last must return exit2, empty stdout and no earlier warning. For valid mixed history, assert literal input dispositions, comparison indexes and warning ordering including final unusable input (no stale human score).
- [x] Extract one shared comparison accumulator from the current compare_reports loop. Preserve comparison math and eligibility exactly. Change public wrapper to call that same accumulator for each borrowed adjacent pair.
- [x] Implement the CLI's rolling path:

```text
accumulator = new(patience)
previous = None
for path in paths:
    current = read_report(path)?
    retain compact disposition(current)
    if previous exists: advance(previous, current), retain pair eligibility
    previous = current  // drops older report
release previous
render dispositions, comparisons, cached eligibility
```

No rendering occurs until every input succeeds. Keep the parser unchanged unless a compact-metadata type belongs in input.rs.
- [x] Run progress_heap, progress, report_heap, and lean_progress_decision_oracle. Run fmt and scoped all-target Clippy. Check final output behavior against literal fixtures and existing property/oracle tests.
- [x] Record RED/GREEN and heap values. Controller owns full workspace/MSRV, RSS experiment, final review and PR. Update plan checkboxes/report and commit task source, tests, spec, plan and report. Do not force-add scratch SDD artifacts; the tracked docs report carries the durable verification record.
