# Issue 320: `Vec<u32>` line-start index design

## Scope

Replace the private line-start offset storage in the CLI analyzer and core
candidate validation context from `Vec<usize>` to `Vec<u32>`. The change keeps
the line and column contract unchanged and adds no dependency.

## Representation

`LineIndex::new` converts each byte offset after LF with `u32::try_from`.
Ruff's parsed-source offsets use `u32`, so the CLI source-size contract makes
the conversion exact. `line_and_column` converts the selected `u32` start back
with `usize::from` before slicing the source.

`CandidateValidationContext::new` converts each byte offset after LF with
`u32::try_from`. Candidate spans already store `u32` fields, and validation
converts the selected start with `usize::from` before slicing its borrowed
source. Construction keeps its current infallible API; an offset larger than
`u32::MAX` exceeds the candidate span format and indicates a violated internal
source-size invariant.

## Preserved behavior

Both indexes retain a zero start, add one start after each LF, select the last
start at or before a valid offset, report one-based lines, and count Unicode
scalars for columns. Empty files, final newlines, LF, CRLF, and multibyte UTF-8
positions retain their current results. Callers continue to supply in-bounds,
UTF-8-boundary offsets.

## Verification

Focused CLI tests cover direct line/column lookup and candidate positions.
Core candidate-policy tests keep validation location behavior covered. Run
formatting, focused suites, workspace tests, all-feature workspace tests, and
Clippy with warnings denied.
