# Target selector documentation

## Goal

Make the difference between `--source` and `--file` apparent at the first command a user copies, so a focused single-file run does not unexpectedly scan every file below a source root.

## Documentation changes

The README's primary example uses `--file` alone, because it demonstrates a focused one-file run.
The development-guide mutation command also uses `--file` alone for its one target file.

The target-selector section distinguishes the two selector scopes and states immediately after the list that explicit selectors are combined as a union.
It includes a concrete consequence: combining `--source src` with `--file src/calc.py` selects `src/calc.py` and every Python file below `src`.
It recommends supplying only `--file` when a run should target specific files.

## Scope and verification

No CLI behavior changes. Verify that README and development-guide examples contain no accidental `--source` and `--file` combination, while preserving the documented union contract.
