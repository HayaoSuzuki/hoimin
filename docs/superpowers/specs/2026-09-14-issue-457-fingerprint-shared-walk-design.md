# Issue 457: shared fingerprint glob traversal

Issue: https://github.com/tokyogas-tech/hoimin/issues/457

## Decision

`fingerprint_inputs::resolve` compiles each include glob independently and records its original input position. A second override containing the valid positive patterns limits one `WalkBuilder` traversal to their union. Negative patterns remain only in their independent matcher; placing them in the union would let one argument suppress a match required by another. Each selected entry is checked against the independent overrides, so duplicate and overlapping patterns retain separate matched states while the output remains sorted and deduplicated.

Compilation, matching and selected-entry failures are stored per pattern. Resolution examines those outcomes in argument order after the traversal. This preserves the first-error contract even when an earlier unmatched glob precedes a later invalid glob. Exact-file processing retains its current ordered phase after every glob succeeds.

The union filter is required for compatibility. An unrestricted shared walk would inspect unrelated symlinks, non-UTF-8 names and special files that the old per-pattern traversal never selected.

## Verification

Deterministic tests assert one traversal for multiple patterns and compare records and first errors across duplicate, overlapping, escaped, unmatched, invalid and unsupported inputs. Release evidence varies both unrelated directory entries and pattern count; elapsed time is supporting evidence rather than a fixed CI threshold.

## Design self-review

1. Traversal: one positive-union-filtered walk removes the repeated root enumeration while retaining the `ignore` crate's descendant matching behavior.
2. Compatibility: per-pattern outcomes and final input-order replay preserve unmatched/invalid/unsupported precedence; exact files remain later.
3. Boundaries: symlinks are not followed, ignored and hidden files remain eligible, and unrelated unsupported entries remain outside the union.

Independent review found that a union containing `*.toml` and `!a.toml` suppressed `a.toml` globally and changed the first unmatched diagnostic. The revised union excludes negative patterns; a regression observes the historical per-pattern result. Escaped leading negation is already rejected by portable-path validation and remains rejected.
