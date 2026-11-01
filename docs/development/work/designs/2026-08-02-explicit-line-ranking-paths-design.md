# Explicit-line ranking path normalization

## Problem

Target resolution accepts logically equivalent `--line` paths such as
`./pkg/a.py`, an absolute path below the configured root, and (on Windows) a
path whose case differs from the discovered file. Candidate paths are stored
root-relative and normalized, but ranking compares the original selection path
to the candidate path literally. A selected candidate can therefore lose the
`explicit_line` boost and fall outside a `--top` budget.

## Design

Expose the existing target-path normalization and equality operations from
`hoimin-core` as narrowly scoped helpers. Ranking will normalize each selected
line path against `selection.root` and compare it to the already-normalized
candidate path using the same platform equality rule as target resolution.

Invalid or outside-root paths remain target-resolution errors. Ranking keeps
its infallible interface because it runs only after successful target
resolution; an unexpectedly unnormalizable selection simply cannot match.
Line-range checks and ranking scores do not change.

## Versioning and tests

No serialized fields or ranking scores change, but the deterministic ranking
semantics do. New plans therefore use ranking-rule version 2; version 1 plans
must be regenerated rather than translated. Unit tests prove that dot-prefixed
and absolute selected paths receive the boost. Existing tests continue to
cover ordinary normalized paths and deterministic ordering, and a Windows-only
test covers case-insensitive path equality.
