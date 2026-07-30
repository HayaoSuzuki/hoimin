# Ordered Candidate Duplicate Handling Implementation Plan

**Goal:** Prevent duplicate ordered candidate IDs from panicking while
preserving the first requested occurrence of each unique ID.

**Architecture:** Normalize ordered IDs at `RunState` construction so the
membership set and ordered sequence share one unique representation. Keep the
existing public constructor signature and downstream machine transitions.

**Tech Stack:** Rust, `BTreeSet`, hoimin-core state-machine integration tests.

## Task 1: Add the regression test

**Files:**

- Modify: `crates/hoimin-core/tests/machine.rs`

1. Add a test that requests `[second, first, second]`.
2. Drive analysis and all candidate reads through end-of-spool.
3. Assert the first scheduled mutation is `second`.
4. Complete it and assert the next scheduled mutation is `first`.
5. Complete it and assert the machine finalizes without scheduling `second`
   again.
6. Run the focused test and confirm it fails on the existing panic.

## Task 2: Stable-deduplicate at construction

**Files:**

- Modify: `crates/hoimin-core/src/machine.rs`

1. Build the ordered vector and membership set together in one pass.
2. Append an ID only when insertion into the set succeeds.
3. Document first-occurrence stable deduplication on
   `with_ordered_candidate_filter`.
4. Run the focused test and confirm it passes.

## Task 3: Verify and deliver

1. Run `cargo fmt --check`.
2. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
3. Run `cargo test --workspace`.
4. Run `git diff --check`.
5. Review the final diff against Issue #69 acceptance criteria.
6. Commit the implementation, push the branch, and create a PR closing #69.
