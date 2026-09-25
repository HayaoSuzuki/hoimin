# Issue 626 progress transition details implementation plan

> **For agentic workers:** Use superpowers:executing-plans to execute this plan inline.

**Goal:** Optional bounded, identifiable improvements/regressions for the final adjacent progress inputs.

**Architecture:** Thread an optional final-pair detail collector through the existing comparison; preserve the default comparison API and output. Render bounded metadata under opt-in schema v2 or quoted human lines.

**Tech Stack:** Rust, clap, serde, existing Lean progress decision model and corpus.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-626-progress-details-design.md`.

## Constraints and review focus

Default output/state/exit remain unchanged; no candidate body clones or history retention. Limits include zero, only explicit details enable v2, final unusable adjacency has no stale details. Confirm different/duplicate IDs cannot be identified through content fallback, altered metadata includes previous/current positions, random map order cannot change top-N selection, JSONL preserves input streaming, and control characters stay quoted.

## Task 1: regression-first public contract

- [x] Add public CLI tests in `crates/hoimin-cli/tests/progress.rs`: actual run strong/weak reports expose regression/improvement ID and positions; `--details --format json` selects v2; without details retain v1. Run these first and record expected unknown-option RED.
- [x] Add fixture tests for cap 0/1/default, stable reversed candidate order, multiple status types, same-content different IDs, duplicate ID/content ambiguity, unusable middle/trailing inputs and original indices, JSON/JSONL equivalence, quoted paths, and malformed CLI limit use.

## Task 2: comparison and presentation

- [x] Add `details: bool` and `details_limit: usize` to parsed progress arguments, default 100 and explicit requires-details validation.
- [x] Add `progress/details.rs` metadata and bounded borrowed collector. Consume only counted changed conclusive pairs, require equal IDs with unique occurrence in both reports. Keep lexicographically smallest N IDs; compute omitted and unidentified separately.
- [x] Extend private accumulator comparison entry point with an optional limit returning details while retaining existing `advance` and public `compare_reports` behavior. In `progress::run`, enable it only for the final pair and preserve barrier handling.
- [x] Extend renderer with optional details: default schema/version/fields unchanged, opt-in v2 adds final-pair details with original input indices. Append quoted human lines and limit/omission/identity constraint summaries.
- [x] Add separate closed v2 JSON schema and README CLI/output contract. Validate v1/v2 with the repository JSON schema test helper.

## Task 3: Lean correspondence and memory

- [x] Extend existing ProgressDecision model/cases/generator with identified transitions and final-pair detail expectations at limit 1. Exclude content-only unequal IDs and ambiguous/inconclusive joins. Add positive/negative/broken-witness checks and generate corpus; never hand-edit generated expectations.
- [x] Request root Lean slot, run each command under 20-second/2-GiB guard, record resource bounds, run corpus freshness/sensitivity. Extend existing Rust adapter to exercise the real public CLI with `--details --details-limit 1` and compare generated detail fields plus unchanged aggregate semantics.
- [x] Add meaningful heap regression proving details do not copy 256-KiB candidate bodies and remain bounded across long history at fixed cap; run alongside existing progress heap tests.

## Task 4: review and completion

- [x] Perform and record three implementation reviews (identity/classification; schema/adjacency/error output; resource/ordering) and three test reviews (sensitivity; malformed/negative/format parity; coverage/measurement limitations).
- [x] Obtain independent read-only review from a peer, address findings, run focused tests and full workspace, exact all-target/all-feature CI clippy plus locked vendor parser clippy and both fmt checks.
- [x] Commit implementation, tests, schema, generated expectations, and review evidence. Root handles publication unless explicitly delegated.

## Plan review passes

1. Requirement coverage: added `unidentified` assertions for fallback joins; omitted alone cannot explain why aggregate counts exceed detail count.
2. Failure/test review: added explicit zero cap, unavailable trailing pair, source control characters, and reversed order assertions; count tests alone would miss these user-visible defects.
3. Integration/resource review: use the existing oracle generator/corpus and public CLI adapter, avoid a second registry target; retain guarded finite bounds and add body-allocation/history checks separately because the model does not establish them.
