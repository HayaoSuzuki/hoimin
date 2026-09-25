# Issue 625 Progress Dispatch Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline. Root coordinates independent review and publication; execution and commits are authorized without additional approval pauses.

**Goal:** Avoid scanning compact report candidate bodies just to select format and schema.

**Architecture:** One serde prefix visitor supplies speculative dispatch; existing full typed parsing and validation remain authoritative.

**Tech Stack:** Rust 2024, existing serde/serde_json, release public-API timing probe.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-625-progress-dispatch-design.md`

## Global Constraints

- Preserve v2/v3, JSON/JSONL, arbitrary field order, opaque config and malformed-input rejection.
- No new dependency, schema or comparison-state changes.
- Use root target/batch-progress, one Cargo job, debug0 and no incremental builds.
- No Lean command without root's global slot (one process, 20 seconds, 2 GiB).

## Review Focus

- A valid prefix followed by malformed or extra data must still fail full parsing.
- Escaped discriminator keys and nested lookalikes must use JSON semantics.
- Version last and candidate body first must remain accepted.
- Pretty JSON and reordered JSONL first lines must select the correct parser.
- Early visitor termination must not be mistaken for successful validation.

### Task 1: Baseline and deterministic regression

**Files:** new `crates/hoimin-cli/src/progress/input/dispatch.rs`; external benchmark artifacts in `/tmp/hoimin-batch-604-632/625-benchmark`.
**Interfaces:** public `progress::read_report` unchanged; private probe returns optional format/schema evidence.

- [ ] Build unchanged release, generate public run JSON/JSONL report, and preserve a linked baseline read_report executable plus same input bytes.
- [ ] Write probe tests with a counting Read and 256-KiB body. First implement the full-map baseline behavior; assert less than 128 consumed bytes and observe actual body-size failure.
- [ ] Add parser integration cases before optimizing: field order, schema eras, opaque config, malformed suffixes and duplicate fields.

### Task 2: Prefix dispatch

**Files:** `progress/input.rs`, `progress/input/jsonl.rs`, new `progress/input/dispatch.rs`, `tests/progress.rs`.
**Interfaces:** `probe` distinguishes document/event and u32 schema; read_report still returns InputReport/ProgressError.

- [ ] Stop the probe at sufficient top-level evidence using captured result and explicit serde stop error; never accept a report on probe success alone.
- [ ] Replace full Kind/ReportHeader probes with combined evidence while retaining first-line and document fallback behavior.
- [ ] Run deterministic gate and progress/heap/Lean-consumer regressions; expect pass with fixed prefix reads and unchanged semantic acceptance.
- [ ] Rebuild release, link the same public timing probe, assert identical serialized mutant output and measure three rounds of 31 post-warmup reads per implementation.

### Task 3: Reviews and verification

**Files:** `docs/reviews/2026-09-25-issue-625-progress-dispatch.md`, this plan.

- [ ] Record three implementation and test self-reviews with findings; obtain independent reviewer via root/team.
- [ ] Run full workspace, exact CI workspace/parser clippy, workspace/vendor fmt and diff check.
- [ ] Record release timings, sample protocol, semantic equality, limitations and final commit. Do not push or merge.

## Plan self-reviews

1. Regression strength: wall-clock tests alone are noisy. Pair public before/after measurement with deterministic prefix-read counts and a baseline that still traverses the whole body.
2. Semantic coverage: existing readers test schemas but not all key placements. Add reordered header/documents and deceptive nested keys; final parser must catch errors after the stop point.
3. Ordering and reproducibility: preserve baseline executable before optimizing and use one unchanged run artifact for both versions. Keep memory gates unchanged and run existing formal-oracle consumers without generating a new corpus.
