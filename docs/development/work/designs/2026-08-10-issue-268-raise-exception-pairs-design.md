# Issue #268: Curated Exception Pairs in `raise` Statements — Design

Date: 2026-08-10
Status: Approved design
Issue: [#268](https://github.com/tokyogas-tech/hoimin/issues/268)

## Goal

Extend the existing safe `exception_type_pair` mutation to the primary
exception expression of Python `raise` statements. This exposes tests that
accept the wrong public exception type while preserving the conservative
builtin-resolution and curated-pair policy already used for `except` clauses.

## Decision

Reuse `exception_type_pair` rather than adding a raise-specific operator. The
operator describes the semantic mutation—replacing one curated built-in
exception type with another—while the AST context determines whether the name
appears in an `except` clause or a `raise` statement. Reuse keeps selector,
profile, persisted-plan, and report contracts stable.

The supported primary exception shapes are:

- a simple name, such as `raise ValueError`;
- a call whose callee is a simple name, such as `raise ValueError(message)`;
- either form followed by `from cause`, where only the primary name is an
  exception-type candidate.

For a supported shape, the replacement span is exactly the primary exception
name. Constructor arguments, comments, whitespace, parentheses, and the
optional cause remain byte-for-byte unchanged.

## Safety boundary

A candidate is emitted only when both the source name and its curated
destination definitely resolve to Python builtins at the primary name's source
position. The implementation reuses `AstFacts::resolves_builtin_pair`, so the
scope-aware rules for assignments, imports, parameters, comprehensions,
patterns, exception targets, wildcard imports, dynamic namespace operations,
classes, closures, and source ordering remain the single authority.

The collector skips:

- bare re-raise (`raise`);
- qualified names (`raise module.Error`);
- dynamic callees (`raise factory()` or `raise errors[kind]()`);
- locally or ambiguously bound source names;
- candidates whose destination name is locally or ambiguously bound;
- termination and control-flow exceptions.

`SystemExit`, `KeyboardInterrupt`, and `GeneratorExit` cannot be emitted because
they have no entries in the curated pair map. The implementation does not infer
inheritance or inspect arbitrary user-defined exception classes.

## Architecture and traversal

`AstCandidateCollector::visit_stmt` will recognize `Stmt::Raise` before its
normal child traversal and call a focused helper for the primary `exc` field.
The helper extracts either `Expr::Name` or the `Expr::Name` callee of an
`Expr::Call`, validates the builtin pair, and uses the existing one-span
`add_candidate` path with `MutationOperator::ExceptionTypePair`.

After collection, normal AST traversal continues once. This preserves existing
mutations within constructor arguments and cause expressions without treating
those expressions as raised exception types. In particular, `from cause` may
still contain candidates belonging to other selected operators, but it never
receives an `exception_type_pair` candidate merely because it is a cause.

Candidates continue through the existing source ordering, deduplication,
line/symbol selection, focused-profile filter, limit, cancellation, plan, run,
and report paths. There is no new operator ID, selector, profile rule, schema,
or runtime execution behavior.

## Error handling

- Parser errors retain the existing diagnostic behavior and produce no AST
  candidates.
- Unsupported shapes are skipped without a diagnostic because they are valid
  Python outside the conservative mutation contract.
- Candidate tests apply each replacement and reparse the complete source.
- Production traversal does not add a source-wide reparse or runtime import.

## Testing

Analyzer tests will establish:

- every curated pair works in both simple-name and constructor-call forms;
- `KeyError` emits both curated destinations in stable source order;
- exact replacement spans preserve arguments and `from cause`;
- every emitted candidate reparses;
- bare, qualified, dynamic, and non-primary cause expressions are excluded;
- source and destination shadowing follow the existing scope-aware resolver in
  module, function, nested, class, comprehension, and dynamic-binding cases;
- line, symbol, operator, profile, candidate-limit, and cancellation behavior
  remains on the shared pipeline;
- plan and run JSON retain the canonical `exception_type_pair` ID and exact
  original/replacement values.

Documentation contract tests will require the README and development guide to
describe both handler and raise contexts, the supported shapes, and the safety
exclusions.

## Formal-method assessment

No new Lean model is added. This change introduces neither a state transition
system nor a new resolution algorithm: it reuses the scope-aware resolver whose
contract is already implemented and tested, then performs a local AST-shape
projection. Exact parser nodes, byte spans, and production resolution premises
are more directly checked by Rust correspondence and property tests. Adding an
abstract Lean model here would duplicate the predicate without strengthening
the implementation correspondence.

## Alternatives rejected

### Add `exception_raise_type_pair`

This would allow handler and raise mutations to be selected independently, but
would duplicate the curated semantics and expand defaults, selectors, help,
persisted configuration, fingerprints, and reports without a separate safety
tier.

### Mutate arbitrary raised expressions

Rewriting qualified names, factory calls, subscripts, or inferred exception
classes would require runtime name and inheritance knowledge. It would increase
false positives and could cross termination boundaries, so it remains outside
the safe default.

## Non-goals

- Adding or enabling risky exception operators.
- Mutating bare re-raise or the `from` cause as an exception type.
- Resolving qualified imports or user-defined exception inheritance.
- Changing `exception_ops`, runtime defaults, or `--profile focused` policy.
- Changing existing non-exception candidates inside raise expressions.
