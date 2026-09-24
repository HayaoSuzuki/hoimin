# Issue 565: recursively exclude disallowed annotation arguments

The shared annotation eligibility gate must reject an annotation whenever a
modeled type-argument descendant is disallowed. Multiple generic arguments are
represented by `Expr::Tuple`; treating that node as clean skips its children and
allows Any, object, TypeVar, strings, Annotated, Callable, Literal, and Protocol.

## Design

Add element-wise recursion for Tuple and List, and recurse through Starred in `contains_disallowed_annotation`, using
`any` to reject the enclosing annotation if any element is disallowed. Preserve
existing recursion through subscripts and unions, alias resolution, and leaf
rules. An all-clean tuple remains eligible. CPython 3.14 evaluates `dict[*(str, Any)]` and
`dict[*[str, Any]]` as ordinary dictionary type arguments, so unpacking cannot
bypass the gate. List-valued arguments also retain recursive descendant checks. This gate applies to nullable add,
nullable remove, and all five collection/iterable operators through
`annotation_replacements`; all seven operators need positive and negative checks.

The change is limited to structural argument traversal. It does not change the
separate #564 question of whether a builtin-spelled annotation name is trusted,
or promise analysis of arbitrary executable expressions as a Python type system.

## Formal and executable correspondence

Reuse the existing NullableGate tree model (atom, one child, pair) and inductive
blocked-descendant theorem. Add explicit left/right/deep rejection and clean-pair
witnesses plus a broken-left-only rule, complementing the existing skip-all-pairs
rule. Keep finite sensitivity enumeration at depths 0, 1, 2, under bounded Lean
execution. Extend the Lean-generated corpus with dictionary key/value positions,
nesting, direct/module aliases, three-argument descendants, and shared operators.

The four existing untrusted-name fixtures remain in the corpus with an explicit
report-only mode for #564. Their expectations remain unchanged. New Rust tests
validate the whole schema and execute every strict row through the public CLI,
comparing exact original/replacement pairs. Do not hide infrastructure failures
as semantic mismatches. CPython 3.14 must compile/evaluate strict source annotations
and compile each generated replacement. Name/provenance and full Python typing
semantics remain outside the structural model.

## Review and acceptance

Before product changes, observe both focused analyzer and public plan regressions
fail. Preserve normal int/list/dict, both clean key/value arguments, nested clean
arguments, and alias controls. Include blocked leaves on both sides and at the
third position so left-only recursion and blanket tuple rejection are detected.
Run model build/sensitivity/corpus freshness, broader Rust tests, CI clippy,
formatting, and targeted OKF validation. Keep historical audit results and #564
status explicit when updating the knowledge catalog.
