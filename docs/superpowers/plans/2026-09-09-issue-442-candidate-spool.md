# Issue #442 candidate spool implementation plan

> **For agentic workers:** Use superpowers:subagent-driven-development for this bounded implementation, followed by independent task and final reviews.

**Goal:** Reduce filesystem writes while retaining the candidate spool contract.
**Architecture:** Count/validate, encode one bounded JSONL record, and write it as one complete buffer.
**Tech Stack:** Rust, serde_json, tempfile, existing candidate-store and analyzer tests.
**Spec:** `docs/superpowers/specs/2026-09-09-issue-442-candidate-spool-design.md`

## Global Constraints

MSRV 1.88. No dependency/schema/replay API changes. Source comments only when necessary. Preserve max-candidates, sequence, 2 MiB record limit including newline, and finish flush/sync/keep. No persistent buffering or retry of failed push.

### Task 1: Coalesce bounded candidate record writes

**Files:** `crates/hoimin-cli/src/analyzer/store.rs` and, if necessary, existing `crates/hoimin-cli/tests/analyzer_handler.rs`; tracked report `docs/superpowers/reports/2026-09-09-issue-442-candidate-spool.md`.
**Interfaces:** CandidateStore public API and CandidateSpoolRef/CandidateCursor are unchanged. Any helper stays private. Controller owns release benchmark, broad validation, reviews, and PR.

- [x] Run baseline store unit tests and analyzer_handler spool tests. Preserve before-source benchmark evidence from the issue; this is a performance defect, so compatibility tests can legitimately pass on old code.
- [x] Add tests for complete JSONL contents and a file write error before count advancement. Construct a read-only file handle from a temporary path and `NamedTempFile::from_parts`; assert StoreError::Io, unchanged counts and no successful record, then drop and verify temporary-file cleanup. Retain exact size/Unicode/sequence tests.
- [x] Establish RED for write amplification using a focused Writer observation at a private production helper if justified: one accepted small record should arrive as one complete buffer, with valid Unicode/escapes/newline and no writes for invalid size. Alternatively retain the measured baseline as performance RED without adding implementation-mirroring assertions.
- [x] After existing count/size checks, build the bounded record:

```rust
let mut record = Vec::with_capacity(record_size);
serde_json::to_writer(&mut record, candidate)
    .map_err(|error| StoreError::CorruptRecord(error.to_string()))?;
record.push(b'\n');
self.file.as_file_mut().write_all(&record)
    .map_err(|error| io_error(&error))?;
```

Derive `record_size` from the accepted count plus newline with checked/constrained conversion; source size is already below 2 MiB. Keep both counters updated only after the write succeeds. Avoid a persistent buffer, new schema fields, or generalized file abstraction.
- [x] Run focused store tests, complete analyzer_handler integration tests and bounded-discovery oracle tests. Run fmt/scoped Clippy. Record commands/results, performance RED, and self-review in the tracked report and scratch report; commit code/tests/design/plan/report. No push/PR from implementer.
- [ ] Controller compares baseline and final source release performance and exact bytes, runs workspace all-features tests/MSRV/full Clippy, task and final reviews, updates the tracked report, and creates a PR closing #442.

## Plan self-review 1 — defect versus compatibility

Behavioral tests can pass before a performance fix. Use measured write amplification as RED rather than inventing a fragile timing test. Existing byte boundaries and new I/O failure tests protect compatibility and failure behavior.

## Plan self-review 2 — error assertions

Chmod alone can succeed under privileges. A read-only handle makes the failure portable and deterministic. Test count and cleanup through the real store; short-write helper testing is optional only if a real production separation needs it.

## Plan self-review 3 — scope and completion

One task owns the spool code and tests; controller owns comparison and broad gates. Record buffer capacity is derived from the already bounded size, and finish remains untouched. The benchmark must include sync and exclude fixture generation/byte comparison. No unrelated replay or persistence redesign is required.
