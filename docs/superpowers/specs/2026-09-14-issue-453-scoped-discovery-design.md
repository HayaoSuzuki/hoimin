# Issue 453: Scope discovery for exact selectors

## Problem and decision

`discover_explicit` walks every entry below the root and retains every regular file before `resolve_explicit` selects one requested `--file` or `--line`. For exact-only selections, traverse from the root as before but filter entries to the normalized requested files and their ancestor directories. Keeping the root walk preserves parent `.gitignore`, include/exclude override, hidden-file, built-in exclusion, symlink, and traversal-error behavior along the selected paths. It also avoids descending into unrelated subtrees and prevents unrelated files from entering the discovered-file map.

The scoped path applies only when at least one file or line selector is present and both source and symbol selectors are absent. Source selectors enumerate a subtree and symbol selectors require module discovery, so those combinations retain full discovery. Invalid exact paths also fall back inside the filesystem function; `TargetHandler` validates them before discovery in production.

## Diagnostic boundary

Errors on the root and requested ancestor paths remain observable, as do missing, non-Python, ignored, excluded, symlink, case, and portable-path outcomes for requested selectors. An unrelated malformed filename in a pruned subtree, or an unrelated root file whose native spelling cannot be represented as a portable path, is no longer converted and therefore no longer fails an exact-file request. This is an intentional consequence of scoped discovery: an unselected malformed filename must not make `--file calc.py` depend on the rest of the repository. Broad source/symbol discovery continues to diagnose such paths. Tests state both sides of this boundary explicitly.

## Evidence

A test-only observer on the real walk counts filter visits and collected regular-file records. A fixture with one selected file and 1,000 files below one unrelated directory must collect one record and visit only the root-level boundary entries. Tests also cover multiple line paths, ignore/include restoration, excludes, missing and non-Python diagnostics, platform case rules, symlinks, and the malformed-path boundary. An ignored release test records five-sample medians for 2,000, 4,000, and 8,000 unrelated files without a timing threshold.

This change does not optimize source or symbol discovery, candidate ranking, or explicit target lookup.
