# Binary diff path parsing design

## Problem

Git's human-readable `Binary files X and Y differ` sentence has no unambiguous
delimiter when a filename itself contains ` and `. Splitting that sentence can
exclude an unrelated Python file while leaving the actual binary target in the
changed set.

## Design

Run a second diff over the identical revision range using `--numstat -z`.
Numstat represents binary counts as `-` and separates paths with NUL bytes, so
spaces and the word `and` have no syntactic meaning. Collect binary paths into
the existing exclusion set before retaining changed text hunks.

For ordinary records, parse `added<TAB>deleted<TAB>path<NUL>`. With `-z`, rename
and copy records use an empty path in that record followed by old and new NUL-
terminated paths; consume both paths for every such record and exclude both
when the counts identify binary content. Reject non-UTF-8 or structurally
malformed numstat output consistently with other Git parsing failures.

The patch parser will no longer interpret the human binary sentence. Existing
text hunk, deletion, untracked-file, and rename behavior remains unchanged.

## Tests

An integration fixture will modify a binary Python path containing ` and `
while also modifying a real `y.py`. The binary path must be absent and the
text file's changed line must remain. Parser tests will cover malformed numstat
records and the NUL-delimited rename form where useful.
