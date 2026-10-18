# Task 7: temporary stop rank index (#997)

## Requirements self-review before implementation

1. Read exact issue #997 and accepted spec; distinguish measured rank-search work from unmeasured end-to-end CLI deadlines.
2. Enumerated initial worker plus queued remainder and repeated worker plus existing stopped as separate states; no fixture combines all three.
3. Preserve request order independent of worker and discovery sequence; retain stable ties and unordered/missing-rank sequence fallback.
4. Preserve Collecting/Ready ownership, duplicate/missing ID handling, synthetic outcomes, late completion rejection and exactly-once counts across cancellation/deadline/disk stop.
5. Limit scope to one temporary borrowed index and focused tests; no persistent rank authority, cloned IDs, CLI knobs, cached linear keys or full-workspace rerun.

## Design self-review before implementation

1. Build HashMap<&str, usize> locally after take_remaining ends its mutable selection borrow; ordered_ids remains the sole rank authority.
2. Keep sort_by_key stable and identical candidate extraction/fallback; use HashMap.get copied rank and sequence conversion unchanged.
3. Index creation visits each requested ID once, sort queries use hash lookup; expected linear indexing/hash work excludes collisions, string-byte hashing, sorting/allocation and Collecting transfer costs.
4. Test-only thread-local counters count actual production requested-ID visits and sort rank queries; no release-state counters, thresholds or duplicated sort harness.
5. Add 2,000/4,000 initial and repeated production transition fixtures; independent literal-order semantics cover disordered worker holdings separately from sorted cost fixtures. Reuse comprehensive existing property/lifecycle tests.

## Progress

Implemented one local `HashMap<&str, usize>` after ordered candidate transfer and before the existing stable sort. It borrows selection IDs and dies at the stop preparation boundary. No permanent field, cloned ID, new dependency or CLI option was added. Candidate variant extraction, summary handling and all drain transitions are unchanged.

Added a focused cfg(test)-only module with thread-local work instrumentation, four count cases, six literal-order stop lifecycle cases and one stable unordered sequence-tie control. Added four exact active operation gates to the existing performance registry.

## Implementation self-review

1. Compared the production diff against the original: only index preparation and rank lookup changed; worker candidate ownership transfer, Persisting summary recording, stable sort, sequence conversion and subsequent draining are identical.
2. Verified borrowing and authority: HashMap keys are borrowed `&str`; order comes only from `selection.ordered_ids()`, after mutable `take_remaining` access ends. The map is local, not retained across repeated stops, and unordered stops allocate no map.
3. Traced each sort candidate variant: NotStarted, SyntheticNotStarted, StartedNotRun, SyntheticFinished and Finished still derive the same ID and sequence. Unknown ranks and unordered runs retain the original checked sequence fallback.
4. Checked test instrumentation isolation: the module and both recording calls are cfg(test); no production state/counter or public testing accessor was added. Thread-local reset scopes cost evidence to the measured stop, excluding setup and the first stop used to arrange repeated cases.
5. Reviewed the growing test module and contracts. Found prepared boundary fixtures omitted the required copy grant and defaulted to one configured worker while two were installed. Set explicit jobs/max_mutants and reserve a real grant through BudgetLedger. The first contracts run failed all eleven tests on that fixture invariant; the corrected run passes all eleven. This was test setup, not a production regression. Existing large machine/tests files were not restructured.

## Tests self-review

