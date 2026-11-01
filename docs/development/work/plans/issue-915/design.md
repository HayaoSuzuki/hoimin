# Issue 915: Git line-unit boundary

Baseline: `54093222abe08c79e8e36e4656ee04b9c6d8dda0`.

[Implementation plan](plan.md) · [Review and verification](review.md)

Git patch ranges count LF-delimited rows. Python physical rows also recognize
bare CR, treating CRLF as one terminator. Thus `b"a\rb\n"` occupies one Git row
and two Python rows. Existing behavior is correct; this change makes its unit
boundary explicit without changing public `LineRange`, CLI, or saved schemas.

## Decision

Keep a private `GitChangedRanges` value between tracked collection and physical
conversion. Tracked collection owns exclusion and normalization and returns that
value. Its consuming conversion reads current bytes, checks eligible path keys,
and returns the existing physical-range map. Unborn-indexed and untracked
collection already produce physical rows and join only after this conversion.
The type has no public constructors, generic unit parameter, or raw-map accessor.

The smaller alternative is branch-local conversion of ordinary maps. It removes
`patch_ranges`, but the tracked producer's return type still cannot distinguish
Git rows from physical rows. The private value costs one type and one consuming
operation, reuses the existing converter, and makes that producer/consumer
boundary explicit. A generic `LineRange<Unit>` would spread a Git implementation
detail into core selectors and persisted structures without a current need.

`DiffPlan::Legacy` remains the current global-rename and unscoped/full-diff path.
It is not an obsolete migration adapter. Git context, deletion gaps, normalization,
binary exclusions, path selection, rename pairing, untracked files, unborn HEAD,
and explicit merge-base resolution retain their existing policies and order.

## Design review passes

1. Unit audit: traced `tracked::collect`, `translate_current_ranges`, and
   `collect_current_worktree_paths`; identified precisely two producer units.
2. Interface comparison: compared branch-local conversion and the private value;
   chose a consuming private boundary without adding public methods or generics.
3. Ordering audit: checked exclusion before normalization and conversion, and
   final normalization after untracked merging; retained both normalizations.
4. Scope audit: inspected `tracked.rs` rename and binary handling; retained
   `DiffPlan::Legacy`, path batching, and error propagation unchanged.
5. Evidence audit: reused byte-index exhaustive and materialized-text property
   oracles; integration tests must assert literal physical rows, not only compare
   two paths through the same resolver. No new Lean model is justified.

Philosophy score: 8.75/10 (7 of 8 observable diagnostics). The boundary has one
purpose, hides conversion mechanics, has a smaller interface than implementation,
keeps callers independent, documents units, and is reviewed for complexity.
The 10–20% design-time allocation diagnostic is unmeasured; reaching 10/10 needs
measured time allocation rather than an assumed percentage.
