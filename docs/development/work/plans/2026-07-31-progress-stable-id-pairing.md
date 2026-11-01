# Progress Stable-ID Pairing Implementation Plan

> **Issue:** #102 — pair progress mutants by stable ID when candidate sets match

## Goal

Ensure progress comparisons detect improvements and regressions for distinct
mutants that share the same content identity whenever both reports contain the
same unique stable candidate IDs.

## Design

`candidate_set_eligibility` already proves that a conclusive comparison has
unique, identical candidate-ID sets. Use candidate IDs as comparison keys for
that `Matching` path. Preserve content-key pairing and ambiguity reporting for
`Different` and `Duplicate` candidate sets, where comparisons are
informational and stable IDs cannot establish a complete one-to-one set.

Represent both strategies with one internal comparison-key enum so counting,
status transitions, score calculation, and inconclusive handling continue
through one implementation.

## Task 1: Add failing stable-ID regressions

**Files:**

- Modify: `crates/hoimin-cli/tests/progress.rs`

Add tests with two mutants that deliberately share a content key but have
different stable IDs. Verify matching candidate-ID sets:

1. Count both mutants as common and none as ambiguous.
2. Detect a killed-to-survived transition as a regression.
3. Break the stall chain and report `Regressing`.

Keep or extend coverage proving different candidate-ID sets still use the
content-key ambiguity behavior and remain indeterminate.

Run focused tests and confirm the matching-ID regression fails before changing
production code.

## Task 2: Select the pairing key from eligibility

**Files:**

- Modify: `crates/hoimin-cli/src/progress/compare.rs`

Add an internal key type for stable candidate IDs and content identities. Index
matching reports by stable ID; index non-matching reports by content key.
Retain duplicate-key exclusion only where content pairing is used.

Run focused progress tests, the full progress suite, formatting, Clippy, all
workspace tests, contract-feature tests, and `git diff --check`. Request an
independent review before creating the PR.
