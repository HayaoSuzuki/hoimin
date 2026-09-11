# Issue #455 Implementation Plan

> Execute task-by-task using executing-plans; autonomous implementation through PR is authorized.

**Goal:** Agree on Python physical lines in analyzer selection and core validation.
**Architecture:** A shared checked u32 physical-line-start vector built from unchanged bytes.
**Tech Stack:** Rust 2024, MSRV 1.88; existing Rust/CPython integration harnesses.
**Spec:** [Python newlines](../specs/2026-09-11-issue-455-python-newlines-design.md)

## Constraints

No new dependency, source normalization or identity/schema change. Retain u32 source limit, linear scan, binary lookup, Unicode scalar columns and analyzer BOM behavior. First-line BOM validation is separately tracked by #469.

## Task 1: Producer and validator

Files: `crates/hoimin-core/src/candidate.rs`, `crates/hoimin-core/tests/candidate_policy.rs`, `crates/hoimin-cli/src/analyzer/rust.rs`, `rust_tests.rs`.

- [x] Add producer tests with `def f():\r    return 1 + 2\r`, expecting one line-2 column-13 candidate when line 2 is selected; compare LF/CRLF and mixed cases.
- [x] Add independent core descriptors over CR/CRLF/LF/mixed text with literal line/column expectations and byte spans, plus rejection of old LF-only coordinates.
- [x] Run regressions and confirm failures on unchanged code.
- [x] Add shared `python_line_starts(source: &[u8]) -> Result<Vec<u32>, CandidateValidationError>` using the existing size-checked builder and this newline-end iterator:

```rust
source.iter().enumerate().filter_map(|(offset, byte)| {
    (*byte == b'\n' || (*byte == b'\r' && source.get(offset + 1) != Some(&b'\n')))
        .then_some(offset + 1)
})
```

- [x] Use the helper from context construction and analyzer LineIndex; preserve the existing u32 guard tests.
- [x] Run core candidate tests and analyzer tests.

## Task 2: CLI and delivery

Files: `crates/hoimin-cli/tests/plan.rs`, README, analyzer OKF and design index.

- [x] Exercise plan/run/verify selecting line 2 across LF/CRLF/CR/mixed endings with a strong arithmetic test; require one killed mutant and valid original span/hash.
- [x] Document stale CR plans requiring regeneration, retain historical report coordinates and separate BOM limitation.
- [x] Run full workspace with contracts, formatting, Clippy and OKF structural/source checks.
- [x] Record three implementation/test/PR self-reviews.
- [ ] Commit and create PR fixing #455.

## Plan self-review

1. Spec coverage: independent core/producer expectations, CLI selection, byte preservation, mixed endings and compatibility are explicitly checked.
2. Interfaces: shared helper returns the current checked vector type and existing error; no new generic source-location framework.
3. Verification: retain oversized virtual-input tests without allocating multi-GiB fixtures; normal BOM tests place candidates after the header so #469 does not conceal newline behavior.

## OKF self-review

1. New rule is attributed to the design, while historical audits retain their original scopes.
2. Source metadata records the actual untracked design hash; no human verification is implied.
3. Existing analyzer concept and design index provide navigation; final YAML/link checks remain required.

## Implementation self-review

1. Byte boundaries: LF always terminates a line; CR only terminates when not followed by LF. Reviewed empty input, CRCRLF, final CR and no final newline against hand-counted indexes.
2. Resource behavior: shared helper delegates to the existing u32 size check before consuming its lazy iterator. Keeps one vector per consumer and no input normalization/copy, with O(n) construction and unchanged binary lookup.
3. Compatibility: candidate IDs exclude line/column; core tests retain the same ID when metadata changes while rejecting the old coordinates. Public verify rejects a stale CR plan before the test-command marker appears. README describes regeneration and retained report history.

## Test self-review

1. Sensitivity: unchanged core rejected a correct lone-CR descriptor; unchanged analyzer produced zero selected candidates where one was expected. Both failures became successes after the shared index change.
2. Independent expectations: literal line starts and Unicode columns, original source byte spans and BLAKE3 hashes are checked. CR, CRLF, LF, mixtures, repeated breaks, missing final terminators and a preceding BOM header are included.
3. Public behavior: plan/run/verify each preserve one line-2 arithmetic candidate; strong CPython assertion kills it. Original source bytes remain unchanged, and stale CR plan rejection occurs before baseline. Focused CLI test passed in all four newline variants.

## Final validation

- `cargo test --workspace --all-features`: 1613 passed, 0 failed, 13 ignored, including both packages' contracts and existing Lean corpus adapters.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`, `git diff --check`: passed.
- OKF YAML/reserved files, 309 source-footnote pairs and 611 local links across 16 pages passed. All pages reachable, all designs indexed and new design hashes matched.
- macOS arm64 / CPython 3.14.7; no new Lean model or local Lean compilation. IDE MCP has no hoimin project open.

## PR self-review

1. Scope and cause: the final diff shares physical-line recognition across the producer and validator; no text normalization, ID or schema change is present. #469 is explicitly a separate limitation.
2. Evidence: matched validation claims to fresh output, retained ignored-test/platform limits, and included the old-code failures and pre-baseline stale-plan rejection. Independent review found no actionable issues.
3. Reviewability: dedicated branch from main 4adf809; PR template references the relevant OKF, design and plan. Only intended source/tests/docs will be staged; the environment symlink stays local.
