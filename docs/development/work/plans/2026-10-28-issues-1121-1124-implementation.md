# Slice-bound and division operators implementation plan

> For agentic workers: use `superpowers:executing-plans` to implement task by task.

**Goal:** Implement the three operators accepted after Issues 1121/1124 research,
enabled by default with explicit exclusion. Keep Issue 1122's deferral decision.

**Architecture:** Extend the existing typed operator catalog and candidate
producers. Division reuses eligible binary tokens; slices reuse AST context,
parenthesized bound ranges and existing bounded/cancellable admission. No new
dependency, scheduler, saved schema or compatibility path.

**Tech Stack:** Rust, Ruff AST/tokens, existing Rust and CLI tests, OKF Markdown.

**Spec:** [Accepted research scope](../reports/issues-1121-1122-1124/README.md).
The maintainer explicitly requested implementation after reviewing that scope.

## Global constraints

- Default-on IDs: `binary_div_floor`, `structure_slice_bound_omit`,
  `structure_slice_bound_move`. Existing `--exclude-operators` opts out.
- Slice omission emits at most two candidates for two explicit bounds; movement
  emits one for exactly one bound. Only load-context, one-dimensional slices
  without a step value, with name/integer bounds. No calls, attributes or
  compound bounds, annotation, assignment/deletion targets or full-range edits.
- Binary `/` and `//` only; preserve operand evaluation and all old arithmetic
  mutations. No augmented-assignment or operator-function expansion.
- Retain candidate budgets, cancellation, target selection and syntax validity.
- Cargo uses at most two jobs, incremental off and debug information off. Check
  free space before broad verification and clean after needed artifacts are used.
- No version-floor or schema changes. Old explicit plans keep their own operator
  sets; create a new plan to use new defaults.

## Review focus

Parenthesized/commented bounds; UTF-8 byte offsets; exclusions despite nested
expressions; deterministic truncation/selection; runtime behavior versus parse-only
success. Each stage gets five self-review passes recorded with the implementation.

## Stage 1: catalog and binary division

- [x] Add a default-discovery assertion for both directions and observe zero
  candidates before implementation.
- [x] Append typed enum IDs, defaults and structural selector membership;
  classify new ranking reasons using the existing arithmetic/behavioral rules.
- [x] Emit the extra division-token candidate through shared bounded admission.
- [x] Verify exact edits, annotation/augmented exclusions and reparsing; review
  identity, defaults, emission, resources and evidence separately.

## Stage 2: slice-bound mutation

- [x] Observe missing default candidates, then implement omission and movement
  in the existing subscript collector.
- [x] Test literal/name/negative bounds, parentheses/comments, Unicode,
  unsupported contexts, exact selection, candidate caps and cancellation.
- [x] Ensure range helpers preserve balanced syntax and bound evaluation once.
- [x] Review applicability, syntax, semantics, resource admission and evidence.

## Stage 3: public plan/verify and regression checks

- [x] Test default discovery, explicit exclusion and `structure_ops` expansion.
- [x] Execute controlled weak/strong plan/verify cases on Python 3.14 and 3.15;
  verify intended behavioral kills, not only successful reparsing.
- [x] Run workspace Rust tests, relevant Python tests, formatting and clippy
  with bounded concurrency; report any host-specific failures explicitly.
- [x] Review public contracts, old operators, execution outcomes, budgets and
  measurement limitations.

## Stage 4: documentation and branch integration

- [x] Update usage, generated CLI reference, current OKF claims/source metadata
  and PR #1133. Preserve old research evidence and distinguish new execution.
- [x] Record five final reviews, validate knowledge structure/references/hashes,
  commit only intended files and push the existing branch.
- [x] Clean build artifacts after checks; retain source and committed evidence.
