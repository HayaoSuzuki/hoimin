# Issue 454: Fixed verify batches

## Selection contract

Add `verify PLAN --top N --offset K`. K is a zero-based count of candidates to skip in the chosen policy's complete deterministic ordering. Strict uses saved ranks. Diverse first forms the existing score-tier/file-round-robin ordering, then slices it: each invocation starts that ordering from the same plan, never reranks the suffix or restarts round-robin at the offset. Adjacent ranges therefore contain disjoint IDs and their union is the retained plan when the ranges cover it.

N remains positive. Offset requires --top, conflicts with explicit IDs and defaults to zero. K at or beyond retained length is rejected before baseline; an empty plan remains an error. A range extending past the end selects the available suffix, matching existing --top clipping. Addition is saturating before clamping to retained length, so usize limits cannot overflow. Truncated plans allow retained ranges and preserve incomplete reporting. Each final batch, not the skipped prefix, must satisfy plan max_mutants. Timeout/resources and fingerprints remain inherited.

## Implementation choice

Keep existing `VerifySelection::Top` source construction intact and add `TopRange`. Both use existing ranked-policy metadata and requested=N. The shared selection helper forms the deterministic prefix through K+N and discards K IDs. Work and temporary IDs are bounded by the retained plan, not the numeric offset. Existing per-mutant candidate IDs in JSON/JSONL identify actual executed/not-run selections; no result or plan schema change is required. Persist the invocation parameters with each batch's reports. Progress remains a comparison of a fixed candidate-ID set; do not combine scores from different batches.

Alternatives: generating reusable batch files adds another manifest/validation format; slicing the saved candidates before diverse selection changes ordering and risks inconsistent batches. A globally ordered range reuses current policy semantics with a small CLI extension.

## Verification

Use 120 independent arithmetic sites with max_mutants=100. Prepare 100+20 for strict/diverse, assert disjointness, union, repeatability and unchanged saved plan/limits. A hand-authored multi-file equal-score table proves global diverse slicing, including tier boundaries. Cover empty plans, exact end, beyond end, numeric overflow, truncated suffix and illegal CLI combinations. Real CLI small batches retain actual IDs in reports and can repeat a batch for progress comparison.
