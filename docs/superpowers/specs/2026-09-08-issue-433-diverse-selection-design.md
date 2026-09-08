# Issue #433: Active file groups for diverse selection

## Problem

Within an equal-score tier, diverse selection revisits every file queue on every round, even after a queue becomes empty. With S singleton files followed by D candidates in one file, this takes (S + 1) * D queue probes when all candidates are requested.

## Design

Keep the existing first-appearance file grouping and score-tier traversal. Convert the groups to a queue of active groups. Remove the front group, select its front candidate, and append the group only if it still contains candidates. Each selection visits one active group, preserving first-appearance round-robin order, within-file order, score precedence, and the global selection limit. Strict selection is unchanged. Group construction remains O(N); selection becomes O(K) queue operations for K selected candidates. No public API or dependency changes are needed.

## Verification

Retain the existing selection tests and add an uneven distribution case, checking full selection and prefixes through file exhaustion, empty input, and counts beyond available candidates. Measure the existing implementation and replacement with the same release-mode standalone harness using actual ranking and selection code. Count queue visits in temporary instrumented copies to establish the mechanism without shipping test counters in production. Wall-clock measurements are supporting evidence, not flaky test assertions.

The default policy is strict; the default 10,000-candidate cap limits typical impact. Report performance gains specifically for diverse selection on skewed tiers. Source comments are unnecessary for this bounded change.
