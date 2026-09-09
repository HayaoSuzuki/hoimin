# Issue 446 explicit line ranking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Eliminate candidate-by-line-selector scanning without changing ranking results.

**Architecture:** A pure core LineSelectionIndex shares existing logical path equality and merged interval semantics; the CLI builds and queries it once per ranking call.

**Tech Stack:** Rust1.88+, existing BTreeMap/Camino/hoimin-core.

**Spec:** docs/superpowers/specs/2026-09-09-issue-446-line-ranking-design.md

## Global Constraints

- Work only in .worktrees/issue-446-line-ranking on perf/issue-446-line-ranking, independently based on58817cf.
- No serialized schema change, ranking rule version change, dependency addition, or output ordering/score/ID change.
- Reuse core platform path equality; do not reimplement Windows Unicode folding in CLI.
- Rust MSRV1.88; keep core pure; minimize source comments.
- Bound retained memory to selector metadata and avoid O(N×L), including a single file with many ranges.

### Task 1: Implement core line index and integrate ranking

**Files:**
- Modify: crates/hoimin-core/src/target.rs (or focused target/line_index.rs module with re-export from target.rs)
- Test: crates/hoimin-core/tests/line_selection_index.rs
- Modify: crates/hoimin-cli/src/plan/ranking.rs
- Test: crates/hoimin-cli/src/plan/ranking_tests.rs
- Document: docs/superpowers/reports/2026-09-09-issue-446-line-ranking.md

**Interfaces:** Introduce public LineSelectionIndex::new(root, selections) and contains(path,line) as specified. Consume existing path_equality_key and normalize_ranges internally. ranking's external interfaces remain unchanged.

- [ ] Add tests for the new index before implementation. First require inclusive endpoints and gaps with ranges [2,4], [4,6], [9,9], compare queries0..12 with this reference:

```rust
let expected = selections.iter().any(|selected| {
    normalize_logical_path(root, &selected.path)
        .is_ok_and(|normalized| logical_paths_equal(&normalized, candidate_path))
        && selected.range.start <= line && line <= selected.range.end
});
assert_eq!(index.contains(candidate_path, line), expected);
```

Record the initial unavailable-index failure as the RED for the new API. The performance defect is separately established by the existing old-production benchmark; output compatibility tests are expected to pass against both algorithms.

- [ ] Implement grouped normalized intervals with existing core equality keys. Preserve the old predicate for invalid selector paths and inverted intervals. Use binary search rather than any over every interval:

```rust
let end = ranges.partition_point(|range| range.start <= line);
end.checked_sub(1).is_some_and(|index| line <= ranges[index].end)
```

- [ ] Add deterministic differential cases for repeated/overlapping/adjacent/sparse ranges; multiple files; 0/u32::MAX; absolute/relative/dot aliases; invalid and mismatched paths. Test Unix case and literal backslash differences. Add cfg(windows) tests for case, separators and non-ASCII simple-uppercase behavior using the same oracle.
- [ ] Replace ranking's ExplicitLine loop with a single constructed index and contains(candidate.path,candidate.line). Preserve selected_symbols and all scoring/sorting logic.
- [ ] Extend ranking tests to exercise mixed line/file/changed/symbol conditions, multiple files and gaps; compare reasons, scores, ranks and ordering. Run `cargo test --offline -p hoimin-core --test line_selection_index` and focused `cargo test --offline -p hoimin-cli --lib plan::ranking_tests`.
- [ ] Self-review, record focused evidence in the tracked report and commit code/tests/docs. Controller owns release comparison, whole-workspace/MSRV/Clippy gates, reviews and PR; do not duplicate those suites or publish.

## Controller validation and delivery

- Run release benchmark /private/tmp/hoimin-446-benchmark against actual before/after modules and assert complete JSON equality for every sample.
- Run workspace/all-features tests, MSRV1.88 check/all-targets/all-features, Clippy same scope warnings denied, fmt and diff checks.
- Run task review, required fixes/scoped re-review, then final whole-branch review. Complete report and PR closing #446. Keep the worktree for follow-up.

## Plan self-review 1 — scope and RED evidence

Mapped every Issue criterion to the index tests, ranking tests or controller measurements. Output tests cannot expose a performance-only defect; the plan explicitly separates the new-API RED and existing production performance evidence from the equivalence oracle. Added direct candidate path alias cases so the implementation cannot normalize candidate paths more broadly than the old predicate.

## Plan self-review 2 — interfaces and portability

The new type belongs in core because it needs private path equality and range policy. Public methods are additive and have no serde surface; the plan does not claim zero Rust API additions. Tests must cover direct candidate dot paths, inverted ranges (never matching), empty selectors, and start=0 raw API behavior. Windows-specific assertions are conditional and must reuse the existing simple-uppercase contract, not full Unicode uppercase expansion.

## Plan self-review 3 — completeness

Reviewed design-to-task coverage, concrete paths and validation ownership. Every behavior and scale requirement has a test or a controller benchmark, and verification reuses the new ranker automatically. Integration tests are meaningful even though correctness was already present: they guard exact scores/reasons/order while the index changes. No placeholder or conflicting task interface remains; one task owns both sides of the core/CLI interface.
