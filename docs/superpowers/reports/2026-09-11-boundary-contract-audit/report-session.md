[監査の要約へ](README.md)

新規Issue: #487 resource metadata。metricsパス衝突: #484。既存reader不整合: #460/#483。

以下は調査時の詳細記録。一時artifactのパスは履歴上の参照であり、再現の最低限の手順は対応するGitHub Issueにも保存している。

# Report / progress / session contract audit

HEAD `623dd808612dbc34775e16814845eec0bc52dff9`, 2026-09-11. Read-only audit of tracked sources; probes only in temporary directories. Scope: process outcome -> machine result -> SQLite -> resume -> JSON/JSONL -> progress. This is a contract/test-matrix audit, not a claim that every possible input was tested.

## Outcome

One new, producer-generated discrepancy confirmed: best-effort runs claim `run.resource_control.mode=hard`, with empty `mechanism`, and reused determinate mutants change from persisted `best_effort` to synthetic `hard`. Full issue-ready draft: `/tmp/hoimin-exhaustive-issue-resource-metadata.md`. Real natural incomplete-run/resume script: `/tmp/hoimin-report-resume-probe.py`. No database or report manipulation is needed.

Existing #460 and #483 cover progress's acceptance of inconsistent outcome data. Further probes show the same reader validation omission includes the newer output-close-timeout fields; fold this into #460 rather than file one issue per field. No evidence found that the actual producer emits these forged status/output-state combinations.

Parent-owned metrics-path issue has supplementary session collision evidence: `--metrics /tmp/.../session.db --session /tmp/.../session.db` exits 0, complete=true, and immediately leaves metrics JSON at the DB pathname. Surviving WAL reconstructs valid SQLite on next connection in this experiment; do not claim permanent database destruction. Selected-source overwrite is the parent's stronger evidence.

## Executed evidence

- `cargo test -p hoimin-core --test report_policy -- --nocapture`: **37 passed**.
- `cargo test -p hoimin-cli --test progress input_ -- --nocapture`: **20 passed**, 42 filtered out.
- Real CLI first/resume experiment: both exit4; first mutant times out and reruns, second is killed and reused. Actual report artifacts: `/var/folders/f0/ldy1m0vn1f1g4w873rm313xr0000gn/T/hoimin-report-resume-1ipg22ct/{first,resumed}.json`; persisted killed mode remains best_effort while resumed event claims hard.
- Sixteen CLI reader probes, eight variants each for schema2 and schema3: `/tmp/hoimin-progress-contract-probe.py`, results `/tmp/hoimin-progress-contract-probes/results.json`.
- Metrics/session collision: `/var/folders/f0/ldy1m0vn1f1g4w873rm313xr0000gn/T/hoimin-metrics-session-q0b_g05s/report.json`. Observed immediate header began `{"schema_version":1,...}`; sqlite3 open restored `SQLite format 3`, integrity_check=ok.

## Boundary matrix: producer and report sequence

“Covered” below means named checked-in test inspected; only the executed subset above was rerun in this audit.

