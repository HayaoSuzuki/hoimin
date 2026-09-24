# With suppression implementation plan

> Execute inline with the executing-plans skill; the user explicitly authorized
> implementation without approval pauses and requested separate self-reviews.

**Goal:** Prevent typing mutations after suppressed exceptions invalidate definite imports.

**Architecture:** Distinguish explicit raised exits from return exits. Enable body
exception collection for with, route possible suppressed states to its successor,
and preserve possible unsuppressed exits for outer control flow.

**Tech Stack:** Rust, Ruff Python AST, Cargo tests, CPython 3.14.

**Spec:** `docs/superpowers/specs/2026-09-24-issue-556-with-suppression.md`.

## Constraints and review focus

Keep first-manager entry distinct from later entry; test positive normal import.
Keep target binding after manager entry; test destructuring target failures.
Keep finally overrides distinct from resumed exceptions; test restoration and return.
Keep successful return/break/continue out of suppressed flow; test successor facts.
Keep deferred bodies from contributing exceptions; test function/lambda boundaries.

## Task 1: analysis and public behavior

Files: `crates/hoimin-cli/src/analyzer/rust.rs`,
`crates/hoimin-cli/src/analyzer/rust_tests.rs`,
`crates/hoimin-cli/tests/with_suppression.rs`, and `docs/development.md`.
The production interface remains `visit_with(...) -> ControlFlowExits`.

- [ ] Add table-driven analyzer tests using
  `analyze_with_only_operator(source, MutationOperator::TypeListSequence)` and
  literal expected counts. Minimal negative: `Sequence = set; with manager:
  hazard(); from typing import Sequence` followed by `value: Sequence[int]`.
  Expect 0; reverse call/import order expects 1. Include the review focus cases.
- [ ] Add CLI tests loading `formal/HoiminOracle/corpus/with-suppression.jsonl`.
  Execute source with injected normal/raising hazard under CPython 3.14, compare
  `typing.get_origin(observed) is collections.abc.Sequence` with literal corpus
  expectations. Run `plan --operators type_list_sequence` and compare candidate
  counts. Run the original failing subject/check pair with `run --format json`;
  assert no killed mutation and successful baseline.
- [ ] Run `cargo test -p hoimin-cli --lib with_suppression` and
  `cargo test -p hoimin-cli --test with_suppression`; retain exact RED evidence.
- [ ] Extend `ControlFlowExits` with explicit raises and propagate through
  `merge_abrupt`, loop/try/finally, and handler cleanup. Maintain legacy oracle
  terminate projections as the union of returns and explicit raises.
- [ ] Implement ordered item entry and local exception tracking in `visit_with`.
  Join only possible suppressed exceptions with normal fallthrough; preserve
  pending exceptions for outer flow. Restore the collector flag after the body.
- [ ] Run focused tests to GREEN; extend contract documentation with supported
  suppression and abrupt-exit semantics and unchanged exclusions.
- [ ] Review implementation and tests in three distinct passes each; record
  concrete findings and fixes in `docs/reviews/2026-09-24-issue-556.md`.
- [ ] Run `cargo test --workspace`, `cargo fmt --all -- --check`, and
  `cargo clippy --workspace --all-targets -- -D warnings`. Record any environmental
  or baseline failures explicitly. Commit implementation, tests, and evidence.
