# Issue 610 Duplicate Progress Inputs Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline. Execution through final commits is authorized; root coordinates independent review, PR, CI and merge.

**Goal:** Warn about repeated report artifacts without changing progress decisions.

**Architecture:** Fingerprint bytes during validated reads, retain compact evidence, and confirm digest candidates by bounded byte comparison. Render warnings after existing diagnostic groups.

**Tech Stack:** Rust 2024, existing BLAKE3 dependency, std buffered I/O and maps.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-610-duplicate-progress-inputs-design.md`

## Global Constraints

- No strict option, exit-code/schema change, deduplication or cross-format normalization.
- Preserve unusable barriers, self-comparison, scores and stall count.
- Keep JSONL and history memory bounds; no report/file-body retention.
- One Cargo job in batch-progress target; no Lean process without root coordination (20 seconds, 2 GiB, global concurrency one).

## Review Focus

- Malformed later inputs must not emit earlier duplicate warnings.
- Same run ID can describe different resumed reports; compare whole bytes.
- Different fingerprints from independent runs must not warn merely for equal mutants.
- Digest collisions and differences after a buffer boundary must not claim equality.
- Optional duplicate confirmation must not reopen nonregular files or add read errors to successful parsing.

### Task 1: Add behavioral regressions

**Files:** `crates/hoimin-cli/tests/progress.rs`, `crates/hoimin-cli/tests/progress_heap.rs`.
**Interfaces:** existing public CLI helpers and `progress::run`; output remains schema 1.

- [ ] Test four repeated paths and copied bytes, both human/JSON, requiring one warning per repeated position plus unchanged saturated/3 stalls.
- [ ] Test independent run IDs with equal results and same-ID reports with changed metadata/result/completion, expecting no duplicate warning.
- [ ] Test repeated reports around an unusable barrier, and a malformed final input with empty stdout and no duplicate warning.
- [ ] Run `cargo test --offline --locked -p hoimin-cli --test progress duplicate_input`; expect missing-warning assertions to fail before implementation.

### Task 2: Implement bounded evidence and warnings

**Files:** `progress/input.rs`, new `progress/duplicate.rs`, `progress/mod.rs`, `progress/render.rs`, README.
**Interfaces:** public read_report unchanged; internal read_report_with_fingerprint returns `(InputReport, blake3::Hash)`; duplicate evidence stores earlier index/reason, not report values.

- [ ] Extract buffered-reader parser and implement Read wrapper updating BLAKE3 only for returned bytes.
- [ ] Add tracker with first-path positions and digest buckets; exact regular-file comparison uses fixed buffers and errors suppress optional byte evidence.
- [ ] Before tracker implementation, add unit tests forcing a digest bucket collision and differences after a buffer boundary. Expect absent implementation/behavior failure.
- [ ] Store optional evidence in InputDisposition and render one-based current/earlier paths after existing diagnostics. Keep report comparison flow unchanged.
- [ ] Document byte-only scope, stderr warnings, unchanged saturation and resume behavior.
- [ ] Run regressions; update old copy-based fixtures' diagnostic expectations and history heap stderr assertion to intentional warning contract.

### Task 3: Review and verify

**Files:** new `docs/reviews/2026-09-25-issue-610-duplicate-progress-inputs.md`, this checklist.

- [ ] Record three separate implementation and test self-reviews; root provides independent review.
- [ ] Run progress, all existing heap gates, Lean progress oracle consumers, then full workspace; require exit 0 and record ignored cases.
- [ ] Run workspace/vendor fmt and workspace all-target/all-feature plus vendor parser clippy with warnings denied.
- [ ] Commit final code/tests/evidence; root publishes the two-branch progress stack.

## Plan self-reviews

1. Behavioral coverage: original fixture reports share IDs and bytes. Distinguish true independent-run controls from deliberate copied artifacts and preserve existing exact warning-order tests.
2. Negative and resource controls: add forced digest collision and late-buffer difference tests, use fixed buffers, and retain existing history heap bound rather than inflating it to accept a regression.
3. Error and implementation scope: shared reader extraction must preserve v2/v3/JSONL validation and source errors. Test all-or-error diagnostics and keep public read_report free of hashing. No new Lean semantics or Python changes.
