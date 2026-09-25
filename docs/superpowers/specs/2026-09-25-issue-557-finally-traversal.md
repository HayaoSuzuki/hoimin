# Issue #557: avoid redundant finally annotation traversal

Baseline: 65865ea. Scope: annotation analysis in `apply_finally`.

A finally suite currently runs once with the intersection of incoming exit states for annotation recording, then once per incoming exit category for transfer. Transfer disables recording, but nested finally suites still run the first traversal. With one normal entry at every level this doubles the leaf visits per depth.

Run the merged-entry annotation traversal only while `record_annotations` is true. Keep the existing per-entry routing, including implicit exceptions and finally overrides, unchanged. The empty-entry recording path remains available to collect syntactically present annotations; transfer-only empty entry returns empty exits without visiting the suite. No AST cache or cross-entry transfer reuse is introduced.

The recording traversal still joins fallthrough, break, continue, return, explicit raise and implicit exception states. Recorded annotations and callbacks retain their order and environments. Separate per-entry transfers preserve the pending exit category when finally falls through and replace it when finally exits abruptly. Mutable class fallback is checked with old/new differential fixtures containing class globals and method definitions; it must not become an unexamined cache key.

Alternatives rejected: unconditional caching would need import, class fallback, scope and observation state in its key; reusing the recording result for several exit categories would lose entry-specific facts. Guarding the redundant traversal addresses the observed recurrence without either complication.

Reuse the test-only counters at actual statement-flow and AnnAssign visits. For a module with one import and depth d nested `try: pass / finally`, require annotation visits <= d+1 and statement visits <= (d+1)^2+1. Class and function wrappers add one statement. These are fixture bounds, not a general complexity theorem for arbitrary multi-exit programs. The annotation-disabled direct-transfer test requires a single leaf visit and zero recorded annotations. A test-only switch restores the old walk for sensitivity and semantic differential checks; it is thread-local and restored with Drop.

Compare full candidate descriptors/order and callback/import/exit observations against the old path. Existing Lean corpus adapters remain semantic oracles; no new universal Rust proof or memory improvement is claimed. Benchmark the release CLI at depths 16 through 20 with three samples, checking Python syntax, one candidate, and baseline descriptor equality. Timings are observations, not CI thresholds.
