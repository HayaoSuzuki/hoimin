# Issue #557 implementation plan

Design: [finally traversal](../specs/2026-09-25-issue-557-finally-traversal.md).
User authorized autonomous implementation and PR preparation; parent owns publication and OKF updates. Worktree `.worktrees/issue-557`, branch `perf/issue-557-finally-traversal`.

## Task 1: demonstrate the regression

Add `rust/finally_transfer_tests.rs` and cfg(test) module. Reuse actual statement/AnnAssign counters. Generate nested finally fixtures at depths 1,2,4,8,16,17,18,19,20, module/class/function scopes, and selected/unselected type operator. Assert candidate count, original/replacement and non-truncation; require leaves <= d+1, statements <= (d+1)^2+1 (+ wrapper). Add a direct record-disabled collector test requiring one leaf visit. Run focused tests before changing production logic; expected failure at depth 1 for transfer-only and depth 2 for recording path. Save log in /private/tmp/hoimin-557-red.log.

## Task 2: remove redundant recording traversal

Guard annotation-entry construction and traversal in apply_finally with record_annotations. Keep normal/abrupt/implicit routing unchanged. Add cfg(test) old-path switch with Drop restoration for sensitivity. Compare old/new descriptors, callbacks and normalized exits for multi-exit finally and class fallback fixtures. Check non-empty callback observations. Existing projection adapters must continue passing. Expected focused suite: all pass; old path at depth8 must exceed the same bound and visit the leaf256 times.

## Task 3: verify and record

Build an unchanged release baseline before production edit, preserve binary in /private/tmp/hoimin-557-before, then build final release with isolated CARGO_TARGET_DIR=/private/tmp/hoimin-issue-557-target. Use CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=2 and RUST_TEST_THREADS=2. Run depth16–20 public-plan benchmark, three samples each, compile each fixture with CPython and compare full candidate arrays before/after.

Add an active exact operation-count gate to docs/performance/shapes.json and explain limits in docs/performance/README.md. Run focused default/contracts tests, implicit-finally/binding-flow/nested-try/annotation-scope adapters, cargo test --offline --workspace, fmt, clippy and registry check. Record each result and three substantive self-review passes each for implementation and tests in ../reviews/2026-09-25-issue-557.md. Parent updates OKF concepts/catalogs. Commit verified work; no push from this agent.

## Review focus

- Recording-disabled direct entry must skip the annotation pass, including nested and empty-entry paths.
- Multi-exit and implicit exception routing must not reuse a joined recording result.
- Class global fallback/method annotations must retain callback facts and exits.
- Full candidate descriptors and ordering must match, not only counts.
- Depth gates count executed statements, not inferred or theoretical visits.

## Executable checklist (review refinement)

**Goal:** Remove transfer-only nested-finally annotation replays while preserving descriptors and exit semantics.
**Architecture:** Gate only the merged-entry recording pass; keep per-entry transfer routes.
**Tech stack:** Rust, Ruff AST, cargo tests, existing Lean corpora, release CLI.

- [ ] Task 1: run `cargo test --offline -p hoimin-cli --lib nested_finally_transfer_tests -- --nocapture`; expect `record_disabled_finally_visits_leaf_once` to report two visits at depth1, and `nested_finally_bounds_actual_statement_and_annotation_visits` to exceed the leaf bound at depth2.
  Representative independent assertions:
  ```rust
  assert_eq!(LOOP_ANNOTATION_VISITS.get(), 1);
  assert!(leaves <= depth + 1);
  assert!(statements <= (depth + 1).pow(2) + 1 + usize::from(!wrapper.is_empty()));
  ```
- [ ] Task 2: wrap annotation_entries construction through its visit_suite_from in a recording condition. Test builds additionally admit a thread-local old-path sensitivity switch:
  ```rust
  let record_annotations = self.record_annotations;
  #[cfg(test)]
  let record_annotations = record_annotations || REDUNDANT_FINALLY_RECORDING.get();
  if record_annotations {
      // Existing merged-entry construction and recording visit, unchanged.
  }
  ```
  Run the same command; expect all depth gates green. Add `old_finally_replay_exceeds_bound_with_identical_candidates`, `finally_routing_preserves_callbacks_and_all_exit_categories` and `empty_entry_finally_records_only_when_enabled`.
- [ ] Task 3: `cargo test --offline -p hoimin-cli --features contracts --lib nested_finally_transfer_tests`; then `cargo test --offline -p hoimin-cli --features contracts --test lean_implicit_finally_oracle --test lean_binding_flow_oracle --test lean_nested_try_flow_oracle --test lean_annotation_scope_oracle`. Expect zero failures.
- [ ] Task 3: `cargo test --offline --workspace`, `cargo fmt --all -- --check`, `cargo clippy --offline -p hoimin-cli --all-targets -- -D warnings`, `python3 tools/performance_shapes.py check`, `git diff --check`. Expect exit0; report any infrastructure limitations explicitly.
- [ ] Task 3: `cargo build --offline --release -p hoimin-cli`, followed by the issue reproduction over depths16–20 against saved before/after binaries. Expect one identical candidate array for every run; record each median and no timing threshold.
- [ ] Record three implementation/test review passes, then commit verified code/docs. Parent publishes PR.
