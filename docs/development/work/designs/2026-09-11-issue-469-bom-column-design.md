# Issue 469: Share Python source-column semantics

Issue: https://github.com/tokyogas-tech/hoimin/issues/469

## Contract and root cause

Python source columns are zero-based Unicode code-point counts. A single UTF-8 BOM at file offset zero is excluded from display columns, while all byte offsets, original slices and hashes refer to the unmodified source. A U+FEFF elsewhere, including inside a string or on another line, is an ordinary counted code point for coordinate validation.

The analyzer's `LineIndex::line_and_column` already strips one leading BOM from the first-line prefix. `validate_candidate_with_context` counts that same prefix without stripping it. Thus a valid first-line candidate is rejected at public discovery even though the analyzer's own position test passes. Worker application and verify reuse the core validator, so duplicating another special case at a caller would leave the contract split.

## Design

Put column computation in a shared core helper, used by both the analyzer and common candidate validator. It receives source plus a known line-start and candidate byte offset, uses a checked UTF-8 slice, ignores exactly one leading BOM only when the line starts at file offset zero, and returns a checked u32 code-point count. A suitable interface is `python_source_column(source: &str, line_start: usize, offset: usize) -> Option<u32>`; callers retain their existing line-index and span-validation responsibilities. Invalid slice boundaries or an unrepresentable count return None rather than panic.

The analyzer consumes the helper under its existing parsed-offset invariants; the validator maps failure to LocationMismatch. Neither path strips bytes from the source or changes hashing, stable IDs, replacement spans, or worker writes. Existing first-line/no-BOM and later-line behavior remain the same. Newline normalization is a separate issue (#455) and is not folded into this independent change.

## Verification boundary

First capture core and public discovery/CLI failures on a BOM-first-line boolean candidate. Core tests must accept correct coordinates and reject the previous off-by-one column, preserve exact hash/span/identity, and distinguish only the first BOM from later U+FEFF. Cover ASCII, multibyte text, ordinary first line, comment then second-line candidate, and a U+FEFF inside a string. A checked helper should reject invalid ranges/UTF-8 boundaries without panicking.

Public plan, verify and run must execute the same source through common validation and worker application. Tests should confirm baseline success, a real boolean behavior change, preserved source bytes/hash, and successful candidate execution. Existing analyzer BOM tests remain unchanged. No new Lean model is needed for this deterministic prefix-count rule; literal boundary assertions and actual CPython/CLI execution give direct correspondence.

## Design self-review

1. Contract: traced analyzer and core zero-based columns and identified the single BOM discrepancy; byte indexing remains separate from display indexing.
2. Boundary: checked existing offset-zero and offset-three behavior, nested/later U+FEFF, multibyte prefixes and checked slicing. Removing all BOMs or normalizing source bytes would violate the contract.
3. Scope: share the rule once in core, keep newline handling and existing hash/span/ID validation, and exercise public discovery plus worker paths rather than only LineIndex.
