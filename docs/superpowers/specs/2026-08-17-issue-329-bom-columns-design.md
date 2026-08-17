# Issue #329: UTF-8 BOM column reporting design

## Problem

`LineIndex::line_and_column` counts Unicode scalar values between a line start
and the queried byte offset. A UTF-8 byte-order mark (`U+FEFF`) at the start of
the file is therefore counted as column one even though it is an encoding
signature, not visible source text. Every user-facing column on line 1 is
reported one position too far to the right.

The parser accepts the BOM and byte spans are already correct. Changing or
stripping the analyzed source would shift those spans and is unnecessary.

## Goals

- Report a leading file BOM as zero display columns.
- Preserve byte offsets, source slices, parsing, and mutation application.
- Keep the existing zero-based Unicode-scalar column convention.
- Count `U+FEFF` normally when it appears anywhere other than the file start.
- Define safe results at both the file start and immediately after the BOM.

## Non-goals

- Changing columns from Unicode scalar counts to grapheme or terminal-cell
  widths.
- Treating an embedded `U+FEFF` as an encoding signature.
- Normalizing, copying, or otherwise rewriting source text before analysis.
- Supporting offsets that are not UTF-8 character boundaries; analyzer ranges
  already satisfy that invariant.

## Options considered

### Ignore a BOM prefix in the first-line column slice

Keep line starts and all offsets unchanged. After selecting
`source[line_start..offset]`, remove a `U+FEFF` prefix from that slice only on
line 1, then count the remaining characters.

This is the recommended design. It is local to presentation, works for offset
zero and the offset immediately after the BOM, and cannot affect candidate
spans.

### Strip the BOM before analysis

Parsing a separate source string without its first three bytes would require
translating every parser range back to the original source. Without that
translation mutations would target the wrong bytes. This option is rejected.

### Store a shifted first line start

Changing `LineIndex::starts[0]` from zero to three would make line lookup for
offsets before the BOM end awkward and would mix byte lookup boundaries with
display-column policy. This option is rejected.

## Detailed design

`LineIndex::line_and_column` continues to locate a line using the unmodified
byte offset and line-start table. It forms the current prefix slice exactly as
today. For line index zero only, it calls `strip_prefix('\u{feff}')`; if the
prefix contains the complete leading BOM, the returned remainder is used for
the character count. Otherwise the original prefix is counted.

Consequently:

- offset 0 in a BOM file reports `(1, 0)`;
- offset 3, immediately after the UTF-8 BOM, reports `(1, 0)`;
- later line-1 offsets exclude the BOM from the column;
- line 2 and later use the existing calculation, even if their text starts
  with `U+FEFF`;
- all byte spans remain measured against the original source.

## Tests

Add a direct `LineIndex` regression covering the file start, BOM end, a later
line-1 token, and an embedded BOM on line 2. Add an analyzer regression for
the reported source `"\u{feff}x = 1 + 2\n"` that asserts the `+` candidate keeps
byte start 9 and reports zero-based column 6.
