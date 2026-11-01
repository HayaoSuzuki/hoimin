# Changed-target range composition Lean audit design

## Decision

Separate the audit into a semantic Lean layer and an observational Rust layer.
Lean consumes parsed change facts; it does not parse unified diffs or reproduce
Git rename detection. Rust owns Git invocation, byte parsing, canonical range
materialization, and public `plan` observation.

The semantic unit is a candidate observation `(path, one-based line, symbol)`.
A parsed change fact has a kind, source and destination paths, destination line
ranges, and an optional current-file line count. Deleted and binary facts form
an exclusion set. Modified and added facts use the destination ranges, renamed
facts use only the destination path, and untracked facts select lines `1..count`.

Range normalization is specified extensionally as the set of positive lines in
valid inclusive ranges. This makes the durable contract independent of one
particular sorting/merging algorithm. Rust correspondence additionally requires
the concrete range list to be sorted, deduplicated, and merged across overlap or
adjacency.

Explicit selectors are an optional path-indexed union of whole-file, line, and
symbol restrictions. Combining `--changed` with explicit selection is
intersection: path, line, and symbol predicates must all hold. An empty explicit
selection leaves changed selection unchanged.

## Proof boundary

Lean proves normalization idempotence and membership preservation, line-set
intersection commutativity and subset properties, no path creation, deletion and
binary dominance, destination-side rename attribution, and exact untracked
unterminated-line bounds. Small bounded cases exercise every broken variant but
are reported separately from proofs.

Rust strict cases create real repositories and inspect public plan manifests.
They independently query Git facts where needed and compare complete candidate
path, span, line, and stable ID. Internal parser cases stay explicitly separate;
Git/setup failures are infrastructure errors, never semantic mismatches.

## Repair policy

If a strict or owned internal observation disagrees under the same premises,
retain the minimized witness as a failing Rust regression before changing
production code. Repair the narrowest owning layer and keep the Lean contract
unchanged.
