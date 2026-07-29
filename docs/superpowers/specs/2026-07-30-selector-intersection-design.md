# Selector Intersection Design

## Scope

Fix #62 so an analyzer request containing both changed line ranges and explicit
symbols selects only candidates satisfying both constraints. Requests with
only lines, only symbols, or neither retain their existing behavior.

## Design

Keep `TargetSlice` and `AnalyzeRequest` unchanged. They already preserve both
selector axes. Change the analyzer predicate to evaluate each non-empty axis
independently and combine the results with logical AND:

- an empty line list imposes no line constraint;
- an empty symbol list imposes no symbol constraint;
- a non-empty list must match its corresponding candidate attribute.

This puts the conjunction at the boundary where candidate line and symbol are
both known and applies equally to token and type-annotation mutations.

## Error handling

Malformed symbol selectors remain non-matching. No new error type or CLI
surface is required.

## Testing

Replace the existing union-oriented analyzer test with a fixture containing a
changed candidate inside the selected symbol, an unchanged candidate inside
it, and a changed candidate outside it. Assert only the candidate satisfying
both axes is returned. Existing line-only and symbol-only tests protect their
unchanged semantics.
