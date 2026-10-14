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

## Independent canonical-wire correction

The issue-902 implementer independently reviewed checkpoint 6416d50 and found a new elapsed wire/schema mismatch: standard Serde Duration accepts an array and normalizes nanos >= 1,000,000,000, while the published object schema omitted its seconds upper bound. Parent agreed to a canonical boundary. Separate tests observed both Rust acceptance failures and the full JSON Schema engine's acceptance of u64-max+1. A private map-only executed-elapsed Visitor delegates map fields to a closed derived struct, rejects noncanonical nanos, and retains std::time::Duration and its producer encoding. The schema bounds seconds to u64 max. The same review found the new diagnostic schema closed while SessionDiagnostic deserialization remained open; an independent nested-extra-field test failed before adding deny_unknown_fields.

After correction, 15 mutation outcome tests and 12 Python full-engine schema boundary cases passed. Added separate raw-string duplicate secs/nanos regressions to prove that the map-only visitor preserves duplicate detection without materializing a JSON Map/Value. Those two tests await the active full workspace run at this checkpoint. OutputSpoolRef's pre-existing open Rust decoder versus closed schema is unchanged; do not claim whole-format schema/Serde parity beyond the new boundaries checked here.

All-target lint checking found only mechanical fixture issues: unnecessary assertion borrows, long numeric literals, one unused import, signed fixture casts, an expanded fixture builder, a remaining heap fixture status field, and a missing borrow in run_e2e.rs. Corrected these without lint exemptions; simplified the documented status fixture through the existing test-only typed factory. A further assertion audit found old flat output/termination/time paths could hide reuse mistakes, so projections now decode the actual outcome. Full workspace execution is running; the next checkpoint includes these corrections so dependent issue 897 can rebase. Full verification is not yet claimed.

Full Python suite passed 990 tests with 29 skips before the new duration module was added; its 12 tests passed separately afterward. Repository Ruff lint and type checks pass. Ruff found the new file's final newline, normalized it, and its whole-repository format check now passes. Documentation site build and OKF structure/11 scoped hashes/topic reachability passed before this new source/ledger correction; hashes will be refreshed with this checkpoint and checked again at final verification.

## Verification review passes

1. Baseline/TDD audit: read the baseline and all red/green logs against the test changes. Confirmed the baseline is only the 25-test resume target, not an untouched full workspace run. Confirmed legacy wire, missing typed API, strict-envelope, noncanonical elapsed, nested diagnostic extras, and seconds schema-bound failures occurred for the intended reasons. Kept the failed runs and property-shrunk nanosecond counterexample instead of reporting only successful retries.
2. Focused coverage audit: checked machine, report, session, progress, and oracle target summaries and their source projections. Corrected the session oracle's future numeric label to current+1; its complete target passed afterward. Added a generated SQLite evidence round trip, separated malformed-row concerns, and removed vacuous old flat output/time assertions that could otherwise pass for the wrong outcome variant.
3. Formal correspondence audit: read model/proof/generator and Rust projection changes together. Found ProgressInput still treated absent executed termination as legacy-valid; changed that premise, projected raw fields directly, and regenerated the explicit expectations. Verified report lifecycle and current-only schema proof builds, corpus checks, and sensitivity witnesses. Built ProgressInputProofs explicitly because its generator does not import it. Status in that raw-input model is a summary assertion, not a second mutant status.
4. Schema boundary audit: independent review found duration alias/normalization and seconds-range mismatches plus nested diagnostic openness. Observed separate failures before correcting each new boundary. The canonical map-only visitor retains duplicate detection through MapAccessDeserializer; raw duplicate-field regressions were added rather than relying only on Value round trips. Full JSON Schema engine tests use exact integer comparisons because the older Rust schema-test helper compares numeric bounds as f64. No complete parity claim is made for the pre-existing OutputSpoolRef decoder.
5. Documentation/evidence audit: checked current migration language against report 6/session 5 while retaining historical fixture and audit eras. Parent found old resource-backend null-field wording and the old schema-4 initialization notice. Final readback caught that an exact-string replacement missed the resource sentence; corrected the actual sentence and verified it with rg. Checked 48 knowledge-page structures, 11 scoped source hashes/targets, and existing topic reachability; source hashes were refreshed after the canonical correction. Historical audit targets and unrelated hashes are preserved. Final hash/site/format checks remain to be repeated after final ledger edits.

