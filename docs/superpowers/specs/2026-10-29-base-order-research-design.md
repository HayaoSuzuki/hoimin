# Base-order mutation research

Extend PR #1135's fixed-corpus experiment, not the product operator catalog.
The user approved this design on 2026-10-29.

Swap two direct bases. For three or more, shuffle with a recorded seed; if the
shuffle leaves the order unchanged, swap the first two bases. This terminates
with a changed order for distinct bases, without promising uniform sampling.
Keep exactly one candidate per example and preserve historical result files.

Compare useful order-sensitive cooperative `super()` examples with existing
operators. Include order-independent behavior, explicit base calls that bypass
MRO, and an inconsistent-MRO `TypeError` control. A kill reveals a missing
behavioral assertion; it does not establish correct or incorrect `super()` use.
Record seed and before/after bases. Do not import targets during generation.

Alternatives: enumerating permutations conflicts with the one-candidate budget;
retrying until a shuffle changes order has no bounded termination guarantee.
The approved shuffle-with-fallback approach keeps generation bounded and replayable.
