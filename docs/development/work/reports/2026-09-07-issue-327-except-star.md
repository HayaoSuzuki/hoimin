# Issue #327: Safe Exception Pairs for `except*`

Date: 2026-09-07

## Implementation

`AstCandidateCollector::visit_stmt` has a dedicated path for Ruff
`StmtTry::is_star`. That path already entered the exception-type traversal
context and visited each handler body, but it bypassed
`collect_exception_handler`. As a result, a simple starred handler such as
`except* ValueError` produced no existing `exception_type_pair` candidate.

The handler collector now separates the safe simple-name pair operation from
the structural risky operations. Ordinary `except` handlers retain the same
combined behavior. Starred handlers call only the safe pair collector before
their existing type and body traversal.

The safe collector continues to require a simple `Expr::Name`, a curated pair,
and definite builtin resolution for both the source and destination names.
It continues through the shared candidate path for operator selection, line
and symbol filters, focused-profile arid suppression, source ordering,
deduplication, and candidate limits. No operator, selector, profile rule, or
serialized contract changed.

## Exception-group boundary

Python specifies that `except*` matches subgroups of the raised exception
group by exception type. Replacing one supported leaf exception type with
another therefore exercises the same bounded type-selection fault as an
ordinary handler without rewriting exception-group structure.

The five `exception_risky` operators remain unavailable for starred handlers,
including the invalid bare starred form and the `Exception`/`BaseException`
and tuple-shape rewrites. Qualified and dynamic handler expressions, tuple
members, shadowed source names, shadowed destination names, and handler target
bindings that shadow a destination remain skipped.

Primary references:

- https://docs.python.org/3/reference/compound_stmts.html#except-star
- https://peps.python.org/pep-0654/#except
- https://peps.python.org/pep-0654/#forbidden-combinations

## Compatibility

Ordinary `except` collection still invokes the safe and explicitly selected
risky collectors in the same order. Handler type expressions still use the
exception-type context, so generic collection mutations do not appear inside
ordinary or starred handler types. Handler bodies, nested ordinary/starred
tries, supported raise expressions, and all non-exception operators retain
their existing traversal.

The public change is limited to new `exception_type_pair` candidates for
simple, unshadowed curated builtin names in `except*` clauses. Existing
operator IDs and default selection make those candidates available in both
full and focused profiles, except where the shared focused-profile arid rule
suppresses them.

## Regression coverage

The Rust analyzer tests cover:

- one exact `except* ValueError` candidate, including byte span, line, symbol,
  replacement source, and parser-backed mutation validation;
- tuple exclusion and ordinary/starred nesting in both directions;
- source-name, destination-name, and `except* ... as ...` target binding
  resolution;
- full and focused profiles, line and symbol filters, focused arid
  suppression, a type-pair-only operator selection, explicit exclusion, and
  bounded candidate truncation;
- explicit selection of every risky exception operator without any risky
  candidate from starred handlers; and
- the pre-existing exact ordinary-handler candidate matrix and parseability
  checks.

## Verification

The regression test was first run against the original analyzer behavior. The
fixture parsed successfully and failed at its behavioral assertion because the
analyzer returned zero safe candidates instead of one. After the collector
change, the same exact test passed.

The following checks completed successfully with
`CARGO_TARGET_DIR=/private/tmp/hoimin-issue-360-target`:

```text
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::except_star -- --nocapture
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_ -- --nocapture
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -- --nocapture
cargo test --workspace --all-features -j 2
cargo clippy --workspace --all-targets --all-features -j 2 -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The focused groups passed 4/4 starred-handler tests and 12/12 exception tests.
The complete analyzer module passed 129 tests with two benchmark tests ignored.
The full workspace run passed every executed unit, integration, oracle, and
documentation test; the main library suite passed 534 tests with nine ignored,
and the duplicated Rust analyzer integration suite passed 153 with two ignored.
Both `except*` Lean oracle suites passed without a corpus or model change.

An external Python 3 execution compiled and ran both a literal
`except* ValueError` program and its `except* TypeError` mutant against an
`ExceptionGroup` containing one exception of each type. Each form handled its
selected subgroup and propagated the other leaf type, confirming that the
name-only replacement remains executable and changes the intended matching
boundary.

An independent semantic review reported no findings after checking the safe
and risky collector split, scope-aware builtin resolution, shared candidate
pipeline, cancellation point, starred traversal, and ordinary/starred nesting.