The full workspace execution, final all-target lint, all-feature compilation, and contract-enabled session execution are in progress. The final verification pass will record their actual outputs and any repairs. These five passes are actual reviews and corrections; their existence does not imply the pending broad checks passed.


## Broad verification corrections

The completed workspace run (`cargo test --workspace --no-fail-fast -- --test-threads=2`) reported exactly two failing targets: result-lifecycle oracle correspondence and the fresh/resumed resource-policy test. Both still queried the removed SQLite `status` column. Their observations now strictly decode `outcome_json` and derive status. The Unix-only first-durable-kill premise received the same correction; historical session-golden extractors deliberately retain their schema-era queries. No production behavior or timing expectation changed. The full run's 156 result summaries contain 2,874 passes, two failures, and 27 ignored results, including nested subprocess summaries; those totals are not a unique-test count. The two complete affected targets are being rerun rather than reporting the original broad run as green.

A final report-corpus parser audit found that the adapter no longer rejected unknown executed termination roles after the typed migration. A separate regression failed before moving the finite-role conversion into the common validation path. The complete six-test report adapter and 17-test mutation outcome target passed, followed by scoped adapter Clippy. This correction is test-only and retains the formal model's declared finite observation boundary.

Default-feature workspace/all-target Clippy passes with warnings denied. All-feature workspace production compilation passes. Contract-enabled session persistence passes 38 tests with one ignored; these checks have different scopes and do not substitute for all-feature/all-target Clippy. That final lint run follows the affected integration rerun.

## Scoped documentation audit

**Score:** 8/10 for the revised mutation-outcome migration/reference passages, not the complete documentation tree. **Shippable:** yes for these passages. **Diagnostic:** 8/10 — failed rows: 1 (the subsection relies on the containing usage page for its audience statement), 5 (the short migration guidance is prose rather than a numbered procedure). These are documentation-structure limitations, not incorrect contract guidance. The surrounding historical audit records are intentionally historical.

**Doc type / reader:** reference and migration guidance for CLI consumers and maintainers; English. **Local style guide:** existing repository usage, compatibility, development, and OKF conventions. **Blocking:** none in the revised passages.

| Location | Rule | Finding and correction | Severity |
| --- | --- | --- | --- |
| Usage / resource backend | V3 present tense; R6 key points | Old nullable result wording described the retired wire; replaced it with Reused's conclusive status, zero elapsed projection, and absent owned execution evidence. | Medium |
| Compatibility / Issue 882 | V7 timeless documentation | The original schema-4 policy looked active; explicitly marked its historical revision and supersession by schema 5. | Medium |
| Usage / mutation result outcomes | P7 colon introduction for code samples | Converted the executed outcome into an introduced multiline JSON sample and stated canonical elapsed bounds separately. | Medium |

The three factual/sample findings were corrected and checked against the typed API, canonical elapsed decoder, SQLite version policy, and published schemas. A future dedicated migration page should add its own audience statement and numbered reader/session migration steps to address diagnostic rows 1 and 5. The existing containing pages supply context for this scoped reference update; no new concept is needed.

## Final scoped design evaluation

Implementation readback confirms the seven measured design rows recorded in the design: purpose, smaller interface, hidden classification, documented promises, design-focused review, one owned classification decision, and explicit consumer boundaries. Current scoped score remains 7/8 (8.75/10 equivalent). Time allocation remains unmeasured, with the action to record design/review/implementation time in a future change. No measured row failed; this is not a repository-wide score or a claim of stronger OS safety.

## Final verification review and result

6. Broad-result audit: inspected every failing workspace target and confirmed exactly two stale current-schema SQL observations, not a classification or persistence failure. Replaced those queries with strict outcome decoding and direct status projection, including the Unix-only durable-kill premise; retained explicitly historical golden queries. Reran both complete affected targets: result-lifecycle oracle 5 passed; run_e2e 64 passed and 1 ignored. Rechecked report-corpus finite-role validation (6 passed), typed outcome properties/boundaries (17 passed), and scoped adapter lint. Then ran all-feature workspace/all-target Clippy with warnings denied: exit 0. This final audit distinguishes the original broad run from its affected green reruns, rather than claiming the original command passed.

