# Issue #325: Bounded candidate token lookups design

## Problem

Several AST candidate helpers search `Tokens::iter()` from the beginning or
end of the complete module for each candidate. They later filter by the
candidate's byte range, so the result is correct, but the work is proportional
to the whole file every time.

The affected paths are trailing commas in `.get(...)`, single-element list
literals, and exception tuple additions and removals. A module with `n` small
expressions therefore performs `O(n²)` token visits even though every query is
local to one AST node.

## Goals

- Limit every affected token query to the smallest relevant AST range.
- Preserve candidate text, order, filtering, and limits exactly.
- Make the complexity regression test deterministic and independent of wall
  clock speed.
- Give future candidate helpers one clear range-bounded lookup API.
- Preserve cancellation and avoid per-candidate allocation.

## Non-goals

- Changing the complete token scan that produces raw-token candidates once per
  module.
- Replacing Ruff's token storage or binary-search implementation.
- Changing which `.get`, collection, or exception candidates are supported.
- Introducing a benchmark threshold into CI.

## Options considered

### Centralize candidate-local ranges in `AstFacts`

Add an `AstFacts` method that accepts a `TextRange` and returns the borrowed
slice from `Tokens::in_range`. The affected helpers receive `AstFacts` instead
of an unrestricted `Tokens` reference and use that method before applying
their existing trivia and comma filters.

This is the recommended design. It makes the bounded lookup the natural API,
does not allocate, and provides one place for test-only scan accounting.

### Call `Tokens::in_range` directly in every helper

This is a smaller textual change, but each helper still has unrestricted token
access and a future edit can easily restore a full scan. It also leaves no
deterministic way to verify the number of examined tokens. This option is
rejected.

### Cache punctuation facts for every AST node

Precomputing trailing-comma and comma-position maps would also remove repeated
full scans. It adds storage and a second indexing scheme for queries already
served by Ruff's binary-search range lookup. This option is unnecessary.

### Add a wall-clock performance test

Timing a large generated module reproduces the problem, but a fixed duration
threshold varies across operating systems and CI load. Timing remains useful
for manual validation, not as the regression oracle.

## Detailed design

`AstFacts::candidate_tokens_in_range(range)` will call
`Tokens::in_range(range)` and return its borrowed token slice. Under
`cfg(test)`, it also records one lookup and the slice length. The accumulated
statistics are exposed through `AnalyzerOutput` alongside the existing fact
index lookup statistics.

Each query uses an AST-derived range:

- `.get(...)`: `call.arguments.inner_range()`.
- Single-element list comma: from the element end through the list end; only
  comma tokens are accepted, so including the closing bracket is harmless.
- Exception tuple addition and removal: `tuple.range()`.

Existing token-kind, trivia, boundary, and source-text logic remains in place.
Only the input slice changes.

## Complexity

`Tokens::in_range` finds the range boundary by binary search and returns a
slice covering the local node. For `n` compact independent expressions, the
helpers examine `O(n)` tokens in total rather than `O(n²)`. The binary-search
component is `O(n log n)` across all queries and does not scan unrelated token
contents.

## Tests

Add generated-source regressions for all reported paths:

- many `.get(value)` calls without trailing commas;
- many single-element list literals;
- many supported exception tuples that exercise addition and removal.

For compact inputs, assert that candidate behavior is present and that the
reported examined-token count is bounded by a small constant times the lookup
count. Before the fix, test instrumentation records the full stream length for
each affected query and the bound fails by orders of magnitude.

Keep the existing semantic tests for trailing commas, list-to-tuple
replacement, and exception tuple replacement. Run focused mutation testing on
the range boundaries and the new regression.

## Documentation

Document in `docs/development.md` that candidate-local punctuation queries
must use the range-bounded accessor and that its test statistics are a
complexity contract, not user-visible telemetry.
