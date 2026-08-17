# Issue #329: UTF-8 BOM column reporting implementation plan

**Goal:** Ignore a leading file BOM when reporting line-1 columns without
changing source byte spans.

**Architecture:** Keep the existing byte-based line index and source intact.
Apply the BOM exception only to the first-line prefix slice immediately before
its Unicode scalar count.

**Tech stack:** Rust, Ruff Python parser AST ranges, Cargo tests.

## Task 1: Add direct line-index regression coverage

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

1. Add a test whose source starts with `U+FEFF` and also contains `U+FEFF` at
   the start of line 2.
2. Assert offset 0 and the offset after the leading BOM both report column 0.
3. Assert a later line-1 offset excludes the leading BOM.
4. Assert the line-2 `U+FEFF` is still counted normally.
5. Run the exact test and confirm it fails because the current implementation
   counts the leading BOM.

## Task 2: Add analyzer-level regression coverage

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

1. Analyze `"\u{feff}x = 1 + 2\n"` with the arithmetic mutation operator.
2. Find the `+` candidate and assert byte start 9 and zero-based column 6.
3. Run the exact test and confirm the current column is 7 while the byte span
   remains correct.

## Task 3: Implement first-line BOM handling

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

1. Form the source prefix from the selected line start through the queried
   offset as before.
2. On line index zero, remove a leading `U+FEFF` from that prefix when present.
3. Count characters in the resulting prefix and retain the existing checked
   integer conversions.
4. Run both exact regressions and confirm they pass.

## Task 4: Verify and review

1. Run formatting and lint checks.
2. Run the Rust workspace tests and Python tests.
3. Run focused mutation testing for the changed condition when practical.
4. Review the diff for correctness, scope, and issue alignment; address any
   important findings.
5. Commit the design, plan, implementation, and tests.

## Task 5: Deliver and clean up

1. Push the issue branch and open a PR referencing issue #329.
2. Wait for required CI and merge when green.
3. Fast-forward the local main branch and rerun the focused regression.
4. Remove the remote branch, local branch, and issue worktree.