Final checked source is the canonical-boundary production checkpoint plus test-observation and documentation corrections. Default-feature workspace/all-target Clippy, all-feature workspace production compilation, and all-feature workspace/all-target Clippy all pass as separate checks. Contract-enabled session persistence passes 38 tests with one ignored. The completed full workspace run's other targets passed; both failed targets passed in full after their test-only repairs. Rust formatting and diff whitespace checks pass after final edits.

Python evidence remains the complete 990-passed/29-skipped suite plus the separately added 12-passed schema module, not a claimed single 1,002-test full run. Ruff lint/format and type checking pass. The documentation site, 48 knowledge-page structures, 11 scoped source hashes/targets/footnotes, three catalog originals, and affected-page reachability pass after final evidence edits. All relevant Lean proof/corpus/sensitivity checks described above pass. No active build was cleaned during verification.

Limits: Windows native checks do not execute Unix-only branches; CI covers other platforms. Ignored/skipped tests retain their declared conditions. Formal guarantees remain bounded by each recorded model and adapter, and the pre-existing OutputSpoolRef boundary was not broadened. The scoped design/documentation diagnostic limitations above remain explicit. Parent performs remote publication and aggregate CI monitoring; this record does not claim an unobserved remote CI result.

## Release-floor correction

Parent verified that GitHub release v0.7.3 was published on 2026-10-14. The earlier unreleased-0.7.0 assumption was therefore false. Under the compatibility policy, this incompatible report/session format raises the floor to 0.8.0. Updated Cargo.toml, pyproject.toml, Cargo.lock, uv.lock, and fuzz/Cargo.lock together, plus the typed-outcome usage/compatibility notice and PR guidance. Historical 0.7.0 notices for earlier changes remain historical. Report readers must update to schema 6; old sessions require a different database path. Plan schema 6 is unchanged by this outcome change, so already-current plans do not require regeneration solely for this change. Version-contract/lock consistency and documentation checks validate this metadata correction; no duplicate behavioral full run is justified.

Release/version verification passed: 51 tests, 10 skips; five-file parsed consistency agrees on all six owned package entries at 0.8.0; workspace and fuzz Cargo metadata pass with --locked; uv lock --check --offline passes (85 packages). Documentation site, scoped OKF hashes/targets/reachability, formatting, and whitespace were rechecked after this correction. Behavioral checks above preceded this metadata-only bump and were not redundantly rerun.

## Scoped dependency-policy follow-up

Parent diagnosed a CI dependency-policy failure after the release-floor bump: deny.toml still restricted the repository-owned hoimin-core and hoimin-cli Elastic-2.0 exceptions to >=0.7.0,<0.8.0. Changed only those two ranges to >=0.8.0,<0.9.0. Parsed TOML and compared the complete normalized policy with its prior value to confirm that global allowances, third-party exceptions, and source policy remain unchanged. Checked both crates inherit the workspace's 0.8.0 version and Elastic-2.0 license, and that the new bounded ranges cover that version. No dependency or license changed.

This is a configuration-only correction. cargo-deny is unavailable in the local environment; the actual dependency-policy checker remains a CI check after publication. No Rust rebuild or duplicate behavioral suite was run. Diff whitespace, scoped source hashes/reachability and documentation site were checked after recording this follow-up.

## Contract-enabled inventory correction

Parent's complete core contract run found an obsolete should-panic test that the earlier all-feature compile/lint checks did not execute. Locally reproduced the exact failure: incoherent_mutant_finish_trips_the_ci_contract did not panic. Its fixture helper still accepted an independent status and optional termination but ignored status for Executed; the intended Survived/Exit(1) contradiction therefore constructed a valid Killed outcome. This was stale test intent, not a missing production guard.

Migrated finished_event_with to accept MutationOutcome directly, and removed its status/optional-termination reconstruction. The close-timeout fixture also supplies its typed outcome directly. Replaced the impossible contradiction assertion with a valid Executed result missing its start event, which exercises the existing lifecycle producer contract. A one-concern AAA property now checks sequence acceptance of typed outcomes in both feature modes, including arbitrary i32 exits and finite resource/cancellation/close-timeout/reused/NotRun cases. Classification cross-properties remain in mutation_outcome; malformed independent-status wire already has separate rejection coverage. No production contract or domain restriction changed.

