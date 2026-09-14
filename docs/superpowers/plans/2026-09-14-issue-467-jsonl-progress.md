# JSONL progress implementation plan

Goal: compare completed current run JSONL alongside legacy/current JSON.
Spec: ../specs/2026-09-14-issue-467-jsonl-progress-design.md.

Execute inline in enhancement/issue-467 under user-authorized autonomy.

- [x] Add tests in crates/hoimin-cli/tests/progress.rs: serialize a valid
  report into run_started, baseline_finished, mutant_started,
  mutant_finished and run_finished lines; assert read_report is usable and
  mixed-format progress matches JSON comparisons. Confirm failure first.
- [x] Add input/jsonl.rs: BufRead line processing, core ReportSequence,
  baseline ordering and required terminal event. Change input.rs to dispatch
  from the first nonblank line and reuse document validation/classification.
- [x] Test missing end, truncated line, duplicate/mixed runs, order, duplicate
  mutant identity, result/count mismatch, current/unsupported schemas,
  blank/CRLF/no-final-newline, opaque config and failed/incomplete parity.
- [x] Add actual CLI producer parity and diagnostic-history heap checks.
- [x] Document format/lifecycle/memory contracts in README and existing OKF
  progress concept; register spec/report with revision and final hashes.
- [ ] Run relevant progress and Lean adapters plus fmt/clippy, record all
  three self-review passes per stage, commit/push and create template PR.

Builds use the worktree's dedicated target with CARGO_BUILD_JOBS=2,
CARGO_INCREMENTAL=0, CARGO_PROFILE_DEV_DEBUG=0 and CARGO_PROFILE_TEST_DEBUG=0.

Plan self-review: (1) test lifecycle data includes starts absent from JSON;
(2) test legacy JSON separately from unsupported v2 JSONL; (3) run real producer
and heap tests, not only synthetic happy-path decoding.

Implementation review found producer contract assertions in ReportSequence;
extract `validate(&self, &OutputEvent)` and call it before observe for untrusted
JSONL. Verify both default and contracts builds; producer assertions remain.
