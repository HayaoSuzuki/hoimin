# Issue 710: line-diverse verification selection

Add opt-in `verify --top N --selection-policy line-diverse`. Keep strict as the
default and diverse as file round-robin. Within each contiguous equal-score tier
of the validated saved-rank order, group by (path, start line), preserve first
appearance of groups and rank order inside each group, then cycle until exhausted.
Finish the higher tier before opening the next. Compute the complete deterministic
order before offset/take; later pages must not restart the rotation.

Reuse the borrowing diversity iterator with a grouping choice. Store candidate
references only, and clone only selected IDs after offset. No candidate suppression,
coverage claims, score recalculation or execution-limit changes. Multi-line spans
belong only to their saved start line. Return existing errors for empty/out-of-range
selection and max-mutants violations. Preview and execution use the same resolver.

Report policy is versioned `line_round_robin_v1`; add it to the public enum,
human rendering and current JSON schemas. Historical schemas remain historical.
Plans need no new version: saved candidates/ranks are unchanged. Old readers that
reject unknown policy strings need updating for new-policy reports; current readers
continue accepting old strict/file reports. Scope remains retained candidates and
partial mutation scores, including truncated-plan diagnostics.

The Google paper's §5.2 evaluates suppression/one-generated-mutant-per-line, not
this deterministic reordering. Compare equal-budget row reach, timing and observed
survivors with strict/file diversity, without claiming extra rows prove improved
fault detection. Keep repeatable authored experiments and identify their limits.

## Design self-reviews

1. Semantics: key includes both path and start line; same line numbers in different files must not collide.
2. Ordering: stable group creation follows ranks, independent of HashMap iteration; score tiers never interleave.
3. Paging: skip occurs on the full iterator; no grouping after offset and no candidate lost when a group drains.
4. Compatibility: strict/file behavior and serialized plans stay unchanged; only new report policy needs reader support.
5. Resources/evidence: borrow bodies, clone selected IDs only; separate line reach from actual test gaps and the paper's suppression results.
