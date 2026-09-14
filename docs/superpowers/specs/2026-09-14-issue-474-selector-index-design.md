# Issue 474: Explicit selector file index

## Problem

`resolve_explicit` stores discovered files in a `BTreeMap<Utf8PathBuf, bool>`, but `require_python` scans that map for every file, line, and symbol-module lookup. With F discovered files and S selectors, lookup performs O(F × S) path comparisons.

## Decision

Build one supplemental `BTreeMap<String, Utf8PathBuf>` of Python files in `resolve_explicit`. Keys use the existing platform-aware `path_equality_key`; values retain the normalized spelling already chosen by the current ordered `available` map. Each explicit lookup derives one equality key and calls `get`, giving O(F log F + S log F) ordered-map work.

The original `available` map remains authoritative for source-root enumeration and target ordering. On Windows, multiple discovered spellings can share one equality key. The supplemental index iterates `available` in its current order, ignores non-Python entries, and keeps the first Python spelling. This matches the current `iter().find` result and prevents a non-Python collision from hiding a Python file.

`resolve_symbol_path` uses the same index for the existing `.py` then `/__init__.py` search. Public types, serialized data, diagnostics, target order, and path spelling do not change.

## Verification

Core regression tests cover file, line, and symbol lookups, missing and non-Python inputs, duplicate normalized spellings, and symbol-module precedence. A test-only operation observer counts equality checks in the real resolver. Doubling both discovered files and explicit selectors must grow measured work by less than the old multiplicative factor while preserving all target values.

Release evidence measures `resolve_explicit` with equal file and selector counts at increasing sizes. The report records compiler, platform, input sizes, medians, and output equality. Timing supports the deterministic regression test and is not a CI threshold.

## Scope

This change does not alter filesystem discovery (#453), range normalization (#475), selector meaning, or Windows Unicode case rules. Native Windows execution remains a separate verification requirement.