| Contract / boundary | Existing exact coverage | Finding / missing intersection |
| --- | --- | --- |
| Exit(0), nonzero exit, Timeout, OOM, ProcessLimit, Cancelled classify into seven statuses | `crates/hoimin-core/tests/report_policy.rs:245` `process_terminations_are_classified_without_test_runner_assumptions` | Covered core mapping. Actual OOM/process-limit CLI coverage remains existing #157/#228/#229. |
| Score denominator excludes all inconclusive statuses; no candidates gives null score | `report_policy.rs:275`, `:326` | Covered. |
| Exit precedence interrupted > infra error > baseline failure > incomplete > survivor > success | `report_policy.rs:294`, `:303`, `:332`, `:340` | Covered individual policy; progress does not apply it (#483). |
| Terminal run sequence, run-ID agreement, monotonic sequence | `report_policy.rs:357`, `:375`, `:553`, `:582`; `lean_report_sequence_oracle.rs` | Covered producer validator. JSON projection intentionally omits mutant_started and diagnostics, so reader cannot simply validate full lifecycle unchanged. |
| Stable IDs unique across concurrent/sequential executions, one sequence per ID | `report_policy.rs:428`, `:458`, `:485`, `:512`, `:537` | Covered validator; progress has independent duplicate checks. |
| Status matches known termination, failed observe does not advance state | `report_policy.rs:606`, `:644` | Covered; absent termination is explicitly valid for legacy/reused/synthetic results. Do not “fix” by requiring nonnull termination. |
| CloseTimedOut requires error status, known termination, output ref, one valid close-timeout diagnostic | `report_policy.rs:733`, `:742`, `:751`, `:776`, `:789` | Strong core matrix; progress bypasses these checks (#460 extension). |
| run resource policy agrees with actual backend and all executed/reused events | No matching end-to-end assertion found. `run_e2e.rs:1544` only compares fresh mutant parity. | **New confirmed producer bug**: minimal run header and synthetic mutant hard defaults leak. |
| JSON/JSONL exact public event kinds and flushes | `report_policy.rs:820`; `report_handler.rs:1081`, `:1132` | Covered serialization shapes/flush. No backend metadata cross-field assertion. |
| JSON writer partial stdout/spool write poisons writer and prevents reuse | `report_handler.rs:1272`, `:1306` | Covered handler fault paths. Existing #463 addresses unbuffered tiny file writes. |
| Slow owned stdout bounded by total timeout + grace | `run_e2e.rs:36` `stalled_report_consumer_cannot_outlive_total_timeout_and_grace` | Covered #335 repair; borrowed library Write remains deliberately synchronous. |
| Machine complete/count/exit projected from same policy | `machine.rs:385`, `:1210` final report; actual timeout `run_e2e.rs:384` | Producer's policy is centralized. Existing #483 concerns reader forgery, not a proven normal producer inconsistency. |
| Baseline failure produces no mutants, exit3 | `run_e2e.rs:1230` | Covered outcome. Captured baseline logs still inaccessible via CLI (#464). |

## Boundary matrix: persistence and reuse

| Contract / boundary | Existing exact coverage | Finding / missing intersection |
| --- | --- | --- |
| Every mutant commits candidate/result/diagnostics transactionally | `session_handler.rs:749`, `:1434` `each_mutant_transaction_rolls_back_when_commit_fails` | Covered; persisting cancellation preservation already covered by #113 and machine tests. |
| Determinate stored result cannot be overwritten; each inconclusive status may be replaced | `session_handler.rs:1500`, `:1529`; rollback `:1467` | Covered status matrix. |
| Timeout -> rerun -> killed -> reuse -> completed run no longer resumable | `session_handler.rs:841` | Covered handler end-to-end, distinct from actual process execution. |
| All termination shapes survive persisted row encoding | `session_handler.rs:907`; `session/mod.rs:930` unit roundtrip | Covered values include negative exit -7, null, timeout/OOM/process-limit/cancelled. |
| Schema1/2/3 golden semantic migration; all optional columns preserved | `session_handler.rs:73`, `:114`, `:131`; `session/schema.rs:234`, `:350` | Covered golden rows/schema and migration defaults. |
| Invalid termination SQL shape rejected; upgrade rollback/future version | `session/schema.rs:282`, `:380`, `:400`, `:439` | Covered storage kind/code constraints and migration failure. |
| Ownership required for lookup/persist/finish, released after finish/drop/death | `session_handler.rs:1015`, `:1035`, `:1072`, `:1108`, `:1150`, `:1231`, `:1257`, `:1328` | Covered many lifecycle paths; root-local DB/lock workspace collision is existing #472. |
| Contention across read-then-write operations | `session_handler.rs:398` matrix; `session/schema.rs:467`, `:482` | Covered. |
| Newest compatible incomplete run only; previous fingerprint schema rejected | `session_handler.rs:954`, `:991` | Covered. |
| Resource policy/verdict limits/profile/source/test argv affect fingerprint; jobs/max-output do not | `hoimin-core/tests/resume_policy.rs:405`, `:429`, `:436`, `:455`, `:466`, `:477`; generated edits `:762` | Covered fingerprint contract. Supports resource metadata issue: resumed compatible policy is known, not arbitrary. |
| Fresh session/sessionless report termination parity | `run_e2e.rs:1544` | Covered exact mutant parity with volatile fields normalized. Does not cover run header provenance. |
| Reuse imports status only, emits null termination/output and elapsed0 | `session/mod.rs:731`; `machine.rs:1066`, `:1625`; `run_e2e.rs:1439`, `:1484`, `:1544` | Intentional/documented; null termination/output is not a new bug. Mode hardcoding is a new bug. |
| Corrupt status, mismatched candidate/result rows, invalid complete flag rejected | `session_handler.rs:1570` | Covered production lookup. It intentionally does not load or validate historical output/elapsed/termination because only ID/status are reused. No actual producer corruption proven. |
| u64 -> SQLite i64 bound rejects overrange before partial commit | `session/mod.rs:767` checked `i64::try_from`; insert paths `:581`, `:606` inside transaction | Explicit conversion exists; no focused `i64::MAX`/`i64::MAX+1` field/rollback matrix found. Test gap, not realistic CLI overflow reproduced. |
| Full persisted result contracts vs lookup projection | `session/mod.rs:446` full `RawResult` reader is `cfg(feature="contracts")`; runtime `decode_stored_result` returns only status | Important scope limit: roundtrip tests do not establish that public lookup returns historical fields; it does not promise to. |

## Boundary matrix: reader and compatibility

| Contract / boundary | Existing exact coverage | Finding / missing intersection |
| --- | --- | --- |
| Oldest/current schema2 accepted with historic config | `progress.rs:33`, `:41`; version3 original/current report goldens in `report_handler.rs:27`, `:104` | Covered golden eras. Our v2 and v3 original files both usable. |
| Additive normalized config fields opaque; object or null only | `progress.rs:255`, `:287`, `:304` | Covered. Reader deliberately uses Value to decouple from current RunConfig. |
| Additive verification-selection fields ignored | `progress.rs:267` | Covered #344 repair; exhaustive presence/absence/version2/version3 matrix not found. |
| Missing output_state defaults Complete; diagnostics defaults []; empty optionals omitted on serialization | `report_policy.rs:808` | Covered typed serde compatibility. Our missing-optionals v2/v3 probes also accepted by real progress; includes missing termination interpreted None. |
| Version3 requires disk summary; version2 migration does not invent disk claims | `progress.rs:53`; `input.rs:247` legacy path | Covered. |
| Nonmatching event kinds/run-ID/monotonic sequence/schema version rejected | `progress.rs:501`, `:511`, `:530`, `:540`, `:551` | Covered. Header version99 on v2-shaped input currently reports missing disk rather than unsupported-version because v3 parse happens first; rejects safely, diagnostic-order issue only. |
| Summary counts, inconclusive subtotal, score match emitted mutants | `progress.rs:359`, `:424`, `:1315` | Covered #55 repair. Both wire-era score mismatch probes reject exit2. |
| Summary complete/exit coherence | `input.rs:339`, `:414` validators do not enforce; `progress.rs:402` actually accepts a timeout-mutant report with untouched complete=true/exit0 | Existing #483; both era complete=true/exit130 probes accepted and saturated. |
| Status/termination/output-state/diagnostic coherence | `input.rs:339`, `:414` do not call core checks | Existing #460 family. Both eras accept killed+close_timed_out+no diagnostic and Complete+close-timeout diagnostic, all yielding saturated. Forged-reader tests only. |
| Missing baseline, failed baseline, incomplete run unusable (not parse failure) | `progress.rs:452`, `:468`, `:484`, `:1332` | Covered. No need to reject these valid partial reports outright. |
| Candidate IDs must match for eligible comparison; duplicates and content ambiguity excluded | `progress.rs:329`, `:645`, `:722`, `:752`, `:773` | Covered. ID/content distinction and duplicate stable IDs checked separately. |
| Inconclusive statuses/empty common set/gaps interrupt stall adjacency | `progress.rs:793`, `:820`, `:850`, `:879` | Covered all statuses; generated properties at :889, :913, :932. |
| No earlier warnings/stdout before later malformed input discovered | `progress.rs:1113`, `:1209` | Covered streaming-history atomic output behavior. |
| Real run -> report -> progress detects exact killed-to-survived regression | `progress.rs:65` | Covered one ordinary JSON chain. No actual session-resume -> progress resource metadata assertion; metadata not used by current comparison. |
| JSONL generated by run accepted by progress | Not implemented | Existing feature #467, not a newly discovered malformed JSON regression. |
| Raw golden duplicate/unknown fields validated before Value collapse | `report_handler.rs:182`, `:201` | Golden-corpus checks only. Legacy reader converts event to Value, so duplicate field strictness differs from v3 typed path; explicit compatibility policy/test gap, not producer corruption. |

## Input-size and numeric boundary matrix

| Dimension | Current boundary / tests | Remaining useful coverage |
| --- | --- | --- |
| JSON writer mutant count | `report_heap.rs:19`: 32 versus 10,000 mutants, <=64KiB incremental peak and <=512KiB live heap | Covered bounded writer heap; existing #463 isolates syscall cost. |
| Progress history count | `progress_heap.rs:18`: 2 versus16 reports, <=512KiB extra peak; `progress/mod.rs:30` keeps only previous/current full report | Covered #432 history fix. O(history) disposition/comparison output remains necessary. |
| Single report bytes and one huge scalar | `progress/input.rs:181` fs::read whole file; normalized_config Value retained until validation | No explicit cap or memory budget. Our 2MiB ignored future-config payloads accepted v2/v3. This is observation/test gap, not a proven bug without a declared cap. |
| Report parsing recursion | serde_json default depth applies; no local override found | Add depth-boundary acceptance/rejection tests and CLI failure-no-partial-output case; no unbounded-recursion crash reproduced. |
| Summary count overflow / nonfinite score | Serialized counts deserialize u64; summary recomputed from actual mutant list with exact equality | Recomputed counts prevent forged huge summary acceptance. Unit record additions theoretically overflow only beyond realizable vector/run count; no producer issue filed. Boundary rejection tests for numeric wire fields remain useful. |
| Disk delta full u64 endpoint range | `report_policy.rs:56` full i128 difference; :8/:91/:106/:121 missing/unknown/type/arithmetic relationships | Strong exact arithmetic coverage. |
| Aggregate disk evidence size | `report.rs` verified_aggregate rejects empty or >4096-byte observations; `report_policy.rs:146` tests invalid claims | Ensure exact4096/4097 multibyte byte-count boundary is tested when changing it; not expanded into unrelated issue. |
| Event sequence numeric edge | ReportSequence monotonic check uses comparison; no increment in validator | Producer sequence allocation exhaustion is separate machine scope. Missing max/max-1 reader matrix is a test gap. |
| SQLite integer widths | Checked u64->i64 conversion, native i32 exit->i64 preservation | Add fieldwise max/max+1 transactional rollback test if persistence changes. No fabricated producer corruption claim. |

## Coherent follow-up scope

1. File one report-resource-metadata bug for real producer evidence (draft provided).
2. Extend existing #460 acceptance criteria to all output-close-timeout status/termination/diagnostic invariants across both readers, retaining nullable legacy/reused termination.
3. Keep #483 summary complete/count/exit coherence separate from the mutant invariant; existing counts/score tests do not cover it and one fixture currently normalizes its acceptance.
4. Use the table's missing intersections as test acceptance criteria within those fixes. Avoid a separate issue for every absent assertion or malformed JSON variation.
5. Keep #463/#464/#467/#472 and parent's metrics path-ownership bug as their existing coherent work units.
