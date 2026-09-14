# Issue #476: missing symbol definition diagnostics

Explicit `--symbol module:qualname` selectors require an AST function or class definition at the exact dot-delimited qualname. Methods, nested functions/classes, async functions and package `__init__.py` definitions count. Runtime imports, assignments and aliases do not establish definitions. Existing definitions remain valid when operator, profile, line, changed-line or candidate limits produce no candidates.

Validate in `TargetHandler::resolve`, after explicit path resolution and before Git intersection. All run, plan and verify paths use this boundary before baseline. Validate every explicit symbol file even when another file fills a candidate limit, Git has no changed lines, or verify requests candidates from another file. Missing definitions return an exit-2 target error naming the file, qualname and original selector(s) with that qualname. Original selectors can share a qualname across modules; the resolved file identifies the failing occurrence.

A lightweight Ruff AST visitor collects definition qualnames once per selected file. Reuse the analyzer depth guard and iterative disposal on rejected trees; malformed syntax is reported as inability to validate symbols, never as proof of absence. Files without symbol selectors receive no additional parse. This intentionally adds a parse for symbol-selected files before candidate discovery; parser work and target-resolution I/O are outside the existing analyzer discovery timeout. No Python code is executed.

Rejected alternatives: checking emitted candidates confuses empty selections with missing definitions; validating only during candidate discovery misses changed-empty targets and verify's requested-file subset. A shared AST cache would expand lifecycle and memory scope beyond this change.

Verification uses public CLI entry points, baseline marker files, independently specified definition fixtures, clean Git and changed-other-definition controls, edited plan selectors, and existing target/analyzer/plan regression suites. No schema or ranking change is needed.
