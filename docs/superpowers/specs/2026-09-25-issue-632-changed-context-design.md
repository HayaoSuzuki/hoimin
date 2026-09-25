# Issue 632: changed context design

## Contract

Add `--changed-context N` to run and plan. Omission and zero retain the existing changed-line selection. Explicit use requires `--changed`. Support integers 0 through 2147483647; reject larger values rather than passing values that Git can wrap. Store the value as `Selection.changed_context: u32`, with serde default zero for historical manifests. Both raw and deserialized configuration validation enforce the supported bound and require changed selection for nonzero context.

Use Git's `--unified=N` with the existing deterministic diff options. Git computes context in current-file coordinates, clips it at file boundaries, and merges overlapping hunks. A nonempty range [s,e] expands to [max(1,s-N),min(L,e+N)]; a pure deletion +a,0 selects [max(1,a-N+1),min(L,a+N)] when N>0 and nothing at zero. Empty/deleted/binary files remain excluded. Untracked files retain whole-file selection. Intersect expanded ranges with existing explicit lines, files and symbols after resolving changes. Do not expand explicit selectors themselves.

The public ResolveGitChanges effect retains its zero-context API; only the internal scoped resolver receives the additional argument. Plans and reports serialize the normalized selection, and verify restores it. Fingerprints already contain effective resolved target ranges: no redundant fingerprint field or version change is necessary. Every candidate selected through the expanded changed scope receives the existing changed_line ranking boost. Documentation identifies that reason as membership of the selected Git scope, including context, rather than a claim that its bytes changed.

## Validation and formal correspondence

Real temporary Git repositories exercise ordinary modifications, deletion at the beginning/middle/end, empty files, overlapping hunks, untracked files, explicit intersections, default zero, maximum context and malformed CLI/config input. Public plan/verify tests cover operator discovery and configuration persistence. Existing historical golden manifests remain loadable.

Integrate a standalone ChangedContextModel/Proofs/Cases and generator with a committed changed-context.jsonl corpus and a public plan adapter. The Lean model uses natural-number interval definitions independent of Git output and proves bounds, monotonicity and zero-context deletion. Mutation sensitivity checks deletion boundary errors, omitted expansion and incorrect intersection. It verifies selection semantics, not Git's implementation or Python parsing generally.

## Design self-review

1. Contract review: checked issue examples against current TargetHandler intersection and hunk parsing. Keeping expansion before intersection prevents escaping explicit selectors; retained zero default preserves deletion behavior.
2. Boundary review: a local Git probe demonstrated --unified=4294967296 wraps to zero. Restricted the public range to signed 32-bit maximum, avoiding conversion and overflow assumptions across Git platforms. Git handles clipping without another whole-file read.
3. Compatibility review: examined Selection serialization, plan conversion, ranking and effective-target fingerprints. Added serde default, validation of deserialized values, and explicit ranking documentation; kept the public effect request unchanged.
