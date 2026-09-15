# Issue #546 implementation plan

> Execute autonomously as authorized. Work only in `.worktrees/issue-546` on `perf/issue-546`.

**Goal:** Remove exponential transfer-only nested-loop replay while preserving annotation callbacks and control-flow exits.
**Design:** [loop transfer](../specs/2026-09-15-issue-546-loop-transfer-design.md).

## 1. Regression before implementation

In `crates/hoimin-cli/src/analyzer/rust.rs`, add cfg(test) counters at the entry to `visit_statement_flow`: increment statements for every call and leaves for AnnAssign. Add `nested_loop_transfer_tests` with a source generator for nested for/while and an isolated operator selection. Use depths `[1, 2, 4, 8, 16, 20]`, both type_list_sequence and boolean_literal, and assert zero candidates, no diagnostics, and no truncation.

```rust
assert!(leaves <= depth + 1);
assert!(statements <= (depth + 1) * (depth + 2) / 2);
```

Run `cargo clean -p hoimin-cli` against the isolated target, then `cargo test -p hoimin-cli --lib nested_loop_transfer_tests -- --nocapture` and capture the expected red result in `/private/tmp/hoimin-546-red.log`. Keep production control flow unchanged until failure is observed.

## 2. Reuse final transfer

Return `LoopHeadTransfer { head: KnownImports, exits: ControlFlowExits, fallback_unchanged: bool }` from loop_head_fixed_point; retain the converged body exits. Keep the original head computation, including for-target invalidation and continue back-edges. A helper may intersect borrowed states to avoid cloning final exits. Update existing test-only loop-head callers to select `.head`.

```rust
let transfer = self.loop_head_fixed_point(body_imports, iteration_target, body);
let body_exits = if self.can_reuse_loop_transfer(transfer.fallback_unchanged) {
    drop(transfer.head);
    transfer.exits
} else {
    drop(transfer.exits);
    self.visit_suite_from(transfer.head, body)
};
```

The helper requires !record_annotations and equality of class_body_fallback before and after the final transfer. Capture that fallback only for the active evaluation, drop the snapshot before returning, and add the same depth gate inside a class (one additional statement). Under cfg(test), any marker/handler/try projection or test_mutation disables reuse. A separate test-only disabled-reuse switch enables sensitivity and differential tests; it does not alter transfer semantics.

## 3. Semantic and sensitivity checks

Compare callback observations and normalized final exits with reuse enabled and disabled for changing aliases, continue, break/else, finally, for-target invalidation, function scope, class global fallback, and nested class/method definitions. Include empty-state and nonempty-state annotations. Assert actual callback count and known resolved alias observations so equal empty outputs cannot pass.

Run the old-path sensitivity test at a small depth and require its visits to exceed the same performance bound while preserving semantic output. Run all analyzer tests and the existing binding-flow/annotation-scope/exception-match Lean adapters in default and contracts builds.

## 4. Documentation and gate

Add an active exact test gate to `docs/performance/shapes.json` and explain the separately measured control-flow dimension in `docs/performance/README.md`. Update existing analyzer/performance OKF concepts and the spec/report catalogs, preserving historical metadata and adding hashes only after the new sources stop changing. Run `/private/tmp/hoimin-validate-okf.py <worktree> 546` with the local .venv Python.

## 5. Verification and handoff

Run `cargo fmt --all -- --check`, focused test suites, `cargo clippy -p hoimin-cli --all-targets -- -D warnings`, and `git diff --check`. Record commands, outcomes, and five substantive self-review rounds for worktree, OKF, design, plan, implementation, tests, and PR preparation in `docs/superpowers/reports/2026-09-15-issue-546-loop-transfer-review.md`. Prepare a PR body following `.github/pull_request_template.md`; commit changes and hand commit plus PR text to the coordinating agent for push/PR creation.
