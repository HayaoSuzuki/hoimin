# Issue 626: bounded latest-pair transition details

The optional progress detail view identifies actual improved/regressed candidates without changing default output, score, eligibility, stalls, saturation, or exit status. This is a bounded extension of the existing reader/comparator/renderer. User authorization covers design through PR; no additional approval gate is required.

## Interface and scope

Add `--details` and `--details-limit N` (default 100; nonnegative, including zero; an explicit limit requires details). Human output appends detail lines. JSON with details explicitly selects schema version 2, documented in a separate closed `progress-result-v2.schema.json`; JSON without details stays byte-for-byte schema v1. No new output files or streaming side effects are introduced.

Show only the final adjacent input pair, with zero-based `previous_input`/`current_input` indexing the original `inputs` array. Earlier comparisons are never substituted. An unusable final input yields `available:false`, null eligibility and no transitions. Each usable pair exposes matching/different/duplicate candidate-set eligibility alongside existing overall state and aggregate diagnostics.

Details are a subset of the comparator's actual counted survived→killed or killed→survived transitions: both IDs must be equal and unique in both reports, and existing ambiguous content joins and inconclusive statuses remain excluded. Content-only matches with different IDs are not identified as a candidate transition. `unidentified` counts aggregate changes that cannot safely be identified; this is separate from `omitted`, which counts eligible details beyond the display cap. Thus an indeterminate set mismatch or duplicate set remains visible, with no invented identity.

Order identified transitions lexicographically by candidate ID, independent of HashMap iteration or report event order. Keep the smallest N identities with borrowed references in a bounded ordered map, count all eligible transitions, then copy only ID/path/operator and before/current positions/status into final metadata. Include current `path/line/column` plus previous position because IDs in legacy reports can survive metadata movement. Columns retain the report's zero-based convention. No original/replacement/output text is copied. Human strings use debug quoting to preserve one-line records for control characters.

## Alternatives and decision

An all-history list would require a global cap/streaming contract and extra retained metadata; latest-pair scope provides the requested debugging entry point with a fixed bound. A separate JSON output file would create overwrite/error handling obligations; explicit schema v2 on the existing output stream gives a clear opt-in contract. Default v1 is unchanged.

## Semantic and verification boundary

Extend the existing progress decision Lean oracle with identification/filtering and latest-pair selection, generate expectations into its existing corpus, and adapt the real public CLI to compare detail IDs/classifications/omissions against generated values. Existing decision expectations continue to govern all counts and states. Model excludes file parsing, formatting, full Unicode ordering and allocator behavior; Rust tests cover those. No new exhaustive domain: retain existing 24 fixed cases and existing bounds, plus small named cases if needed. Request the global Lean slot; guard every Lean invocation at 20 seconds/2 GiB, never run concurrently.

## Design review passes

1. Identity review: original proposal listed all content-key changes; this would falsely identify different IDs. Restricted detail collection to actual comparator transitions with equal IDs unique in both inputs, recording `unidentified` separately.
2. History/schema review: a last stored comparison can precede an unusable trailing report. Select only final input adjacency, explicitly unavailable on a barrier, and keep schema v1 untouched through an opt-in v2 schema.
3. Resource/presentation review: sorting every change would allocate O(changes), and raw paths could break records. Retain at most N borrowed entries in ID order, serialize only short metadata after comparison, quote human strings, and document previous/current positions and zero-cap behavior.
