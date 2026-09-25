# Issue 610 review and verification record

Design/plan commit `29ff92a` preceded code and test changes. The spec and plan each record three self-review passes. This branch builds on issue 609 (`ddc66c0`) and changes only duplicate-input diagnostics, keeping all comparison semantics.

## Implementation self-reviews

1. **Identity and collision safety.** Reviewed path identity separately from byte identity. Matching run IDs and candidate results are never evidence. BLAKE3 buckets only find candidates; every different-path warning requires full byte equality, including EOF/length. Distinct candidates remain in a collision bucket, and a later matching copy can select the second candidate. Same-path evidence deliberately identifies artifact reuse rather than claiming immutable contents.
2. **I/O and memory ownership.** Traced the same underlying read through fingerprint wrapper and both JSON/v2 and JSONL parsers. Hashing includes blank lines and all bytes returned, not buffer capacity; public read_report bypasses hashing. The tracker stores paths, hashes and indices only. Exact comparison uses two BufReaders and consumes the smaller available slice, so partial reads cannot misalign comparison. After independent review, Unix confirmation opens nonblocking and checks descriptor metadata before reading; optional confirmation errors suppress only that diagnostic. No extra file bodies, JSONL event histories or owned mutant copies are retained.
3. **Output and lifecycle compatibility.** Input disposition adds nonserialized evidence; comparisons still run for every adjacent pair, including repeats. Unusable inputs remain barriers. Duplicate warnings are appended after all existing warning groups, and render still starts only after all reports validate. Parser errors preserve their original source mapping. No state model, corpus, schema or exit-code contract changed. Existing Lean consumers remain the policy check; no Lean process was launched.

## Test self-reviews

1. **Behavioral RED and controls.** Before implementation, repeated-path/copy and barrier tests failed with zero warnings versus three; independent/resumed/mixed-format controls passed. New tests exercise both source encodings and both output formats. Review strengthened warning assertions to include actual current/earlier paths, not just positions. The existing saturation output test now creates four different run IDs with identical candidates/results and requires no warnings.
2. **Equality and stream boundaries.** Unit tests force the same digest for different content and then a copy of the second candidate, check a differing byte beyond 32 KiB and differing lengths, and exercise missing confirmation files and nonregular directories. The fingerprint test fills its buffer with sentinel bytes and makes a partial final read, catching accidental hashing of unused buffer bytes. Initial unit RED was missing-implementation compilation; the feature's meaningful behavioral RED is recorded separately, not conflated with that compile failure.
3. **Warnings, barriers and allocation gates.** Review added a repeated unusable report as well as repeats on both sides of the barrier, retaining two comparisons and one final stall. A malformed trailing input must emit no earlier warnings or stdout. Existing exact warning-order expectations include the appended copy warning. The history heap fixture intentionally repeats one file, so only its empty-stderr assertion changed; the heap allowance is unchanged. JSONL-history and comparison-text heap gates remain active.

## Verification

- Behavioral RED: `cargo test --offline --locked -p hoimin-cli --test progress duplicate_input`: 1 passed, 2 expected failures (missing warnings).
- Initial focused GREEN: progress 78, both Lean progress consumers 7, and three existing progress heap tests passed.
- Initial duplicate unit suite: 3 passed; added partial-read unit included in final workspace run.
- Final workspace after the FIFO fix: `cargo test --offline --locked --workspace` exit 0; 2,299 passed and 22 ignored across 96 result groups (including subprocess test groups). Includes progress, all heap gates, existing Lean consumers and five duplicate unit tests.
- Final focused rerun after strengthening resumed-result and repeated-barrier coverage: `cargo test --offline --locked -p hoimin-cli --test progress duplicate_input` exit 0, 3 passed.
- FIFO regression RED: the blocking opener failed with `Err(Timeout)` versus `Ok(true)` after one second; rescue opened the FIFO to release the worker before assertion failure. GREEN: all five duplicate unit tests passed in 0.00 seconds.
- Final `cargo clippy --offline --locked --workspace --all-targets --all-features -- -D warnings`: exit 0.
- Final `cargo clippy --offline --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`: exit 0.
- Workspace and vendor parser `cargo fmt ... -- --check`, plus `git diff --check`: exit 0.
- Logs: `/tmp/hoimin-batch-604-632/610-{red,fifo-red,unit-final,focused-final,workspace-final,clippy-final,parser-clippy}.log` in the implementation environment.
- No Python code changed; shared Python tooling remains untouched. No Lean process ran.

## Independent review and correction

The issue-605 reviewer found a regular-file-to-FIFO race between path metadata and a blocking confirmation open. This was a blocker despite confirmation being best-effort. Added an opener regression using a FIFO with no writer; it failed in 1 second with a blocking opener and releases that opener for clean test shutdown. Corrected Unix confirmation to use O_NONBLOCK and inspect descriptor metadata, eliminating the path-check/open race. Nonregular descriptors and open errors suppress only optional byte evidence. The remaining review found bounded buffers, full parser bytes, deferred warnings, barriers and schema preservation sound.

The issue-605 reviewer re-reviewed the nonblocking opener and the deadline/rescue regression, confirmed the blocker resolved, and reported no remaining findings. Final workspace verification includes the fix.
