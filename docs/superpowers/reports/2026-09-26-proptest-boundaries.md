# Proptest boundary coverage: review and evidence

Base: main 5f2ae319c99eafac1f6ccd8b86ed9ed08001ebaf. Design and plan with three reviews each were committed as 2ff26c9 before test changes. Both crates already declare proptest and Cargo.lock resolves 1.11.0. No dependency or runtime contract changes are included.

## Implementation self-reviews

1. Expected values: Latin-1 expected text/boundaries come from independently rendered scalars; UTF-8 uses standard string boundaries. Git row expectations materialize newline-normalized text rather than calling production line-index helpers. Streaming observations use complete input slices and one-shot BLAKE3, while invoking real stream/hash operations.
2. Generation and termination: constructive generators have no rejection filters; coordinate sources and selection masks are bounded. Stream data includes small arbitrary bytes and generated repeating patterns at 65535/65536/65537/131071/131072/131073 bytes. Readers have independent positive fragment sizes, at most one Interrupted before progress, and optional terminal errors. Equal, modified, truncated and independent right streams are deliberately generated. Shrinking stops after at most 2048 iterations; ordinary seeds and failure persistence remain at proptest defaults.
3. Scope and error contracts: only cfg(test) module registrations enter existing source files. New readers and oracles are test-only. Complete byte counts ensure that mismatches do not short-circuit accounting. Consumer errors stop further reads; terminal read errors retain operation, path and message. No new Lean proof, OS race coverage or native Windows observation is claimed.

## Test self-reviews

1. Coverage: existing fixed examples and Lean adapters remain. The new eight properties exercise generated combinations and every scalar boundary, including EOF/out-of-bounds/split UTF-8 scalars, normalized disjoint Git selections, short reads, buffer crossings and error propagation. Cookie recognition and malformed UTF-8 retain their existing dedicated tests; generated Unicode is not described as arbitrary invalid byte coverage.
2. Sensitivity: six temporary production defects were tested individually with PROPTEST_CASES=64 and PROPTEST_RNG_SEED=20260926. Each failed its named new property with proptest shrinking output. Defects were restored in finally blocks, and artificial failure persistence was disabled for these runs only. Initial Rust test-code compiler errors (temporary Cow borrows and braces in assertion message expansion) were corrected separately and are not counted as sensitivity evidence.
3. Reproduction and integration: default-case runs and a larger explicit-seed run verify ordinary execution and replay configuration. Run the full workspace and exact static gates after restoring production sources. Inspect the final diff for temporary defects and artificial seed files, and keep only cfg(test) module declarations as changes to existing Rust modules. Final executed results are recorded below.

## Sensitivity observations

| Temporary production defect | New property that failed |
| --- | --- |
| Latin-1 raw expansion lookup uses <= instead of < | latin1_coordinates_preserve_arbitrary_bytes |
| Bare CR is ignored as a Python newline | disjoint_git_rows_match_normalized_text |
| Comparison overwrites earlier mismatches at each chunk | stream_equality_and_counts_ignore_read_partitioning |
| Streamed hasher drops the last byte of every chunk | streamed_copy_and_hash_preserve_all_bytes |
| Consumer errors are discarded | consumer_error_stops_the_stream |
| Terminal read errors are treated as EOF | streams_propagate_terminal_read_errors |

These are seeded, bounded sensitivity experiments, not an exhaustive mutation score. Local logs and sensitivity.json are under /tmp/hoimin-proptest-20260926; raw logs are not committed. Repeatable normal test commands and persistence guidance are in docs/development.md. A shrunk input is not asserted to be globally minimal when the shrink iteration limit is reached.

## Independent review

The read-only reviewer found no critical/important issue. Its minor finding was that chunks only asserted is_err for terminal failure; the test now requires WorkspaceError::Io with the expected operation, path and injected error text. Equal already asserted its error kind. Reader schedules, invalid encoding/cookie recognition, nonnormalized Git intervals and filesystem races were explicitly outside this bounded addition and remain so in the development guide.

## Knowledge references

Consulted docs/knowledge/index.md, audits/rust-2026-07.md, and docs/okf-workflow.md. The historical July audit is not evidence for this change. Update the design/report source indexes for these new artifacts; the canonical repeatable procedure lives in docs/development.md. Existing runtime/design contract concepts are unchanged, because no runtime behavior or Lean claim changes.

## Final execution results

- Baseline affected core encoding and CLI line/stream suites passed before test changes. The previous full main result was historical context, not substituted for this run.
- New properties passed at the default configuration, then all eight passed at 1024 cases each with seed 20260926 (8192 successful generated cases). The core pair took 0.16s and the six CLI properties 1.23s in this local debug run; these observations are not CI timing limits.
- Full cargo test --workspace passed: 2492 top-level tests, zero failures, 22 ignored across 123 result groups. Two additional successful subprocess test summaries are excluded from this count.
- Exact CI workspace Clippy and vendored-parser Clippy passed. Both formatting checks and git diff --check passed. All 40 workflow-definition Python tests passed.
- Final source diff confirms existing production functions are unchanged; only two cfg(test) module declarations were added. No dependency changes or artificial regression files remain.
