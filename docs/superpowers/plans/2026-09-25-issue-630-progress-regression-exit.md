# Issue 630 regression exit implementation plan

> **For agentic workers:** Use superpowers:executing-plans to execute inline.

**Goal:** Return exit1 after successful progress output only for an explicitly selected latest regression.

**Architecture:** Carry one boolean CLI option to progress::run, use the existing latest state after rendering, and preserve error2 propagation. Extend the existing Lean generator/corpus rather than adding an oracle registry target.

**Tech Stack:** Rust/clap, existing public CLI tests, Lean progress decision model/corpus.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-630-progress-regression-exit-design.md`.

## Constraints and review focus

Default code0 and all output bytes/schemas remain unchanged. Check past regression followed by improvement/unusable, candidate-set mismatch with nonzero regression count, trailing malformed input, failed stdout/stderr, and optional details across both output formats. New proofs use10000heartbeats; Lean commands use20sec/2GiB guard with root's exclusive slot.

## Task 1: public RED and implementation

- [x] Add public CLI regression-gate tests in `tests/progress.rs` for all latest states, recovery/barrier histories, set mismatch, human/JSON/details parity and malformed trailing input. Observe unknown-option RED before code.
- [x] Extend the real strong/weak run fixture to verify opt-in exit1 and unchanged candidate/report results.
- [x] Add `fail_on_regression: bool` to raw/public ProgressArgs and parser conversion; default false. Destructure it in progress::run and return `i32::from(fail_on_regression && result.latest == ProgressState::Regressing)` after render success. Update direct argument constructors with false.
- [x] Add failing stdout and stderr writer tests through run_with_io, preserving error2 even with a latest regression. Run focused tests GREEN.

## Task 2: Lean correspondence and documentation

- [x] Add pure progress exit function and default/only-regression/error precedence lemmas, plus broken historical-regression witnesses. First use an intentionally missing always0 model and observe RED, then implement the defined gate.
- [x] Add regression-then-recovery and regression-then-unusable cases; add ordinary/opt-in exit expectations to generated corpus schema3. Extend existing typed Rust adapter to record actual subprocess exit and classify code mismatches separately from infrastructure failures.
- [x] Request global Lean slot; run model/native/generator/freshness/sensitivity under20sec/2GiB with no domain increase. Run public CLI consumers, retaining all aggregate/detail assertions.
- [x] Update progress README/help: opt-in1 for latest regression, indeterminate0, errors2, previous jq policy still usable. No JSON schema edits.

## Task 3: review and verification

- [x] Three implementation self-reviews (latest/error precedence; output/schema preservation; parser/API/defaults) and three test self-reviews (meaningful RED; all state/fault branches; model correspondence boundaries). Record findings.
- [x] Independent read-only review; full workspace and exact CI clippy2/fmt2/diff checks. Use batch-progress cache with debug0/incremental0/jobs1.
- [x] Commit implementation/evidence, rebase on latest main if needed, and complete focused integration recheck.

Publication uses an independent gh stack PR with Closes630; root handles CI/merge.

## Plan review passes

1. Acceptance coverage: add set mismatch with fallback regression so the flag cannot silently gate on counts rather than latest state; add regression→unusable to distinguish latest from last comparison.
2. Fault sensitivity: use real failing writers for both streams and an actual malformed trailing file; output snapshots alone would not establish error precedence or continued validation.
3. Oracle/resource review: reuse the existing generated cases with an explicit schema bump, preserve existing aggregate/details expectations, and expose actual process status rather than treating every nonzero exit as an infrastructure failure. Keep the finite domain and resource caps unchanged.
