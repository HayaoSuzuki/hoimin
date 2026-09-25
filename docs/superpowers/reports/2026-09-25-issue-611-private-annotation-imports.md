# Issue 611 evidence and self-review

Base: `ba96e06` (#605 fix). Author self-reviews below are not independent approvals.

## Design reviews before code

1. Traced both endpoint routes into `annotation_import_stable` and reviewed Python's mangling rule. A private-name-only string filter would incorrectly reject trailing-dunder and underscore-only class positives; chose an inherited class prefix with explicit exceptions.
2. Reviewed reverse spelling and executed CPython private-global/nested-underscore probes. Found rejecting only raw private annotation roots leaves `_C__Alias` imports vulnerable to `__Alias` writes. Added conservative transformed-key write tracking and directive ownership, without claiming full flow normalization.
3. Parent challenged an early idea to reject canonical prefixes whenever a matching class existed anywhere. Agreed that unrelated classes must not taint imports. Restricted transformed-key taint to actual binding writes and their destination scope; preserve compiler context in lexical descendants and reset at nested classes.

## Plan reviews before code

1. Mapped issue acceptance cases and shared consumers. Added separate source/destination, reverse spelling and qualified-root regressions; all operators must traverse the common guard, not an operator-specific patch.
2. Compared the original 36-case safety oracle with the conservative design. Kept runtime permission and key/identity unchanged, added separate static count so clean private aliases may be suppressed without weakening soundness. Observer remains outside analyzed source to avoid invalidating imports.
3. Reviewed #605 integration results and CI executable registry. Included the Python registry from the outset, explicit baseline/public-run checks, finite closed coordinate validation, and bounded Lean commands. Parent owns independent review; preserve artifacts rather than skill cleanup.

## Execution ledger

Pre-flight: Task 2 consumes Task 1 only through public CLI; generated semantic expectations remain independent. No interface conflict.
