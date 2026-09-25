# Issue 613 buffered diagnostic implementation plan

> **For agentic workers:** Use superpowers:executing-plans to execute inline.

**Goal:** Reduce stderr writes for escaped JSON diagnostics without changing event bytes or delivery/error semantics.

**Architecture:** An event-local 8-KiB BufWriter in the JSONL serializer, invoked only for JSON/JSONL stderr diagnostics, flushed explicitly and dismantled without Drop retries.

**Tech Stack:** Rust std::io, serde_json, existing ReportHandler and report-delivery tests.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-613-buffer-diagnostics-design.md`.

## Constraints and review focus

Preserve schema, stdout/human routes, newline/event flush and effect-ID error mapping. Watch errors after partial buffered writes, interrupted/zero writes, repeated diagnostics with no cross-event buffering, exact control-character serialization, and large-event memory growth. No new dependencies or report state machine.

## Task 1: measured RED and error contracts

- [ ] Add counted-writer regression in `crates/hoimin-cli/tests/report_handler.rs` for both JSON formats and plain/newline/quote/backslash/control-heavy payloads. Assert exact serde bytes, parsed message, one flush per acknowledged event, and at most payload-bytes/4KiB plus small constant underlying writes. Run and record the unbuffered write-count RED.
- [ ] Add fault writers for partial-progress then error, write-zero, Interrupted-then-success, and flush failure; assert error kind/effect ID and no writes after the first non-interrupted error, including handler destruction.
- [ ] Build unchanged release and retain a public run_with_io probe for same baseline workloads before changing production, if available within the existing cache.

## Task 2: fixed buffering and bounded-memory evidence

- [ ] Implement `write_buffered_event(writer, event)` using `BufWriter::with_capacity(8 * 1024, writer)`, existing `write_event`, and unconditional `into_parts`. Route only diagnostic JSON/JSONL stderr through it.
- [ ] Run focused report tests GREEN. Add a single-test heap binary measuring serialization after constructing/moving payloads outside the measured interval; compare 16-KiB and 1-MiB diagnostic messages with a fixed overhead limit.
- [ ] Demonstrate allocation-test sensitivity using a temporary whole-event serialization Vec, remove it, and rerun GREEN. Confirm large escaped messages retain exact output and constant extra allocation.
- [ ] Rebuild release and repeat the same public baseline probe; record underlying Write calls, output reconstruction, status3 and timings, without claiming syscall counts.

## Task 3: review and final checks

- [ ] Three implementation reviews: precise buffering scope; error/acknowledgement/drop flow; memory/lifetime/event order. Three test reviews: meaningful RED; writer failure sensitivity; public baseline and format parity/measurement limitations. Record concrete findings.
- [ ] Independent read-only review, then full workspace, exact workspace all-feature/all-target clippy, locked vendor parser clippy, both fmt and diff checks. Existing relevant Lean consumers only; no Lean command needed.
- [ ] Commit code/tests/evidence, rebase onto latest main if necessary, rerun integration checks, and publish an independent gh stack PR with Closes613. Root owns CI/merge.

## Plan review passes

1. Acceptance mapping: added byte-for-byte and parsed-message assertions so a write-count reduction cannot silently truncate escapes or omit event newline/flush.
2. Failure sensitivity: added a writer that becomes writable after its first error, making destructor retries observable; always-failing writers alone would miss unintended second attempts.
3. Resource/measurement: measure only output serialization allocations with payload construction excluded; retain original/optimized public release probes and distinguish Write call counts from OS syscall tracing. Preserve no-Lean-process scope because no decision contract changes.