1. RED ran the real production `transition(..., CancellationRequested)` with instrumentation at actual position comparisons. Initial N=2,000/4,000 produced 3,999,999/15,999,999 request-ID visits; repeated produced 6,004,995/24,009,995. All four failed the expected N visit assertion. An earlier compile attempt exposed a missing TopPolicy argument in the new fixture, corrected before behavioral RED; no compile error is claimed as optimization evidence.
2. Initial fixtures start with worker+queued remainder and empty stopped queue. Repeated fixtures perform one real stop first, assert the ordered remainder is empty and existing stopped count is N-1, reset counters, then perform the measured stop. They never combine worker+queued+existing stopped in the measured state.
3. Macro-generated semantic cases use literal request d,b,a,c against worker b,d and discovery sequences 2,4 with queued a,c. Each stop cause and each initial/repeated scenario has its own named test; starts/finishes and four NotRun outcomes are literal expectations. Repeated cases reject the retired output ACK and the full drain asserts one summary and cause-specific exit.
4. The unordered control uses a,c,b as the literal expected sequence order, with c,b tied at sequence two. This catches both replacement of request ranks by discovery sequence in ordered cases and accidental unstable sorting/fallback changes. Existing missing-ID, dedup, Collecting stops, synthetic reuse and adversarial generated schedules remain covered by all 110 existing machine tests.
5. Checked cost scope and regression sensitivity: exactly N request-order visits and at most 4*N key queries are asserted only on the declared sorted/rotated shapes, using 64-byte unique IDs with discovery order opposite requested rank. This excludes HashMap hashing/collisions/equality, allocation, generic sorting complexity, Collecting transfer and whole-CLI time/RSS. Removing sorting is caught by the disordered literal-order cases; repeated position scans fail the observed RED gate. No wall-time threshold or standalone imitation harness exists.

## Verification

Every Cargo command and commit hook uses:

```sh
export CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export RUST_TEST_THREADS=2 PREK_CONCURRENCY=1 CARGO_NET_OFFLINE=true
```

- RED: `cargo test -p hoimin-core --lib machine::stop_tests::ordered_stop_rank_work -- --nocapture` — 0 passed, 4 failed, expected N-vs-quadratic visit assertions. `/tmp/hoimin-issues-992-998/task7-red.log`.
- GREEN: `cargo test -p hoimin-core --lib machine::stop_tests -- --nocapture` — 11 passed, 0 failed. `/tmp/hoimin-issues-992-998/task7-green.log`.
- Corrected contracts GREEN: `cargo test -p hoimin-core --features contracts --lib machine::stop_tests -- --nocapture` — 11 passed, 0 failed. `/tmp/hoimin-issues-992-998/task7-green-contracts.log`.
- `cargo test -p hoimin-core --test machine` — 110 passed, 0 failed, including generated scheduling and late completion tests. `/tmp/hoimin-issues-992-998/task7-machine.log`.
- `python3 tools/performance_shapes.py check` — validated 29 shapes, including four exact nonignored named core gates.
- `cargo fmt --all` and `git diff --check` passed. Commit hooks passed: workspace fmt, vendored-parser fmt, workspace/all-targets/all-features clippy with `-D warnings`, and vendored-parser clippy. Log: `/tmp/hoimin-issues-992-998/task7-commit.log`. The initial sandboxed commit could not create the shared worktree index lock; the authorized escalated retry completed with all hooks.

| Scenario | N | Original request-ID visits | Indexed request-ID visits | Sort key queries / lookups |
| --- | ---: | ---: | ---: | ---: |
| Initial worker + queued | 2,000 | 3,999,999 | 2,000 | 3,998 |
| Initial worker + queued | 4,000 | 15,999,999 | 4,000 | 7,998 |
| Repeated worker + stopped | 2,000 | 6,004,995 | 2,000 | 7,996 |
| Repeated worker + stopped | 4,000 | 24,009,995 | 4,000 | 15,996 |

Initial candidates are in requested rank order when merged; repeated candidates are a one-position rotation because existing stopped candidates precede the worker candidate. Counts are revision/toolchain/input-shape observations, not universal sort comparison identities. The optimization removes repeated requested-prefix scans and uses expected constant-time hash lookup; it is not a proof of constant time under arbitrary collisions. No end-to-end shutdown timeout, cleanup duration or wall-clock improvement was measured.

## Files changed

- `crates/hoimin-core/src/machine.rs`
- `crates/hoimin-core/src/machine/stop_tests.rs`
- `docs/performance/shapes.json`
- `docs/maintenance/2026-10-19-issues-992-998/task-7.md`

## Concerns and handoff

No correctness concern remains after focused verification. Full workspace tests/contracts/performance registry dispatch and final documentation reconciliation remain the controller's Task 8 integration scope. No merge, release, push or PR operation was performed by this task implementer.
