# Issue #451: Parenthesized exception removal

## Contract and cause

`exception_exception_to_bare` must remove the complete exception expression, including optional parentheses. A bare handler catches ValueError and BaseException subclasses. Removing only the AST name leaves `except ()`, which catches nothing. Tuple member removal must remove that member's optional parentheses together with one adjacent comma, leaving the other members and their comments intact.

## Design

Use Ruff's token-based `parenthesized_range` with the actual parent node (handler or tuple), falling back to the expression range only when it has no optional parentheses. Keep candidate construction, builtin resolution, final-handler gating, termination-exception filtering and except-star exclusion unchanged. Do not reparse each candidate in production. Parentheses and comments inside the deleted expression are deleted with it; comments outside that expression remain byte-for-byte intact. For two-element tuples, the remaining member may be a grouped exception name, which retains the intended catch behavior.

Rebuilding handlers from AST names would discard spelling and comments. Skipping all parenthesized expressions would avoid the defect but unnecessarily suppress supported mutations. Existing token ranges provide the syntax-preserving operation without a new parser or dependency.

## Validation

Unit regressions cover nested parentheses, multiline comments, leading and trailing tuple members and trailing commas, asserting literal replacement text and reparsing. CLI plan candidates are applied and executed with CPython to check ValueError, TypeError and KeyboardInterrupt behavior. A real CLI run checks that a ValueError test survives the bare-handler mutant. Existing risky-operator tests retain final-handler, bound-name and except-star restrictions.

Lean is not needed here: the defect is in the correspondence between parser token spans and executable Python. A separate abstract proof would not establish that correspondence; actual candidate execution is the independent oracle. Windows execution remains subject to CI.

## Self-review

1. Cause and scope: traced both deletion paths; safe name swaps already preserve grouping and need no change.
2. Syntax and semantics: reparsing alone misses `except ()`; require CPython catch results and run report assertions. Internal comments disappear only with their deleted expression.
3. Compatibility and cost: reuse existing token APIs, no per-candidate parser, no new dependencies; preserve existing operator eligibility and exact unparenthesized replacements.
