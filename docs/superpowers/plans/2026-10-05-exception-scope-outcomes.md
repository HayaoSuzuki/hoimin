# Exception scope and outcomes implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline. Use TDD for changes.

**Goal:** Preserve candidates across unrelated local renames and explain incomplete analysis.
**Architecture:** A bounded analysis report accompanies candidates. A module-local
lexical scope helper resolves typed write/import/alias keys before existing closure
and provider invalidation. Existing project fingerprints and candidate schema stay fixed.
**Tech Stack:** Rust, Ruff AST, Lean 4 with the existing guarded oracle workflow.
**Spec:** `docs/superpowers/specs/2026-10-05-exception-scope-outcomes.md`.

## Constraints and review focus

Use the existing branch and no new dependencies. Scope IDs, per-scope declaration sets, and edge
tables are bounded by 65536 entries; total declaration entries across scopes are
bounded by 131072; existing input and resolution limits remain.
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
- [x] Commit the outcome implementation and verification record.

## Task 2: lexical identities and formal correspondence

Files: hierarchy analyzer, new `exception_scopes.rs`, alias extraction adapter,
hierarchy integration tests, existing Lean model/proofs/generator/corpus/support.
Produces: typed scoped keys with deterministic serialization in test observations.

- [x] Write renaming, global, nonlocal, closure, class, header, lambda and
  comprehension regression/control tests; observe the relevant RED results.
- [x] Collect lexical declarations and resolve scope-qualified references; thread
  keys through aliases/imports/writes and only invalidate reached module bindings.
- [x] Extend Lean keys/proofs/fixtures; generate expectations, compare extraction
  and public candidates, retain broken-variant sensitivity and actual limits.
- [x] Review implementation three times (binding semantics, effect propagation,
  resources) and tests three times (positive controls, adversarial cases, integration).
- [x] Run guarded Lean/freshness/sensitivity, focused tests, workspace tests,
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

### Task 2 resource ruling

Ruling: use 65536 per declaration set and 131072 total declaration entries —
a single total bound of 65536 would reject the existing supported case of 65536
alias assignments inside a function, because the function name also consumes an
entry. The total is independently checked and exercised at 131072/131073. Cost:
up to 131072 declaration entries may be retained/visited, not 65536 total.

Implementation review 1: function locals are collected before resolution; method
free variables skip class namespaces; class loads retain both local and fallback.
Implementation review 2: imports, alias endpoints and writes use the same typed key;
only reached scope-0 names affect module bindings; eager headers remain outside bodies.
Implementation review 3: bounded scopes/declarations complement the existing edge,
import and write caps; limit failure prevents publishing partial summaries.

Task 1 committed as `ddda24f`. Task 2 focused validation passes: all library tests
(765 passed, 12 ignored), 22 hierarchy tests, four scope regression tests, and
151 public Lean cases. The 39 extraction and 170 snapshot cases agree. All-features
Clippy and Lean aggregate/freshness/sensitivity pass. An independent final review
and a fresh workspace run after the implicit-scope fix remain before completion.

### Final completion

Independent review found one Important implicit-class-cell write gap; public RED
expected zero candidates and observed one. Fix `c5f288c` resolves the owner through
aliases/nested closures and preserves local/global/header controls. The final
workspace run passed 2601 tests with 22 ignored, zero failures. Clippy,
formatting, Lean model/proofs, corpus freshness and 18-variant sensitivity passed.
The final corpus contains 160 strict and 215 internal cases. See
`docs/superpowers/reports/2026-10-05-exception-scope-lean-audit.md` for reviews,
resource decisions and remaining model boundaries. All work remains on the branch.
