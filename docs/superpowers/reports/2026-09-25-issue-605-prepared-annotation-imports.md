# Issue 605 evidence and review

Base: `43989c2`. Worktree: issue-605. These are author self-reviews, not independent approvals.

## Design reviews before code

1. Read `AnnotationImports::resolved_name`, `spelling_for` and `annotation_import_stable`. Found both source and target already converge on the same checker, including attribute roots. Chose one guard there instead of separate operator-specific checks.
2. Compared builtin resolver precedence and #598's runtime nonlocal finding. Found putting the guard after nonlocal or explicit imports would incorrectly certify custom mapping reads. Placed it after lexical class skip/global redirect and before both paths; added these exclusions to policy.
3. Compared the issue's 16 runtime cases with static conservatism. Found exact candidate equality with the original runtime permission would force emission in empty custom mappings. Retained the original runtime formula unchanged and introduced a separate static eligibility projection with an implication proof; both values remain in corpus.

## Plan reviews before code

1. Mapped every acceptance item to tasks. Found the original integer destination is stronger than Shadow-only identity probes for false-kill regression; added that precise baseline/public-run case.
2. Read existing public adapter and guarded CI lists. Found a generator alone would not maintain correspondence. Added model import, executable, freshness/sensitivity, closed-ID validation and exact span/line/symbol checks.
3. Inspected local unevaluated annotation early return and class scope setup. The early return applies only to function-local declarations; preserved it. Added generic method and lexical positives to protect scope propagation, and required independent CPython observations rather than deriving expectations in Rust.

## Execution ledger

Pre-flight: Task 2 consumes Task 1's shared predicate only through public CLI; corpus expectations are independent. No interface conflict. Parent instruction supersedes skill cleanup/reviewer delegation: artifacts stay and root owns independent review.
