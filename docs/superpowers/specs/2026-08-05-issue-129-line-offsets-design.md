# Issue 129 Analyzer Line Offset Design

## Goal

Remove the analyzer's repeated full-prefix scans for candidate line and column positions while preserving every existing candidate span, line, column, filter, ordering, and cancellation result.

## Current behavior and cost

`analyze_source_cancellable` calls `line_and_column(source, start)` for every token candidate and `type_annotation_candidates` calls it for every annotation replacement. The helper scans `source[..start]` for newlines and Unicode scalar values each time. Candidate-heavy files therefore perform quadratic-like repeated prefix work even though all calls refer to the same immutable source.

## Chosen design

Build one private `LineIndex` per successfully parsed source. It stores byte offsets for the first byte of every line, beginning with `0`, by scanning the source once for `\n`. To resolve an offset, binary-search the last line start that is less than or equal to the offset. The one-based line is the index plus one, and the zero-based column is the Unicode scalar count only between that line start and the requested byte offset.

Construct the index once in `analyze_source_cancellable`, after parsing and before candidate traversal. Pass it by shared reference to `type_annotation_candidates`. Token candidates and annotation candidates use the same `LineIndex` instance.

The analyzer only supplies Ruff token and AST range starts, which are valid UTF-8 boundaries within `source`; `LineIndex` remains private and relies on that invariant just as the current helper does.

## Alternatives considered

### Store a position for every source byte

This gives constant-time lookups but uses memory proportional to file bytes rather than line count, and most stored entries can never be queried because candidate starts are UTF-8 boundaries. The line-start index has a much smaller footprint while eliminating full-prefix rescans.

### Compute positions during one ordered candidate pass

Token ranges are ordered, but type annotations are collected through a separate AST traversal and expanded into multiple replacements. Coupling both candidate pipelines to one mutable cursor would complicate control flow and correctness. A reusable immutable index supports both paths directly.

### Cache prior lookup state

This assumes monotonically increasing offsets and is fragile when candidate production order changes. Binary search has no ordering requirement.

## Correctness and compatibility

- Lines remain one-based.
- Columns remain zero-based counts of Unicode scalar values, not bytes.
- Empty input and offset zero resolve to `(1, 0)`.
- A position immediately after `\n` resolves to the next line at column zero.
- CRLF behavior is unchanged: `\r` contributes one column before `\n`.
- No public API or serialized schema changes.
- Existing cancellation checks remain in place; index construction replaces many repeated scans with one bounded scan between existing cancellation probes.

## Verification

Add focused tests for ASCII, multiline, CRLF, and multi-byte Unicode positions. Add a candidate-level regression containing both token and annotation candidates so both consumers preserve positions. Add an ignored release benchmark before the production change, measure the same candidate-heavy fixture on the baseline harness commit and optimized commit, and report five-run medians without enforcing timing in CI.

