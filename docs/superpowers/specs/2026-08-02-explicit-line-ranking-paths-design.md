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

## Compatibility and tests

No serialized format or ranking-rule score changes. Unit tests will prove that
dot-prefixed and absolute selected paths receive the boost. Existing tests
continue to cover ordinary normalized paths and deterministic ordering. The
Windows equality helper remains covered by its platform-specific core tests.

