# Issue 465: Retired Rust function-discovery disposition

Issue: https://github.com/tokyogas-tech/hoimin/issues/465

## Problem and current applicability

The former focused-mutation Python development tool used `_FUNCTION`, a line-oriented regular expression, to enumerate Rust functions before filtering cargo-mutants inventory. Inspection of the parent of removal commit `2f27e2ae2060b5ca9da423622f0d5aaa2d505d2e` confirms that its generic parameter pattern excludes inner `>` characters, the name pattern excludes raw-identifier prefixes, and the declaration prefix excludes same-line attributes. Nested generics, `Fn() -> bool` bounds, `r#match`, and same-line attributes can therefore disappear from discovery. When the selected set was nonempty, the inventory filter subsequently retained only those path/symbol pairs; an empty selected set retained the full inventory. This review inspected historical source; it did not run the removed tool or cargo-mutants.

The current base `4adf809` includes the removal commit. Both `tools/focused_mutation_support/discovery.py` and `tools/focused_mutation.py` are deleted, along with the supporting package and focused-mutation tests. Neither explicit-file nor changed-file discovery can reach the historical regex or inventory filter. Current `docs/development.md` explicitly prohibits installing or invoking cargo-mutants for repository checks.

## Decision and implementation

Issue 465 is resolved by the prior removal of the complete integration. This PR records the issue-specific investigation in the existing OKF architecture concept, design index and execution plan. It does not recreate the regex, introduce a new Rust parser, or restore an unsupported inventory command.

This Rust-function discovery belonged to a developer tool. The live hoimin analyzer is implemented in Rust but analyzes Python source using Ruff; its AST traversal is a different component. No claim is made here about all Python syntax, all Rust syntax, or parser completeness. A future intentional Rust mutation-testing integration would need a separately reviewed syntax-aware inventory contract, diagnostics for unsupported constructs and bounded source/time/candidate handling.

## Verification on the current base

- Removal-commit ancestry check returned 0.
- Both historical discovery and inventory-filter modules are absent from the checkout and Git tree.
- Active configuration, CI, tests and skills contain no `focused_mutation`, `discover_candidates` or `FUNCTION_PATTERN` references.
- A subprocess importing `RepositorySnapshot` and `discover_candidates`, bounded externally to five seconds, immediately failed with `ModuleNotFoundError: No module named 'tools'`.
- `python3 -m unittest discover -s tests -p test_skills.py -v`: all three current development-skill contract tests passed.

These observations establish removal and current workflow consistency. They do not establish that a modified regex discovers the five original examples; no such modified regex exists. There is no current executable semantics for a new Lean model or mutation campaign to compare.

## Design self-review

1. Root cause: inspected historical regex enumeration and downstream selected-pair filtering, covering both stages of the reported loss.
2. Applicability: verified removal ancestry and both modules' absence, plus the active caller search; the issue is not being dismissed solely because one file is missing locally.
3. Scope: distinguished retired Rust-source discovery from the live Python analyzer and preserved the explicit development policy.
