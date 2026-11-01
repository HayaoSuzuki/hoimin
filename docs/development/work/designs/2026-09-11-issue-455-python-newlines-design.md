# Issue #455: Python physical newline indexing

## Contract and cause

Python accepts LF, CRLF and lone CR, including mixtures. Analyzer line selection and candidate validation must use identical physical line starts and count CRLF once. Both currently index only LF. Keep source bytes, spans, hashes and candidate identities unchanged; metadata is derived from the original bytes.

## Design

Expose `python_line_starts(&[u8]) -> Result<Vec<u32>, CandidateValidationError>` from core candidate policy and use it in both CandidateValidationContext and analyzer LineIndex. Each LF ends a line; a CR ends a line only when the next byte is not LF. Retain the existing checked offset builder and its oversized-source-before-scan tests. This uses one linear scan and one u32 entry per line, preserving memory bounds and binary-search location lookup.

Duplicating newline recognition risks repeating this mismatch. Normalizing input would invalidate original spans and hashes. A full source-location abstraction is unnecessary for this change: the shared physical-line index is the common boundary that is wrong.

The analyzer's leading-BOM column convention remains unchanged. Tests include BOM before CR-terminated header lines and Unicode scalar columns after those lines. The separate first-line BOM validator mismatch is tracked by #469 and is not claimed fixed here.

## Compatibility

Candidate identity excludes line and column, so correcting metadata does not itself change an ID for the same file bytes, operator and span. Existing LF/CRLF plans retain their metadata. Old CR plans with erroneous positions fail rediscovery/validation and must be regenerated; do not silently reinterpret saved selection. Historical reports retain their recorded coordinates because report readers do not have original source bytes.

## Verification

Literal tests cover LF, CRLF, CR, mixed newlines, repeated CR, empty lines, Unicode, a leading BOM on a preceding line, and no final newline. Check line selection and core validation independently, including rejection of old metadata. A public CLI run/plan/verify test selects line 2 across newline variants and requires a killed mutant for a strong arithmetic assertion. No Lean model is added: hand-derived byte offsets and executed candidate validation/CLI behavior directly check the two implementations' shared index.

## Design self-review

1. Root cause: both producer and validator use LF-only indexes; changing only the analyzer would turn silent omissions into validation failures.
2. Boundaries: CR before LF contributes no extra boundary, trailing terminators add the next empty line, original bytes stay intact and the size guard runs before scanning.
3. Compatibility/scope: line metadata is outside stable identity; stale CR plans require regeneration. Keep #469's independent first-line BOM issue explicitly separate.
