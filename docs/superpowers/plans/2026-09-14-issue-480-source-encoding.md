# Issue #480 Source Encoding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans inline; no subagents. Follow test-first steps.

**Goal:** Analyze UTF-8, ASCII and Latin-1 Python while keeping candidate and worker byte identities exact.
**Architecture:** Shared core decoding/mapping/encoding; source-aware validation; analyzer span conversion; worker codec writeback.
**Tech Stack:** Rust, Ruff, existing Tokio and CPython test fixtures. No new production dependency.
**Spec:** ../specs/2026-09-14-issue-480-source-encoding-design.md

## Constraints

- Original bytes own hashes, spans and file writes. Metadata strings are Unicode.
- UTF-8/ASCII/Latin-1 only, finite documented aliases; no Python production loader.
- Dedicated target: CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0.
- Dependent PR against enhancement/issue-476 after merging its implementation and adapting symbol decoding.

## Task 1: Codec and candidate contract

Files: create `crates/hoimin-core/src/source_encoding.rs` and `tests/source_encoding.rs`; update `src/{lib,candidate}.rs` and `tests/candidate_policy.rs`.
Interface: `decode_python_source(&[u8]) -> Result<DecodedPythonSource<'_>, SourceEncodingError>`; result exposes `text()`, `encoding()`, `utf8_to_raw(usize)`, `raw_to_utf8(usize)`; encoding exposes `encode(&str) -> Result<Cow<[u8]>, SourceEncodingError>`.

- [ ] Add failing public codec tests with raw Latin-1 bytes, every boundary round-trip, invalid UTF-8 interior rejection, cookie placement, aliases and declaration errors. Run `cargo test -p hoimin-core --test source_encoding` and confirm failures.
- [ ] Implement byte-based cookie recognition, finite codec selection, borrowed UTF-8/ASCII and owned Latin-1 with sparse non-ASCII index; use checked offsets.
- [ ] Add candidate tests with independently chosen raw spans and Unicode columns/replacements. Implement context reuse, raw↔decoded mapping and replacement representability without changing existing validation precedence or stable UTF-8 IDs.

## Task 2: CLI and writeback

Files: `crates/hoimin-cli/src/analyzer/mod.rs`, `src/workspace/mutation.rs`, new `tests/source_encoding.rs`.

- [ ] Add public plan/run/verify tests for Latin-1 candidates on either side of accented identifiers/literals, plus path/declaration diagnostics and unmodified original bytes. Observe UTF-8-only baseline fail.
- [ ] Build validation context before analysis, pass decoded text to Ruff, map each candidate span through the context, keep raw file hashes.
- [ ] Reuse context in worker validation; encode replacement in original codec, then write raw prefix + encoded replacement + raw suffix. Assert replacement with non-ASCII content to catch accidental UTF-8 writes.
- [ ] Run focused tests, workspace/format/Clippy and contracts gates. Record environmental failures honestly and rerun only when evidence justifies it.

## Task 3: Integration, documentation and PR

- [ ] Commit initial implementation, merge enhancement/issue-476, adapt its symbol validation to `decode_python_source(&bytes)?.text()` and verify Latin-1 symbol selection.
- [ ] Update README, analyzer OKF contract and source indexes with final spec/report hashes. Record at least three actual review passes for each stage.
- [ ] Validate YAML, source citations/hashes, links, reachability and claim scope; commit/push; create dependent PR explaining #536 and close #480 only when codec acceptance is fulfilled.