Parent independently reviewed the typed helper, property and missing-start contract and found no actionable issue. Full `cargo test -p hoimin-core --features contracts --all-targets --no-fail-fast -- --test-threads=2` passes: 30 result summaries, 399 passed, zero failed, two ignored. Default-feature report policy 32, report-sequence raw adapter 6, process-output raw adapter 2, and mutation-outcome 17 also pass. Core all-feature/all-target Clippy passes with warnings denied. These runtime results supplement the earlier workspace lint/compile and contract-session checks; compiling contracts alone was insufficient evidence for their conditional tests.

Verification review pass 7 inspected the contract-only inventory against helper construction and the earlier evidence scopes. The observed failure and correction are preserved in separate logs. Final fmt, diff whitespace, scoped OKF hashes/reachability and documentation site checks pass after recording this test-only follow-up. No additional broad workspace rerun is justified by this fixture migration; CI and the parent execute their final aggregate checks.

## Packaged smoke projection follow-up

The macOS wheel CI job reached a real mutation run but failed in tests/wheel_smoke.py:93 with KeyError on mutant["status"]. The shared wheel/archive assertion still used the retired flat result shape. Primary CI evidence is retained externally at `.superpowers/targets/issue-895-macos-wheel-ci.log` (run 38648876104, job 116143168030). New typed smoke tests observed nine failures against that reader before the correction, including generated Executed/Reused acceptance and flat-wire rejection.

The shared helper now projects only the smoke check's conclusive-outcome predicate: Executed with complete output and integer Exit, or Reused with killed/survived status. All other typed outcomes fail that predicate. It requires the current outcome field and has no flat-status fallback. This remains a private smoke assertion, not a second schema decoder or a general Python status framework. Archive smoke imports the same exercise helper; its mocked current-wire fixtures were also updated.

One-concern AAA tests generate arbitrary i32 exits, both allowed reusable statuses, and close-timeout outcomes across arbitrary exit codes and all finite non-exit terminations. Separate parameterized cases cover complete resource/cancellation outcomes; separate tests cover NotRun, empty mutants, and retired flat status. The smoke-script audit found no other retired flat result reads; the remaining flat fixture is the deliberate rejection regression.

Wheel/archive helper suites pass 64 tests with one skipped. Whole-repository Ruff lint/format and ty type checks pass. Verification review pass 8 traced the packaged smoke call chain and checked generated raw outcome shapes against the published Executed/Reused wire, with close-timeout classification left explicit. These Python-only adapter corrections do not justify a Rust rebuild. A real macOS wheel/archive smoke was not rerun locally; CI must execute the installed-artifact checks after publication. Final source hashes/reachability, documentation site and diff whitespace checks pass after recording this follow-up.

## Fuzz harness boundary follow-up

CI's fuzz build artifact exposed six compile errors in report_sequence.rs: the harness constructed removed MutantFinished fields and mutated bad.status for an obsolete typed contradiction. Primary build evidence is retained externally at `.superpowers/targets/issue-895-fuzz-evidence/_temp/fuzz-report/build.log`. The shipping workspace checks do not compile the separately rooted fuzz workspace, so their earlier success did not cover this harness.

Migrated generated valid lifecycles to owned MutationOutcome inputs: Executed with complete or close-timed-out output, Reused with either conclusive status, and NotRun. Summary status derives from the supplied outcome. The contradictory-domain mutation is replaced by serializing a current finish, inserting a forbidden raw status field, and asserting OutputEvent decoding rejects it. No domain-state repair or invalid typed value is introduced. Parent independent review found the remaining recovery sequence redundant for a decoder rejection; removed it, retaining only that one raw-boundary concern. Existing valid lifecycle checks already exercise the original finish.

Verification review pass 9 audited every fuzz target for old result fields. Remaining baseline elapsed/termination/output fields belong to the unchanged BaselineFinished contract. All seven bins pass `cargo check --manifest-path fuzz/Cargo.toml --bins --locked` on Windows, including libfuzzer-sys compilation, and all seven pass locked Clippy with warnings denied. Fuzz-workspace formatting, shipping-workspace formatting, diff whitespace, scoped source hashes/reachability and documentation site pass after recording this follow-up. This is harness-only; no redundant shipping-workspace behavioral suite was run. These checks compile/lint the harnesses, not a claim of sanitizer-instrumented fuzz execution or corpus coverage; CI must perform its actual fuzz build/run after publication.
