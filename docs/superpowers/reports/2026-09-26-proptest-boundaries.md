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

## Initial execution results (b1641ee, before CI follow-up)

- Baseline affected core encoding and CLI line/stream suites passed before test changes. The previous full main result was historical context, not substituted for this run.
- New properties passed at the default configuration, then all eight passed at 1024 cases each with seed 20260926 (8192 successful generated cases). The core pair took 0.16s and the six CLI properties 1.23s in this local debug run; these observations are not CI timing limits.
- Full cargo test --workspace passed: 2492 top-level tests, zero failures, 22 ignored across 123 result groups. Two additional successful subprocess test summaries are excluded from this count.
- Exact CI workspace Clippy and vendored-parser Clippy passed. Both formatting checks and git diff --check passed. All 40 workflow-definition Python tests passed.
- Final source diff confirms existing production functions are unchanged; only two cfg(test) module declarations were added. No dependency changes or artificial regression files remain.

## Randomized CI follow-up

Job 108135063356 on PR 663 failed the existing managed-child ownership test: after dropping the final local child, reclaimed_roots was 0 instead of 1. The new proptest cases passed; all other CI jobs passed. The log did not include ReclaimReport details. The original shuffle seed 1790350533343656379 passed locally on macOS before the fix, so the historical Linux scheduling interleaving is not asserted to have been reproduced.

A controlled temporary fork probe retained an inherited lease descriptor while the local owner/child were dropped. It reproduced the same 0-versus-1 failure with preserved_roots=1 and empty details. Releasing and reaping that process before reclamation passed. Existing Linux process setup uses pre_exec, which gives unrelated parallel tests a plausible fork-to-exec descriptor inheritance window. These probes establish a concrete failure mechanism, not definitive attribution of the original CI occurrence. Both probes were restored; no fork/unsafe test instrumentation ships.

The ownership assertions now run in a bounded subprocess, with the fixture created only after that process starts. The parent harness requires successful exit and a completion marker emitted after all child assertions; a deliberately nonexistent exact test name was rejected despite its zero-test successful exit. The timeout uses kill_on_drop and does not claim synchronous reaping before runtime shutdown. Runtime ownership and reclamation code are unchanged.

### Follow-up implementation reviews

1. Isolation: child-only Command environment, exact test name, single test thread, and fixture creation after re-exec exclude unrelated harness forks from the lease-owning process. No global environment mutation or production unlock is introduced.
2. Assertion preservation: live root remains present/unreclaimed and is now explicitly reported preserved; after dropping the child, exactly one root must be reclaimed and its path absent. Failure messages include complete ReclaimReport values.
3. Process outcome: the completion sentinel prevents zero-test false success. Captured child output explains failures, and a 30-second timeout with kill-on-drop bounds hangs. Independent review found the sentinel omission before it was corrected; no production or portability blocker was found.

### Follow-up test reviews and results

1. Controlled inheritance RED failed the original strict assertion; release/reap control passed. The original CI seed was tested both before and after the fix, and its pre-fix local success is retained as a reproduction limitation.
2. Zero-test negative control failed as intended. The final isolated test passed 100/100 executions. Final pinned-nightly library run with the original shuffle seed passed 738 tests, zero failures, 12 ignored on macOS.
3. Exact workspace Clippy, formatting and diff checks passed after the sentinel change. The prior full-workspace evidence predates this test-only follow-up; remote CI is rerun for the final branch. No temporary probe, assertion mutation or artificial seed remains in the diff.
