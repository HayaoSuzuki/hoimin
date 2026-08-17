# Issue #324: Annotation token gate design

## Problem

The token scanner excludes annotation tokens only when their source spelling is
`&`, `|`, `<<`, or `>>`. Other AST-approved token mutations, including unary
signs, boolean literals, and arithmetic operators, still become runtime
mutation candidates inside annotations.

For example, the current analyzer emits default-runtime candidates for `-` and
`True` in `Literal[-1]` and `Literal[True]`. Those expressions describe types;
they are not executable behavior that the default runtime operators should
mutate. Hoimin already has separate opt-in `type_*` operators for deliberate
annotation mutation.

## Goals

- Suppress every raw-token candidate whose complete token range is contained
  by a recorded annotation range.
- Preserve the same operators in executable expressions outside annotations.
- Keep opt-in `type_*` annotation candidates unchanged.
- Preserve cancellation, candidate ordering, filtering, and candidate limits.
- Keep annotation containment lookups logarithmic through the existing index.

## Non-goals

- Changing which AST nodes count as annotation positions.
- Expanding or narrowing the `type_*` operator families.
- Evaluating whether an individual annotation happens to execute at runtime.
- Changing AST-based candidate gates, which already exclude annotation spans.

## Options considered

### Gate every AST-approved token before replacement dispatch

After `is_operator_token` accepts a token, query
`contains_annotation_span(range)` and skip the token when contained. This is
the recommended design. It states the policy once and automatically covers new
token operators added to the AST allowlist.

The containment query moves from four spellings to every approved operator
token. The existing annotation index keeps each query at `O(log n)`; when a
file has no annotation facts, the query performs no range comparisons.

### Expand the spelling list

Adding current spellings such as `+`, `-`, `True`, and `False` would fix the
reported examples but would repeat the same omission whenever another token
operator is introduced. The spelling is not the semantic criterion, so this
option is rejected.

### Filter candidates during the final merge

Filtering after candidate construction would require carrying annotation facts
into a later stage, spend work building candidates that will be dropped, and
risk inconsistent behavior across bounded producer prefixes. This option is
rejected.

## Detailed behavior

The token loop will apply gates in this order:

1. Ignore unsupported token kinds and cancellation.
2. Require an AST-recorded operator-token start.
3. Reject the token when its range is contained in an annotation range.
4. Derive the replacement and operator ID.
5. Apply the existing selection, profile, source-order, and limit rules.

The gate applies to parameter, return, variable, and supported type-alias
annotations because all use the shared `AstFacts` annotation index. Executable
expressions outside those ranges keep their current candidates.

## Tests

Add a regression with `Literal[-1]`, `Literal[True]`, and an arithmetic
expression inside an annotation, followed by matching unary-sign,
boolean-literal, and addition expressions in the function body. Assert that
only the body tokens produce those three runtime operators and verify their
source lines.

Update the lookup-stat test to express the new invariant: addition,
multiplication, and bitwise operator tokens all query the annotation index.
Keep the existing bitwise annotation regression as coverage for the previously
supported case.

Run the focused regressions, analyzer suite, workspace tests, Python contract
tests, formatting, clippy, wheel smoke, and focused Rust mutation testing for
the changed gate.

## Documentation

Clarify in `docs/development.md` that the annotation-span exclusion applies to
every AST-approved raw-token operator. No public CLI syntax changes.
