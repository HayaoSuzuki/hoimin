# Issue 463 implementation plan and evidence

Spec: ../specs/2026-09-14-issue-463-json-record-write-design.md

## Plan

- [x] Add a counting-spool RED showing fragmented serialization and short-write/write-zero correctness tests. The RED observed 18,599 spool writes for 100 records instead of 100.
- [x] Serialize each complete mutant record locally and issue one synchronous `write_all`.
- [x] Run report, heap, release, Clippy and workspace verification.
- [x] Record three reviews per stage; commit, push and PR publication follow this evidence update.

## Plan self-review

1. The RED observes the real `ReportHandler::with_mutant_spool` path and exact bytes, rather than benchmarking a duplicate serializer.
2. Failure tests distinguish legal short writes from exhausted or erroring writers and require poisoning after the failed effect.
3. Final report framing, disk location, flush, seek, copy and shutdown remain exercised by existing report integration tests.

## OKF self-review

1. The concept limits its claim to JSON mutant records and does not generalize it to JSON Lines or human output.
2. It separates immediate write acknowledgment from persistence guarantees; `flush` is not described as `fsync`.
3. Final checks cover YAML, footnotes, links and index reachability without assigning unperformed human verification.

## Implementation self-review

1. Acknowledgment: `record` sets `Poisoned` before calling `write_mutant` and returns success only after its synchronous `write_all`; there is no persistent writer buffer.
2. State: the local bytes include the separator, and `has_mutants` changes after the full write. A partial failure cannot lead to a retry with ambiguous comma state because lifecycle remains poisoned.
3. Lifecycle: the spool object, managed directory, final flush/seek/copy and `flush_and_release_spool` code are unchanged; only the number of calls at the write boundary changes.

## Test self-review

1. Performance: the real handler RED counted 18,599 calls for 100 events; GREEN requires exactly 100 and parses all 100 final records.
2. Writer semantics: a seven-byte writer requires repeated legal short writes and yields valid JSON; a zero-progress writer maps to the original effect's `ReportIo` before acknowledgment and poisons the handler.
3. Existing failure and resource tests: `partial_mutant_spool_failure_poisons_json_report` still crosses a successful short prefix before error, while `report_heap` checks that 10,000 events do not accumulate in memory.

## PR self-review

1. Scope: the diff contains the JSON record write boundary, its direct tests and the issue-specific design/plan/OKF evidence; it does not change JSON Lines or human output.
2. Claims: the PR describes one `write_all` invocation per record, while the short-write test makes clear that `write_all` may internally call a conforming writer more than once.
3. Evidence: the release-profile real-handler test passed, the full workspace/all-features suite passed, scoped Clippy is warning-free and the 17-page OKF catalog validates.

## Release and final evidence

- `cargo test -j 2 --release -p hoimin-cli --test report_handler json_spool_writes_one_complete_record_per_mutant -- --exact --nocapture`: passed; the real handler completed 100 records with exactly 100 spool `write` calls.
- `cargo test -j 2 --workspace --all-features --quiet`: passed, including 24 report-handler tests and the 10,000-record heap regression.
- `cargo clippy -j 2 -p hoimin-cli --tests --all-features -- -D warnings`: passed.
- `cargo fmt --check`, `git diff --check` and `/private/tmp/hoimin-okf-check.py .`: passed; the latter validated 17 pages.
- The first focused documentation run could not locate the worktree-local `.venv/bin/python`; untracked links to the repository environment fixed the test harness prerequisite without entering the diff.
