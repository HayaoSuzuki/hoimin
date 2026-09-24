# Issue #558 implementation plan

Spec: `docs/superpowers/specs/2026-09-24-issue-558-deferred-imports-design.md`.
Goal: prevent deferred annotations from using rebound imported type spellings.
Architecture: add later-binding queries to the existing lexical scope index;
keep source-order import snapshots and filter unsafe imported candidates.
Technology: Rust, Ruff AST, CPython 3.14, existing public CLI harness.

## Constraints and review focus

No new dependency, CLI switch, operator or report schema. Work only in issue-558.
Check source and destination independently; module aliases must not survive via
literal qualified spelling; class lookup differs from function-body lookup;
restoration/cache conservatism must be explicit; unrelated locals must stay valid.

## Tasks

- [x] Add `tests/deferred_annotation_imports.rs` with table-driven public plans:
  `from typing import Sequence; def f(x: Sequence[int]): pass; Sequence = set`
  expects zero, `list[int]` with the same rebind expects zero, class-local late
  binding expects zero. Stable aliases expect exactly one pair with checked span.
  Add actual Python annotation evaluation and public run score assertions.
- [x] Run `cargo test -p hoimin-cli --test deferred_annotation_imports` and record
  RED caused by the invalid emitted candidate before changing production code.
- [x] Extend the existing name scope/index with imported-name binding history (evaluation events after integration with
  #560; byte offsets continue to identify annotation sites).
  Implement `annotation_import_stable(offset, name) -> bool` walking the lexical
  scope tree and respecting class visibility, global/nonlocal directives and
  type parameters. Any visible later write or wildcard uncertainty rejects proof.
- [x] Use this predicate at annotation candidate generation to validate both
  source resolution and destination spelling; preserve existing builtin checks.
  Ensure raw `typing.Sequence` cannot bypass loss of known module provenance.
  Check all referenced imported names in the annotation AST before operator
  eligibility, and perform source/destination proof lazily rather than scanning
  every import for scalar annotations.
- [x] Re-run the new tests GREEN, reconcile old candidate expectations with the
  documented deferred policy, and update #264's original source-order contract.
- [x] Run analyzer/public annotation suites, workspace tests, `cargo fmt --check`,
  and `cargo clippy --workspace --all-targets -- -D warnings`. Record limitations
  and all failures accurately. Use dedicated target directory and two jobs.
- [x] Complete three distinct implementation reviews and three test reviews,
  record findings and fixes, then commit implementation, tests and review log.

## Pre-implementation self-review

Design/plan review details are recorded in the issue-specific review report.
Execution is authorized by the user; implement inline without an approval pause.

- [x] Independent final review: reproduce conditional pre-site import provenance
  loss, retain imported spelling history without cloning/scanning maps, and test
  qualified prohibited descendants, local annotations and sibling-scope positives.
