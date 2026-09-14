# Fixed batch implementation plan

> Execute inline with the executing-plans workflow; user authorizes autonomous PR completion and requires three self-review passes per stage.

**Goal:** Deterministic nonoverlapping verify ranges.
**Architecture:** Parse optional offset into TopRange; select the global policy prefix and skip its leading offset before enforcing batch size.
**Tech Stack:** Rust, clap, existing plan and report contracts.
**Spec:** `docs/superpowers/specs/2026-09-14-issue-454-fixed-batches-design.md`

## Global constraints

No plan/report schema change. Offset requires top and conflicts with IDs. Preserve Top behavior without offset. Keep saved limits and partial-plan semantics. Saturating arithmetic. Diverse ordering is global.

## Task 1: Range selection

Files: `crates/hoimin-cli/src/cli.rs`, `crates/hoimin-cli/src/plan.rs`, `crates/hoimin-cli/src/plan/selection.rs`, selection unit tests and plan integration tests.

- [x] Add a parser-to-preparation regression creating 120 arithmetic sites then invoking top100 offset0/top20 offset100; compare independent expected ID set, disjointness and limits. Run `cargo test -p hoimin-cli --test plan fixed_batch` and confirm current parser rejects offset.
- [x] Add raw `offset: Option<usize>` with `requires="top"`/ID conflict; emit `TopRange { count, policy, offset }` only when explicitly supplied.
- [x] Add `select_top_candidate_ids_at(candidates, count, policy, offset)`: reject out-of-range at plan resolution, use existing top selection with `offset.saturating_add(count.get()).min(candidates.len())`, then skip offset. Existing callers retain offset zero behavior.
- [x] Match both Top and TopRange in selection resolution, apply max_mutants to returned IDs and preserve metadata requested=count.
- [x] Test known diverse ordering slices across unequal file widths and score tiers; usize::MAX, zero/exact-end/outside/empty/truncated/oversized count, illegal CLI options. Real CLI compare actual IDs and repeat-batch progress.
- [x] Run targeted selection and full plan suites, CLI argument contracts and Clippy/format.

## Task 2: Documentation and PR

- [x] Document batch creation/repetition and separate progress histories in README.
- [x] Update selection OKF with design/source provenance and source indexes; validate YAML, links, hashes and claims separately.
- [x] Record three reviews for each stage and address independent review.
- [x] Commit dedicated branch, push and create PR using repository template; inspect published head/body/checks.

Publication verified: https://github.com/tokyogas-tech/hoimin/pull/539. Current applicable CI checks passed before this documentation-only delivery-state update.
