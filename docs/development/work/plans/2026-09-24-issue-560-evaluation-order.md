# Issue 560 implementation plan

Goal: resolve builtin provenance at Python evaluation events while preserving source spans.
Spec: ../specs/2026-09-24-issue-560-evaluation-order.md
Execution: implement inline in the issue worktree, as authorized by the user.

## Constraints and review focus

No dependencies or public schema changes. Retain binary-search binding-history lookup complexity. Review definition self-shadowing, comprehension first iterable, partial stores before exceptions, augmented assignment target evaluation, and keyword/starred source-order inversion.

## Task 1: demonstrate behavioral failures

- Add table-driven Rust analyzer tests to crates/hoimin-cli/src/analyzer/rust_tests.rs. Assert builtin-pair candidate counts for sequential/nested stores, walrus RHS, keyword/starred argument order, and source/destination bindings. Pair negative fixtures with earlier RHS lookups and augassign controls.
- Run `cargo test -p hoimin-cli --lib evaluation_order` before changing production code; confirm failures are unexpected candidate counts.
- Add crates/hoimin-cli/tests/builtin_evaluation_order.rs public plan and run tests using the real CLI and temporary subject files.

## Task 2: index evaluation events

- Modify crates/hoimin-cli/src/analyzer/rust.rs: add evaluation event to NameOccurrence and counter to NameResolutionBuilder. Occurrence offsets remain map keys, while resolve_scope receives occurrence.event.
- Have recording helpers append binding effects with the current event; advance events for lookups and bindings. Remove source-offset comprehension boundaries because the first iterable is now traversed before conditional outward writes.
- Explicitly traverse assignment RHS then recursively visit/store each target. Traverse augassign target, RHS, store. Traverse call arguments as args followed by keywords. Move definition stores after their eagerly evaluated headers/bodies. Keep scope, loop-backedge, annotation, and conditional-path machinery.
- Run focused regressions and all analyzer library tests; correct any evaluation-order regressions.

## Task 3: verify and deliver

- Run public plan/run regression tests, `cargo test --workspace`, `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets -- -D warnings`.
- Perform three separate implementation reviews and three separate test reviews. Record findings, corrections, and actual command results in the issue review log, including limitations.
- Commit implementation/tests/log and hand the clean branch to the parent for push and PR creation.
