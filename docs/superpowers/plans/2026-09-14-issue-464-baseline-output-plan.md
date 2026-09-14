# Baseline output implementation plan

> Execute inline with the executing-plans workflow; user authorizes autonomous completion and requires three self-review passes per stage.

**Goal:** Make retained failed-baseline logs visible and saveable before cleanup.
**Architecture:** Stream existing process spool into existing diagnostic delivery before BaselineFinished output.
**Tech Stack:** Rust, Tokio file I/O, ReportDelivery; no new dependencies.
**Spec:** `docs/superpowers/specs/2026-09-14-issue-464-baseline-output-design.md`

## Global constraints

Keep core/report schemas, exit policy, cleanup/disk bounds. Maximum log data equals retained --max-output bytes. Fixed16KiB reads plus UTF8 carry. Diagnostics on stderr. No output for successful baseline. Existing scheduler deadline remains outer bound.

## Task 1: Capture export

Files: new `crates/hoimin-cli/src/baseline_output.rs`, `src/lib.rs`, `src/shell.rs`, new `tests/baseline_output.rs`.

- [x] Add real CLI failure regression for human/json/jsonl, two marker lines and clean cleanup; run and observe missing logs.
- [x] Implement `emit` taking ProcessHandler, ReportDelivery and BaselineFinished. Read16KiB at a time using tokio::fs with retained-byte limit; decode complete UTF8 prefixes and preserve trailing incomplete bytes; encode invalid bytes as replacement characters.
- [x] Emit baseline.output records with run/token/offset/retained/observed/truncated details; escape control characters and keep newline/tab. On read errors emit baseline.output.read, on diagnostic write failure stop without changing result.
- [x] Call helper for termination != Exit(0) in the BaselineFinished EmitOutput branch before existing report delivery.
- [x] Add timeout/verify, max-output, invalid bytes, split Unicode and success control cases. Bound child processes by timeout/kill-on-drop.
- [x] Run focused regressions, complete run_e2e and relevant shell delivery timeout tests; fmt and Clippy.

## Task 2: Documentation and PR

- [x] Document stderr viewing/saving, combined streams, truncation, encoding, interrupted export limitation.
- [x] Update session/report OKF concept and source indexes with final hashes; validate YAML, links and claims separately.
- [x] Record three actual reviews for each stage; incorporate independent review before publication.
- [ ] Commit/push dedicated branch and create PR using template; inspect published head and CI separately.
