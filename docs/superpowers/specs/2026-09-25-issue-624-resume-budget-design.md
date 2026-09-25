# Issue 624: increasing a cumulative resume budget

Permit a compatible incomplete run to retain its run ID when the requested max-mutants is equal to or greater than the last accepted limit. Keep the existing cumulative scheduling rule: reused candidates consume the current invocation's candidate budget, so increasing 1 to 2 reuses the first stable result and executes the second. A decrease cannot reuse that run; a different eligible older run may still be selected. Completed runs remain ineligible. Baseline always runs and only killed/survived results remain reusable.

Remove max-mutants alone from the compatibility digest and bump fingerprint schema 9 to 10. Preserve source/import ordering, all other current compatibility fields, and the existing jobs/max-output relaxation. A fingerprint match is necessary but now also requires monotone budget compatibility.

Persist the latest accepted limit per run in SQLite schema 4. Store a positive fixed-width big-endian u64 blob, preserving the full supported usize range without imposing SQLite's signed-integer ceiling. Migration leaves legacy rows NULL; their old fingerprint schema remains incompatible under the existing policy. BeginSession and LoadSession carry the requested nonzero limit. Under the existing run ownership, an atomic conditional update rechecks fingerprint/incomplete state and stored budget before accepting/reserving the new limit. No ownership bypass or result rewrite is introduced.

RunStarted already records the current normalized limit, so reports show the actual invocation budget. The database retains the last accepted limit rather than a new budget-history table. Once accepted, an increased limit remains recorded even if baseline subsequently fails; the next attempt cannot decrease it. This is an explicit initial policy, not unlimited reuse across arbitrary limit changes.

## Design review passes

1. Existing machine paths increment scheduled_mutants before reuse lookup, in both ordered and streaming scheduling. They already implement cumulative accounting; do not reset counters or count only newly executed mutations. Inspected session load's ownership/recheck boundary and begin transaction before selecting storage placement.
2. Compatibility cannot be relaxed by digest omission alone: persist/check the prior limit and atomically reserve increases after ownership. Preserve completed/incompatible/old-schema and database-error behavior. Use an unsigned, order-preserving representation with schema constraints; do not narrow the CLI's accepted range to i64.
3. Migration and observation: legacy rows remain untouched, old-schema incompatibility remains explicit, and current RunStarted config shows the new value. Same run ID and latest-limit-only persistence are the bounded policy adopted from the issue proposal. Finite Lean cases must model both monotonicity and unchanged stable-status reuse, then be checked against actual SQLite and public execution.
