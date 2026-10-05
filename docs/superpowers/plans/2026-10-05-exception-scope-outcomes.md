# Exception scope and outcomes implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline. Use TDD for changes.

**Goal:** Preserve candidates across unrelated local renames and explain incomplete analysis.
**Architecture:** A bounded analysis report accompanies candidates. A module-local
lexical scope helper resolves typed write/import/alias keys before existing closure
and provider invalidation. Existing project fingerprints and candidate schema stay fixed.
**Tech Stack:** Rust, Ruff AST, Lean 4 with the existing guarded oracle workflow.
**Spec:** `docs/superpowers/specs/2026-10-05-exception-scope-outcomes.md`.

## Constraints and review focus

Use the existing branch and no new dependencies. Each new retained scope/local/edge
table is bounded by 65536 entries; existing input and resolution limits remain.
Lean commands are serial, guarded at 20 seconds / 2048 MiB; proofs keep 50000 heartbeats.
Review headers versus bodies, method closure rules, comprehension walrus targets,
nonlocal resolution with later assignments, and bounded diagnostic aggregation.

## Task 1: explicit analysis outcomes

Files: `crates/hoimin-cli/src/analyzer/exception_hierarchy.rs`, `analyzer/rust.rs`,
`crates/hoimin-cli/tests/exception_hierarchy.rs`.
Produces: a report from `ExceptionIndex::collect`, passed through the AST producer
and formatted with existing diagnostic fields. Scope work consumes this report.

- [x] Write public tests for disabled scope, no relation, constructor policy,
  unsupported tuple, aggregation/location and bare raise; observe RED.
- [x] Replace the empty-vector/boolean inference with explicit outcomes; run
  focused hierarchy tests and review semantics, bounds and integration separately.
- [ ] Commit the outcome implementation and verification record.

## Task 2: lexical identities and formal correspondence

Files: hierarchy analyzer, new `exception_scopes.rs`, alias extraction adapter,
hierarchy integration tests, existing Lean model/proofs/generator/corpus/support.
Produces: typed scoped keys with deterministic serialization in test observations.

- [ ] Write renaming, global, nonlocal, closure, class, header, lambda and
  comprehension regression/control tests; observe the relevant RED results.
- [ ] Collect lexical declarations and resolve scope-qualified references; thread
  keys through aliases/imports/writes and only invalidate reached module bindings.
- [ ] Extend Lean keys/proofs/fixtures; generate expectations, compare extraction
  and public candidates, retain broken-variant sensitivity and actual limits.
- [ ] Review implementation three times (binding semantics, effect propagation,
  resources) and tests three times (positive controls, adversarial cases, integration).
- [ ] Run guarded Lean/freshness/sensitivity, focused tests, workspace tests,
  formatting and all-features Clippy. Obtain the final independent review, fix
  material findings with regressions, update docs and commit.

## Pre-flight and plan review

Task 1 → Task 2: collect now returns a report; adapters observing candidates can
discard it, while tests observing diagnostics must preserve it. No corpus candidate
schema change is needed for Task 1. Scoped extraction schema changes belong to Task 2.

Review 1: public behavior tests precede the outcome API and key refactor.
Review 2: the key adapter preserves scope; stripping it would hide the target defect.
Review 3: syntax coverage needs paired positive/rejection cases and actual entry
limits; a small Lean domain does not establish Rust's resource bound.

## Execution record

Base `97c5bcd`; design/plan reviewed before implementation. User authorized the
follow-up implementation. The existing dedicated branch is reused.

### Task 1 review and verification

Implementation review 1 (semantics): only skipped outcomes warn; disabled scopes
are counted at occurrences, while bare raises are excluded deliberately.
Implementation review 2 (bounds): four skipped reasons and four policy reasons,
saturating counts and one earliest span bound retained report data.
Implementation review 3 (integration): existing diagnostic code and candidate
producer ordering/limits remain; cancellation is checked before publishing results.
Test review 1: policy exclusions and unsupported/disabled cases are paired.
Test review 2: 200 disabled sites assert aggregation and source location.
Test review 3: 22 public integration tests, 21 hierarchy library tests and 125 Lean
public oracle cases passed; the workspace run reached successful doc-tests with
no failures. The new scope regressions fail on eight unrelated-shadowing cases
and the parameter rename case, as expected before Task 2.

Disk ruling: resume builds with CARGO_INCREMENTAL=0 and dev/test debug info disabled;
monitor target and available space, clean generated artifacts between major gates
if necessary. The previous target directory was already absent at resume.
