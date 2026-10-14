# Issue 895 review and verification ledger

## Execution status

Design and plan are recorded before production changes. Focused baseline and implementation checkpoint checks have passed in the isolated issue-895 target with debug zero, incremental disabled, one build job and two test threads. Five actual implementation review passes are recorded below; whole-workspace verification remains pending.

## Review record

Design and plan have five evidence-based passes each in their respective records. Implementation and verification entries will be appended only after their actual reviews.

## Baseline and TDD evidence

- Untouched focused baseline: `cargo test -p hoimin-core --test resume_policy -- --test-threads=2`, 25 passed. Dedicated issue-895 target, dev/test debug zero, incremental disabled, jobs one. A full untouched workspace baseline was not run; do not infer one from this result.
- Behavioral RED: flat schema-5 MutantFinished payload is accepted by the old decoder; the new rejection test failed at `result.is_err()` before production edits (`issue-895-red-wire.log`).
- API RED: MutationOutcome and ReusedMutationStatus imports were unresolved before implementation (`issue-895-red-api.log`).
- Initial GREEN: exit-code classification property, execution evidence wire round trip, reusable output projection, and flat-wire rejection passed.
- Review regression RED: strict MutantFinished rejected the current OutputEvent `kind` envelope. The focused current-wire test failed with unknown field `kind` (`issue-895-red-envelope.log`). The tagged one-kind envelope now consumes that discriminator before closed payload validation. Current event decoding, machine behavior, and formal adapters passed afterward.
- First focused CLI run found stale test-only payload decoding, old golden optional pointers/schema literals, and a progress oracle that treated required-payload parse rejection as infrastructure failure. Corrections preserved structural/summary checks and distinguish malformed input from infrastructure errors. The subsequent six CLI integration targets passed: decision oracle 5, progress input 4, session oracle 5, progress 97, report delivery 36, session handler 35 (1 ignored). Generated persistence/corruption tests and the stricter formal missing-termination premise were added afterward; final focused checkpoint is pending.
- An overbroad no-run compile was stopped after checking its exact own cargo process and compiler target to unblock the focused envelope regression. No other target/process was stopped and no active target was cleaned. This interrupted compile is not verification evidence.

## Implementation review passes

1. Classification authority: compared ProcessFinished ingestion, persistence acknowledgement, and report emission. Found a remaining direct machine classifier call alongside the new outcome. Corrected machine status projection to use `result.outcome.status()`; public core classifier remains the one implementation reused by the outcome and process-output oracle.
2. Closed wire boundary: compared RawValue OutputEvent decoding with strict MutantFinished serde. Found the discriminator would be treated as an unknown payload field. Added and observed a focused regression, then consumed the discriminator through one tagged FinishedEnvelope without changing unrelated event decoders or dropping duplicate-key checks.
3. Persistence round trip: traced insert, lookup, replacement, and contracts-only RawResult paths. Removed nullable-column encoding/decoding and stored status; all paths decode the owned outcome. Parent review found stale local `status` names for JSON payloads; renamed them to outcome_json. Added generated full-u64 output/time evidence persistence checks and separate malformed-row lookup/replacement/preservation tests.
4. Formal boundary: inspected report lifecycle and progress raw-result models rather than filtering impossible old states from correspondence. Replaced report lifecycle payload with typed Outcome and retained lifecycle proofs/cases/sensitivity. Found ProgressInput's legacy missing-termination acceptance theorem; changed required executed termination to reject missing data and regenerated explicit expectations. Session initialization adds retired v4 explicitly, current maps to 5.
5. Consumer/fixture contract: inspected progress/detail/review/human projections, JSON/JSONL schemas, new current goldens, and legacy eras. Status derives from owned outcomes; diagnostics are independent. Corrected direct fixture decoding through the event envelope and stale optional/version assertions. Historical goldens remain untouched. Split required execution-field rejection into individual generated test functions; broadened close-timeout/reuse/diagnostic properties after parent review.

Parent's independent production review found no blocker in classification, SQLite strict decoding, or the narrow envelope decoder. Parent separately reviewed the ProgressInput reinterpretation described below and found no additional issue. WorkerState layout is outside this change and remains unchanged for issue 897.

## Formal observation boundaries

ReportSequence now observes typed outcomes rather than independent status/optional termination pairs. Its corpus explicitly includes Executed, Reused, NotRun, and output-close-timeout cases. Lifecycle sequence/identity/terminal/atomicity claims and their existing proofs remain the observation boundary; no OS behavior theorem is added.

ProgressInput's status variables now represent **reported summary assertions**, not independent mutant statuses. Supplied raw termination/output-state fields project directly into executed outcome wire. Classification derives the actual mutant status, and summary counts reject contradictory assertions. Missing executed termination is rejected at JSON decoding. Summary-only cases use canonical matching typed outcomes; synthetic provenance is covered separately by the typed report lifecycle tests. The missing-termination premise changes intentionally; no historical legacy-acceptance claim is reused as evidence for the current contract.

The session initialization model retains abstract current/future labels and adds retired v4; the Rust adapter maps current to schema 5. Generator checks and sensitivity checks do not prove physical SQLite locking schedules beyond the existing adapter's declared strict/model-only cases.

## Property-test applicability

Generated exit codes exercise all i32 classifications; generated elapsed/output counters exercise full serialized u64 evidence. Wire and SQLite round trips are separate properties. Generated terminations include arbitrary exits and every finite non-exit termination; close timeout always derives error. Reused status generators cover both permitted values, and arbitrary other strings must reject. Arbitrary diagnostic codes explicitly include the output-close timeout code and cannot change outcome status. SQLite property cases are capped at 32 to bound temporary database overhead; core properties retain normal proptest configuration. Lifecycle scheduling remains covered by existing machine/corpus/subprocess tests rather than generated unsupported OS timing guarantees.

## Remaining verification

Final focused checkpoint, whole workspace tests/clippy, contracts checking, Python/docs checks, source catalogs/hashes, and verification review passes are pending. No PR or whole-workspace passing claim for issue 895 yet.

## Focused implementation checkpoint

Final focused checks: 318 passed and 1 ignored across the five core targets, two relevant internal detail tests, and seven CLI integration targets. Core: process output oracle 2, typed report-sequence adapter 5, machine 77, mutation outcome 12, report policy 32. CLI: decision oracle 5, progress input 4, session oracle 5, progress 97, report delivery 36, session handler 38, current-only initialization adapter 3. The initialization target first exposed a stale future-label mapping to numeric 5; changed it to current+1 and reran the complete target green. This is fixture correction, not a product fallback.

Report-sequence, progress-input, and current-only schema models/corpora and sensitivity checks pass; ProgressInputProofs was built explicitly because its generator imports the model/cases without that proof module. Workspace formatting and diff whitespace checks pass. Subsequent lint-only assertion borrow fixes do not change production semantics. All-target Clippy and full Python verification are running; full Rust workspace testing and contracts checking remain pending.

Parent documentation review found old flat null-field wording in the resource-backend paragraph and a present-tense historical schema-4 notice. Corrected both: reused outcomes contain only their conclusive status, and the historical Issue 882 policy explicitly yields to schema 5.
