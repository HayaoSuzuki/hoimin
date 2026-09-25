# Compact Latin-1 positions implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline; root arranges independent review. No delegated implementation or approval pause is required by the authorized task.

**Goal:** Reduce both redundant Latin-1 indexes and demonstrate the allocation savings through public APIs.

**Architecture:** One usize coordinate per decoder expansion, plus raw physical-line starts for Latin-1 validation. Preserve the existing Unicode index for all other encodings.

**Tech Stack:** Rust, Cargo integration tests, System allocator probe, CPython/public CLI checks.

**Spec:** docs/superpowers/specs/2026-09-25-issue-615-latin1-positions-design.md

## Global constraints

No codec removal, byte normalization, input-limit reduction, schema/ID change, or unrelated Git-line fix. Use the assigned batch-analyzer Cargo target, one build job, debug information and incremental compilation disabled. No Lean process is planned: the behavioral algebra is checked independently with exhaustive byte-boundary tests, while the performance claim needs actual allocator measurements.

## Review focus

- Adjacent expansions and EOF: round-trip every raw boundary and reject every UTF-8 scalar interior.
- CRLF versus lone CR and 0x85: count physical lines only by Python's existing newline contract.
- UTF-8 BOM and multibyte scalars: keep the public Unicode index and its tests unchanged.
- Decoder versus context size limits: preserve usize decoder support and both u32 context checks.
- Same-line non-ASCII before a mutation: assert raw span/hash/ID, Unicode column, and worker bytes.

## Task 1: establish baseline and failing allocation gates

Files: new crates/hoimin-cli/tests/source_encoding_heap.rs; existing tests/support/heap_tracking.rs reused unchanged; temporary issue probe artifacts under /tmp/hoimin-batch-604-632/615-perf.

- [x] Run baseline core source_encoding and candidate_policy tests.
- [x] Build release baseline before production edits, preserve CLI/probe executables, extract the issue's public-API System allocator probe, and generate its exact three sizes/codecs. Measure each three times, checking drop returns live allocation to baseline; record public-plan candidate metadata and RSS.
- [x] Add one isolated integration test owning the process-global allocator. For sizes 65536,262144,524256 and non-ASCII strides 0,64,2,1, construct a three-line one-candidate source before measurement. Measure decode and context independently.

```rust
let decoder_bound = 2 * source.len() + expansion_count.next_power_of_two() * size_of::<usize>() + 1024;
assert!(decode_peak <= decoder_bound);
assert!(context_peak <= decode_peak + 1024);
```

The first gate rejects paired usize entries; the second rejects a duplicate Unicode correction table. Handle zero expansions without allocating an artificial correction entry. Check ASCII/UTF-8 controls separately.

- [x] Run the new test and capture both expected failures before modifying production code. If short-circuit assertions mask the second failure, collect measurements then report all failed bounds together.

## Task 2: compact mapping and raw Latin-1 locations

Files: crates/hoimin-core/src/source_encoding.rs; crates/hoimin-core/src/candidate.rs; crates/hoimin-core/tests/source_encoding.rs.

- [x] Add exhaustive boundary tests against a separate scalar-width prefix oracle for all byte values and adjacent/sparse expansions. Compare public context validation metadata with decoded PythonSourceIndex across LF/CRLF/CR/mixed line endings, including 0x85 and EOF; retain invalid replacement/span/location cases.
- [x] Replace paired decoder entries with raw starts. Reverse mapping uses binary search over entry indexes and the equation `decoded_end = raw_start + index + 2`; forward mapping retains partition_point. No linear prefix scans.
- [x] Add the private context enum and construction/lookup helpers. Check decoded length fits u32 before choosing the variant; Latin-1 uses python_line_starts(raw), the other variants use PythonSourceIndex::new(decoded). Keep raw-to-decoded start/end checks in validation; choose raw versus decoded location coordinate internally.
- [x] Run allocation gates and core mapping/location tests; confirm measured heap improvement and unchanged byte contracts.
- [x] Review implementation three times: arithmetic/boundaries; compatibility/error ordering; retained allocations/lookup complexity. Fix findings and record evidence.

## Task 3: public flow, performance, and completion

Files: crates/hoimin-cli/tests/source_encoding.rs; docs/superpowers/reports/2026-09-25-issue-615-latin1-positions.md.

- [x] Extend the existing Latin-1 plan/run/verify test with a dense comment plus non-ASCII before a same-line candidate for LF/CRLF/CR. Compare manifest metadata and actual worker byte arrays; keep exact candidate identities consistent across plan/run/verify.
- [x] Repeat baseline release measurements after implementation on identical fixtures/flags: three separate processes per API mode and CLI input. Record retained/peak requested bytes and process RSS independently.
- [x] Use a temporary public-API timing probe for decoder construction, context construction, and 100000 pseudo-random/reverse raw/decoded mapping queries. Record medians and ranges over repeated trials at multiple sizes/densities; correctness-check a digest. No wall-clock CI threshold.
- [x] Review tests three times: independent oracle/RED sensitivity; codec/newline/boundary coverage; measurement isolation/reproducibility. Record findings and any corrections.
- [x] Run full workspace tests, exact workspace all-target/all-feature Clippy and locked vendor Clippy gates, workspace/vendor formatting, and diff checks. Run workflow Python tests only if registration files change.
- [x] Obtain root-arranged independent review, fix blockers, and commit implementation/report. Publish via ghstack after integration checks; record the resulting PR and evidence in the handoff to root.

## Plan reviews, before implementation

1. Acceptance coverage: the issue requires real measurements of both context and public plan, plus density and query-time evidence. Added isolated allocation test and preserved baseline binaries before any production edits; estimates alone do not satisfy completion.
2. Test independence: merely checking Vec element widths would mirror implementation. The committed gate measures public API allocations; functional tests compute prefix widths and compare decoded location semantics independently. One test per allocator executable prevents cross-test races.
3. Execution/scope: no production changes should precede the design commit or RED evidence. Release baseline must complete first. Existing codec writeback flow is extended, not replaced by a decoder-only test. Review and CI commands remain required despite focused tests passing.
