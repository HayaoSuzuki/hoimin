# Issue #441: preserve Python syntax in structural mutations

## Scope and choice

Repair structural source transformations in `analyzer/rust.rs`, retaining the existing operator eligibility, public schema, candidate span convention, and one-line output wherever possible. Do not change killed classification or suppress otherwise supported parenthesized forms. No new dependencies or explanatory source comments are needed.

Use existing Ruff AST ranges and parser tokens to distinguish expression text, enclosing parentheses, and actual delimiters. For transformations that only rename a method, replace the attribute-name range inside the original call range. For transformations that also edit arguments, retain the original callee and call delimiters and replace only the method and required argument contents. Mapping conversions must preserve the receiver's enclosing parentheses and locate the subscript brackets using tokens, not raw text search. Existing `ruff_python_ast::token::parenthesized_range` is available in this file.

Alternatives considered: reparse every generated mutant would detect syntax errors but leave missing candidates and add repeated whole-module parsing; unconditional wrapping and string searches would hide particular examples without preserving syntactic boundaries. Source-preserving edits with AST/token boundaries fix the ownership of delimiters directly.

## Behavioral requirements

`(items.sort)()` must become `(items.reverse)()` and vice versa. Both mapping directions must keep multiline attribute receivers syntactically grouped. A `[` inside a receiver comment must never be interpreted as the subscript opener. Nested parentheses, comments, CRLF, Unicode, and multiline keys must retain valid byte boundaries and the intended receiver/key expression. Cover nearby append/insert/extend call-building helpers, where parenthesized callees can expose the same defect.

Keep supported-shape restrictions (no speculative new receiver or argument types). Preserve selection, candidate limits, deterministic ordering, and operator IDs. Continue using bounded token indexes where available; do not introduce a scan of all module tokens for each candidate.

## Validation

Tests assert concrete original/replacement/operator triples and apply each mutant to its original module before parsing. Cover both directions, ordinary forms, and nearby helpers. A real CLI plan plus external CPython 3.14 syntax/behavior check verifies correspondence beyond the parser used by implementation. A run using only py_compile must no longer kill the repaired multiline mapping mutant for syntax failure. Whole-workspace all-features tests, MSRV 1.88, formatting, and warnings-denied Clippy follow focused tests.

## Design self-review 1 — delimiter ownership

Found that changing only mapping receiver extraction misses `(items.sort)()` and parenthesized append callees. Expanded the design to preserve the full call skeleton while modifying method/argument ranges. Keeping the existing full-call candidate range avoids changing unrelated candidate identity conventions.

## Design self-review 2 — syntax versus behavior

Parseability alone can accidentally alter tuple keys, receiver evaluation, or nested expressions. Added exact replacement expectations, both-direction coverage, and external behavior checks. Comments with brackets must be tested separately from multiline grouping; one does not cover the other.

## Design self-review 3 — cost and compatibility

A generic reparse-and-drop fallback would mask defects and be costly. Rejected it. Require bounded token lookup, unchanged eligibility and normal forms, and no dependency/schema changes. Scope is the affected structural source builders, not a rewrite of the large analyzer.
