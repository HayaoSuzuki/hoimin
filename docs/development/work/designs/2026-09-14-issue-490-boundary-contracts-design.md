# Issue 490 cross-boundary contract registry and replay

## Scope and premises

Stack on #538 for current JSONL ingestion. The first shared semantic fixture is
the existing 282-case Lean ProgressInput corpus, not a new implementation of
expected results. Extend its actual CLI adapter to JSON v2/v3 and JSONL v3.
Missing-baseline JSON projections with mutants cannot be encoded as a valid
JSONL lifecycle: emit an explicit unexecuted projection-premise row rather than
change Lean's expectation or claim strict equivalence. All current 282 cases
satisfy this encoding premise (846 strict observations, zero projection skips).

A checked-in registry connects all six requested boundaries to executable
existing or new tests. A bounded runner executes exact tests, verifies that a
named test actually ran (zero-test success is failure), captures logs and emits
match/mismatch/infrastructure-error/unexecuted rows. Report mode retains every
registry entry, including native unavailable and known preparation-scope gaps.
Evidence labels distinguish internal replay, public handler, real CLI/SQLite
and native backend. Test existence alone never implies execution.

## Contract worksheet

| Boundary and premise | Lean representation | Production settings | Public observation | Evidence source | Mode |
| --- | --- | --- | --- | --- | --- |
| Reader to progress: canonical result fields and coherent summary | ProgressInputModel classify/toProgressReport, ProgressDecision history | JSON v2/v3, JSONL v3, patience 1 | disposition, exit, stalls, comparison count | generated progress-input corpus + real CLI | strict where projection premises hold; report otherwise |
| Selectors to plan/run/verify: fixed source bytes and operators | BoundedCandidateDiscovery prefix, CandidateSpan, ranking | max-candidates 1/full, strict top | independent literal spans/IDs across commands, overflow versus retained partial execution | shared literal fixture | strict real CLI |
| Backend to header/results: backend selected before output | ReportSequence identity/result contract; backend selection is native | best-effort approval, JSON/JSONL | explicit mode and mechanism agree in header/baseline/mutants | existing selected_resource_policy test | strict real CLI on available OS, native hard controls separately reported |
| Completed result to SQLite/resume: only settled results reused | SessionModel completed/reuse | session, resume, best-effort, JSON/JSONL | saved row equality, stable ID, null historical output/termination | same existing resource-policy fixture plus Lean session corpus through SQLite | strict real CLI + real SQLite |
| Output configuration to finalization: protected path alias | Workspace/DiskGuard ownership; filesystem aliasing outside Lean | source/fingerprint/session versus metrics destination | rejection before baseline and exact original bytes | metrics_destinations fixtures | strict real CLI; no new atomic filesystem claim |
| Plan validation to inherited execution: unchanged inputs | Budget and candidate/plan premises; preparation I/O outside analyzer deadline | planned limits/profile, fingerprint, analyzer timeout | pre-baseline errors, inherited limits, stage error precedence | plan tests and new preparation-scope table | strict for actual observations; cancellation of all preparation stages unexecuted |

Read/check/commit remain distinct filesystem operations. This work does not
collapse them into an atomic Lean step or prove TOCTOU absence. Native Linux
hard OOM/process limits and Windows PID/fault evidence remains linked to
#228/#229/#157/#162/#223; unsupported environments are unexecuted.

## Execution and cost

Reuse existing model/proof, sensitivity and freshness jobs before adapter
shape checks, strict registry execution, all-case report and minimal witness.
No new Lean state space is introduced: 282 semantic cases, three encodings,
patience 1 and self-comparison of two inputs. Record actual applicable/skipped
counts and durations; this bounded corpus is not an arbitrary-trace proof.

Dedicated Cargo target, no debug/incremental, jobs=2. The runner bounds each
Cargo test invocation and kills its POSIX process group on timeout; logs go to
an external artifact directory. Reuse existing tests rather than duplicate
resource backends. Report mode consumes captured strict results without rerun.

## Review decisions

1. JSON document projection is weaker than a full lifecycle; never fill an
   absent baseline merely to make JSONL pass.
2. Registry entries either execute exact tests or explicitly say unexecuted;
   hard-native conditions do not become best-effort passes.
3. Analyzer rediscovery timeout is not an end-to-end preparation deadline.
   Add stage-precedence observations and keep unsupported cancellation claims
   visible. Parent approved this bounded scope and evidence classification.
