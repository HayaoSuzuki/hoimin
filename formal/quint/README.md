# Quint / TLC state-machine verification

This follows the finite model and negative control in `pytest-allpair/formal/quint`.
Quint describes TLA state machines; TLC enumerates their reachable finite states.
Apalache 0.62.4 translates Quint to TLA+, while TLC performs the check. These
checks complement the existing Lean and Creusot proofs.

## Run

Requirements: Node 22, Java 21, Python 3.14+, and the repository's Rust toolchain.
The npm lock pins Quint to 0.33.1. The first TLC check downloads Apalache unless
it is installed under `QUINT_HOME` (default: `~/.quint`). CI verifies the archive's
SHA-256 and installs it before starting the model-checking time budget.

```sh
npm ci --prefix formal/quint --ignore-scripts --no-audit --no-fund
python tools/verify_quint.py
cargo test --locked -p hoimin-core --test quint_budget --test budget_model \
  --test quint_itf --test quint_report --test report_model
cargo test --locked -p hoimin-core --lib machine::quint_completion::
```

After model changes, regenerate expected histories with
`python tools/verify_quint.py --update`. The local editing shortcut
`--simulate-only --update` does not require Java and does not run TLC.
Ordinary `cargo test` uses committed fixtures without requiring Node or Java.
The reader removes fully qualified variable namespaces, rejects name collisions,
and interprets ITF integer/set encoding. It does not calculate expected policies.

`tools/verify_quint.py` owns the model registry, tool invocation and freshness
checks. Each model generates four random histories (seed 1172, 24 transitions,
20 samples). Named scenario actions normally generate 12-transition histories (24 for active
worker schedules):
`Report.interleavedStep` starts two mutants, rejects premature finish, and finishes them in reverse order;
`Completion.gapStep` completes four requests in reverse order;
`Completion.wrongKindStep` attempts a wrong kind before a generic failure;
`Completion.cleanupStep` retries a cleanup while a completion gap is open.
These schedules constrain inputs and reuse the same model transitions to compute
expectations. They avoid depending on chance to cover these paths.

Logs and resource statistics go to `_build/`. The POSIX process-tree guard limits
each command to 20 seconds and 2048 MiB RSS. Java heap is 1 GiB; TLC uses one worker.
The models run sequentially. Timeout, memory exhaustion, syntax errors and missing
Java never count as successful negative-control detection. Freshness checks
preserve expected states and remove only generator metadata.

The three disk configurations use `verifyStep` for TLC and `step` for ITF
generation. Both call the same admission and lifecycle transition; `verifyStep`
keeps only `op`, `arg`, `root`, and `accepted` at their initial values. These four
fields record the preceding input and its acceptance for replay; neither the
transition nor `Safety` reads them. Thus TLC still explores every lifecycle
transition, including rejected inputs, without multiplying states by input history.
The negative disk control uses the same projection. Bounds and resource limits
are unchanged; committed ITF files retain their complete input records.

## Models and implementation correspondence

`DiskObservation` adds three-input histories of threshold observations,
measurement failures, and dispatch requests. Its six measurement representatives
include both exact thresholds, simultaneous limits, a changed duration, and changed
byte values. The state retains complete primary evidence and ordered, deduplicated
secondary evidence. `priorityStep`, `duplicateStep`, and `failureStep` exercise
these paths deliberately. `DiskObservationReplace` must produce a semantic
counterexample when a later failure replaces the first primary reason.

The isolated Connect workspace replays this model against public `DiskLifecycle`
calls with state comparison after every step. Ordinary Rust tests replay seven
fixed ITF histories. Seed PBT generates live histories; byte-history PBT and the
`disk_observation` fuzz target extend to full-width byte counters, measurement
durations and error identities, with at most 128 commands. The fuzz target uses
25-byte records and shares its driver with both replay harnesses. It checks first
failure preservation, secondary uniqueness, and stopped dispatch admission.
These are simulated observations; no test fills the real filesystem.

See the [design](../../docs/development/work/quint-live-lifecycles/design.md),
[plan](../../docs/development/work/quint-live-lifecycles/plan.md), and
[review evidence](../../docs/development/work/quint-live-lifecycles/review.md).

`BudgetConnect` supplies typed live commands to the same `BudgetModel` transitions
used by `BudgetExtended` (four successful reservations, capacity two per kind).
It generates at most eight commands. `reuseStep` covers a rejected reservation,
release, duplicate release, unknown ID, zero-sized reservation and allocation
after rejection; `zeroStep` fills the workload with zero-sized reservations and
releases them out of order. The existing TLC checks establish the underlying
finite model's invariants; Connect also checks `Safety` on generated histories.
The command wrapper is not a separate exhaustive Rust proof.

The Connect state compares real returned IDs, reservations visible through public
ID queries, inferred released IDs, totals for all three budget kinds, and actual
result codes. IDs 0–4 and all returned IDs are queried; no private ledger fields
are exposed. A negative control reports a successful release without calling
the real API and must fail the framework's state comparison. Seed PBT adds live
histories alongside the existing fixed ITF, independent journal PBT, and
`budget_ledger` fuzz target. Ordinary tests remain offline.

| Model | Finite domain | Rust correspondence | Negative control |
| --- | --- | --- | --- |
| Budget | Capacity 2 per kind, 3 kinds, amounts 0–3, 2 successful reservations | Public `BudgetLedger::reserve/release`, returned IDs, contents and totals | Overdraft |
| BudgetExtended | Same capacities, 4 successful reservations, release IDs 0–4 | Same public adapter, covering release followed by further allocation | Shared overdraft control |
| Report | 2 run IDs, 2 mutant IDs, 2 mutant identity sequences, event numbers 0–5 | Public `ReportSequence::validate/observe` errors with their exact fields | Finish while mutants remain active |
| Completion | 4 issued requests, 3 completion kinds, generic failure, attempted IDs 0–5; at most one cleanup with two reserved IDs | Internal fixture of production `RunState::allocate_id/register/accept_completion/retire_pending`; pending requests and their reservation lists, next ID, retired, compressed floor/gaps and outcomes | Accept a retired completion again |

Report covers run start/finish, mutant start/finish and a diagnostic as the
representative ordinary report event. Its model-derived replay compares public
validation outcomes, including the stored run ID, the previous event number, the
active mutant count, and the stored mutant sequence. `gappedStep` accepts gaps,
rejects an older event, and reports the original run ID for an event sent for a
different run after completion. `interleavedStep` checks that two active mutants
take priority over a repeated event sequence and exercises identity rejections
before and after a mutant finishes. The independent accepted-event
journal derives errors from the history and probes subsequent validation to
detect hidden state changes. Default builds also exercise rejected `observe` calls.
With contracts enabled, invalid consumer input goes through `validate`, because
`observe` intentionally asserts on invalid producer events.

Completion replay compares the internal ledger directly. `cleanupStep` delivers
a later completion first, rejects duplicate, missing and unexpected reservation
IDs while retaining the earlier cleanup request, then accepts the same IDs in
reverse order. The accepted retry closes the gap. The independent PBT oracle
derives the prefix from an uncompressed journal of pending, completed and
retired slots. It generates reservation lists with up to four entries, including
empty lists and full-width IDs. The fixture covers request registration and
acknowledgement in the ledger. Public phase transitions and actual resource
release have separate tests.

The reservation/request counts bound workloads; they do not replace production
u64 ID exhaustion with small-number exhaustion. Unbounded histories, fairness,
liveness and OS process effects are outside these models. TLC success alone does
not establish Rust correspondence; ITF replay remains a separate required check.

## Cleanup and ownership extension

The cleanup and ownership extension adds five finite configurations to the four above:

| Model | Finite scope | Implementation correspondence |
| --- | --- | --- |
| DiskLifecycle | Both owned roots, up to one dispatch, all drain/report success or failure and cleanup outcomes | Public `DiskLifecycle::apply/snapshot/may_finish/safety_succeeded` |
| DiskExecutionOnly | Execution root only; failures may retain it while still reaching finish | Same public adapter |
| DiskNoRoots | No owned roots; attempted cleanup of unowned roots is rejected | Same public adapter |
| Finalization | Inner/outer drains, joined/failed/unjoined measurement, cleanup admission/results, one-shot free-space query for two filesystem keys | CLI `ExecutionFinalization` methods, per-key free-space lookup and summary projection, internal fixture |
| Ownership | Two actors/two run IDs and independent file handles | Real `RunOwnership::acquire` and handle drop using native OS locks |

The disk model initially allowed two dispatches but hit the outer 20-second limit;
that run is not counted as success. The finite workload uses one dispatch, while
PBT/fuzz checks longer histories and arbitrary u64 observation values. The public
adapter compares admission, the complete primary failure, and ordered diagnostics
against generated expectations. `processEvidenceStep` keeps a measurement failure
primary when the process drain fails and delivery cleanup is deferred.
`cleanupDiagnosticStep` records identical failures from two roots once and then
records a measurement stop. `cleanStep` submits a late measurement failure after
finish and checks that the terminal snapshot is preserved. PBT extends these checks
to longer histories and verifies cleanup and finish prerequisites.

Finalization checks that an unjoined measurement cannot be repaired by a later
joined success, a refused removal attempt remains terminal, failed joined readings
preserve prior evidence, and failed free-space queries do not retry or become zero.
The free-space model distinguishes a failed query, a successful empty query, and
readings indexed by filesystem key. `pairedAvailableStep` retains two distinct
readings after a second query request. `missingAvailableStep` retains a zero
reading for one key and absence for the other, even when a later request would
supply the missing key. Rust checks both keys and an unknown key after every step.
PBT varies key presence and full-width byte counts independently. The query
callback supplies synthetic readings and counts invocations.

Ownership PBT expands to four actors/three run IDs, and each step probes the real
OS lock with another independent open. I/O errors are infrastructure failures, not
successful detection of contention. Handles close before temporary directory
removal. These tests exercise descriptors on the current host; they do not prove
SQLite recovery, process-crash handling or all possible host failures.

The corpus through the cleanup and ownership extension contains 54 histories, including `cleanStep`/`deferredStep`
for disk, `cleanStep`/`refusedStep`/`unjoinedStep`/`availableStep` for finalization,
and `reacquireStep` for ownership. Six negative-control models check detection of
unsafe transitions. Every generated history participates in freshness checking.

```sh
cargo test --locked -p hoimin-core --test quint_disk --test disk_model
cargo test --locked -p hoimin-cli --lib shell::finalization::quint
cargo test --locked -p hoimin-cli --lib session::ownership::quint
```

## Startup janitor reclaim evidence

`janitor_reclaim.qnt` models two selected roots from three matching directory
entry observations. Each transition represents one call to
`ReclaimLedger::{begin, hold, absent, removal}` in
`crates/hoimin-cli/src/workspace/owned/janitor.rs`. It follows inspection,
preservation, absence, and deletion acknowledgement. The scheduled histories
distinguish a failed deletion with partial progress from an acknowledged deletion
that also observes absence. Acknowledgement takes precedence over both absence
and an earlier failure classification. The summary checks the outcome partition,
acknowledged entry counts, and selection omissions.

The Rust adapter replays each bounded prefix through the real ledger and checks
its finished summary. A negative control drops the root deletion acknowledgement
and checks that the same history reports a state mismatch. Measurements and
filesystem results are injected as observations. The model covers the report
ledger boundary. The janitor's traversal, OS locks, and deletion effects have
their own tests. Update this model and regenerate its ITF fixtures when the
observable classification or summary contract changes.

The registry runs these janitor checks with Quint simulation and checks the ITF
fixtures for freshness. `acknowledgementLostStep` suppresses the effect of an
acknowledged removal and must violate `Safety`.

```sh
cargo test --locked -p hoimin-cli --lib workspace::owned::janitor::quint_reclaim
```

## Startup janitor diagnostic retention

`janitor_diagnostics.qnt` uses two remaining slots and up to six arrivals.
It selects the earliest issue details, fills the remaining slots with the
earliest normal details, and retains the arrival order across both classes.
`priorityStep` covers eviction of the most recently retained normal detail,
first by an issue that leaves one normal detail and then by an issue that
displaces that last normal detail. `issueOnlyStep` checks overflow when the
slots are already occupied by issues. Omission counts include evictions.
Truncation counts retain the evidence of an admitted detail that was shortened
and subsequently evicted. An input omitted at admission contributes only to
its omission count. `Safety` derives the total from each detail's admission
conditions at its arrival. `truncationLostStep` counts only shortened details
still retained and must produce a violation once a shortened detail is evicted.

The Rust adapter fills the other 254 slots with earlier issues and calls
`ReclaimLedger::detail` for each simulated arrival. It compares the retained
order, exact messages and all omission counts. An oversized UTF-8 message
exercises truncation across a character boundary. A negative control drops the
priority issue and detects the changed retained details. The two models use
simulated observations and keep filesystem operations outside the replay.
Update the model and its fixtures when detail priority, accounting, or
observable truncation semantics change.

```sh
cargo test --locked -p hoimin-cli --lib workspace::owned::janitor::quint_diagnostics
```

## Startup janitor deferred cleanup

`janitor_retry.qnt` starts with two roots deferred by initial cleanup and
models up to six visits. A round visits its starting queue in order, once per
root. A root is queued again only when its slice is deferred. A new round needs
at least one deferred slice that removed an entry. A completed or failed slice
does not count toward that condition. The deadline stops visits and preserves
the queue order for incomplete cleanup reporting.

`stalledStep` completes one root and defers the other without progress.
`progressStep` retries both roots in the following round, and `deadlineStep`
stops with an unvisited root ahead of the root deferred during that round.
`nextRoundDeadlineStep` allows the next round's entry check and expires before
its first visit, so the replay exercises both clock checks.
`stalledRetryStep` starts a new round despite the absence of progress and
must violate `Safety`.

The Rust replay invokes `retry_deferred` in
`crates/hoimin-cli/src/workspace/owned/janitor.rs`, the scheduler used by
`reclaim_abandoned_with_clock_and_selection` in `owned.rs`. It supplies slice
results and a monotonic deadline, then checks the sequence of calls and the
pending queue at exit. The bounded model represents clock observations and
cleanup results as inputs. Filesystem locks, deletion, and wall-clock timing
are exercised by the separate janitor tests. Update the model and regenerate its
ITF fixtures when the retry or stop policy changes.

```sh
cargo test --locked -p hoimin-cli --lib workspace::owned::janitor::quint_retry
```

## Startup janitor candidate selection and cursor

`janitor_selection.qnt` gives each of four managed names a lexical rank.
Up to three scans see a stable set of names per scan. A scan returns the sorted
names strictly after the stored cursor when that page is nonempty. It wraps to
the full set only after exhausting names beyond the cursor. Each published
nonempty page stores its last name and increases the generation. The model
distinguishes enumeration EOF, page selection, and cursor persistence.

`wrapStep` covers the exclusive cursor and wraparound. `emptyPageStep` checks
that an empty scan preserves an existing cursor and generation.
`eofDeadlineStep` expires as soon as enumeration completes, and
`persistDeadlineStep` expires after computing the page but before writing the
cursor. Both preserve the prior cursor and generation. The negative control
`prematureCursorStep` advances the cursor on the cancelled scan at EOF.

The Rust replay calls `select_reclaim_candidates_locked_until_with_hooks`
in `crates/hoimin-cli/src/workspace/owned.rs` on small temporary directories.
It compares returned pages and scan evidence, then reads the actual coordinator
slot and generation. It also checks that a deadline leaves the coordinator
file byte-for-byte intact. Directory capacity and storage faults are exercised
by the separate janitor tests. Update the model and its ITF fixtures when
candidate ordering or the cursor publication boundary changes.

```sh
cargo test --locked -p hoimin-cli --lib workspace::owned::janitor::quint_selection
```

## Startup janitor cursor recovery

`janitor_cursor_recovery.qnt` models two slots and up to eight operations.
Each valid slot contains a generation and cursor rank. A save writes the
inactive slot and preserves the current slot. Reopening chooses the highest
valid generation, uses slot zero for the initial identical pair, and rejects
both an entirely invalid file and a valid pair that disagrees at the same
generation. `conflictingGenerationStep` checks that the reader and coordinator
open both reject that conflicting pair without rewriting the file.
After the newest digest is damaged, recovery
uses the preceding slot, and the next save increments that recovered generation.
`preferOlderStep` selects the older slot after a successful save and violates
`Safety`.

The Rust replay also joins `recoverStep` to the second page of the selection
model's `recoveredCursorStep`. The real selector uses the recovered cursor,
returns the expected page, and writes the expected replacement slot.

`interruptedWriteStep` covers a failure before writing, a partial write with
an invalid digest, and a complete write whose sync reports an error. The last
case records bytes visible on the next open under the injected fault. Survival
across actual power loss is outside the model.

The Rust replay uses `persist_coordinator_cursor`, `decode_coordinator_slot`,
and `ManagedRootCoordinator::open` from
`crates/hoimin-cli/src/workspace/owned.rs` on a temporary file. It injects
write and sync errors, checks each slot after every command, and reopens the
coordinator to check which record it uses. Update the model and regenerate
its fixtures when cursor write ordering or recovery semantics change.

```sh
cargo test --locked -p hoimin-cli --lib workspace::owned::janitor::quint_cursor_recovery
```

## Startup janitor reclaim eligibility

`janitor_eligibility.qnt` covers one active-name root with a valid lease marker.
An active lease, a retain marker, and a retain-inspection failure each stop
admission before cleanup readiness or heartbeat age is considered. Valid
cleanup-ready evidence permits reclamation. A mismatch or identity inspection
failure adds a diagnostic and falls back to heartbeat evidence. A matching
heartbeat is stale at the threshold, including equality.

The scheduled histories exercise the precedence rules, the exact stale
boundary, and the ordered diagnostics on fallback. `retentionIgnoredStep`
permits reclamation despite a retain marker and must violate `Safety`.
The Rust replay drives `ManagedRunRoot::reclaim_abandoned_with_clock_and_selection`
in `crates/hoimin-cli/src/workspace/owned.rs`. Each observation uses one
temporary root with real markers and lease handles, a supplied clock reading,
and at most one injected I/O failure. The replay compares the root disposition,
the typed diagnostics, and whether the payload survives. Staging roots and
already-claimed roots have separate cleanup rules. Update the model and
regenerate its fixtures when reclaim eligibility or diagnostic precedence changes.

```sh
cargo test --locked -p hoimin-cli --lib workspace::owned::janitor::quint_eligibility
```

## SQLite persistence, resume selection and ownership

The SQLite extension adds three domains to the earlier configurations:

| Model | Finite scope | Implementation correspondence | Negative control |
| --- | --- | --- | --- |
| Session | One run, two mutants, all seven result statuses, owner release/reacquisition and completion | Public `SessionHandler::begin/persist/lookup/load/finish` on a real temporary SQLite database; `resume_policy` on recovered results | Replace a determinate result |
| SessionSelection | Two chronological runs, two fingerprints, budgets 1 and 2, complete/incomplete rows | Public handler outcomes, fresh-start reasons and stored rows after every close/reopen, including budget promotion | Choose the oldest eligible run |
| SessionOwnership | Two independent handles, one saved run, budgets 1 and 2 | Two `SessionHandler` instances on the same SQLite database using native locks, with public ownership probes and durable row comparison | Share ownership between handlers |

`Session.recoveryStep` saves a timeout, reopens and resumes the DB, replaces it
with a killed result, and rejects a later survived replacement. Selection's
`latestStep` and `fallbackStep` force multiple eligible rows and fallback past a
newer run whose budget is too large. `freshPriorityStep` starts with empty history,
finds a matching complete run, then prefers the budget-decrease reason once a
matching incomplete run with a larger budget exists. All five ordinary fresh
reasons participate in the safety invariant and in SQLite outcome comparison.

`SessionOwnership.handoffStep` starts with a saved timeout. It repeats the run's
creation, then acquires the run and rejects another handle's promotion request
while the run is owned. The failed promotion leaves the stored budget intact.
Reopening the first handle releases the lock. The second handle can then resume
and promote the budget, and a smaller request gets the budget-decrease reason.
The schedule exercises both incomplete and complete finish. The driver also
checks durable rows and looks up the saved timeout through each handle after
every operation. Interleavings occur between completed public calls.

The Rust driver compares operation outcomes, every stored result and run row,
and probes ownership and reuse with public lookups after every lifecycle step.
Rejected operations must preserve a complete logical snapshot of all five tables.
Separate tests inject an invalid diagnostic after an inconclusive result has been
removed inside a replacement transaction, then check rollback and recovered lookup.

Literal cases cover all 49 pairs of previous and replacement statuses. PBT adds
64 histories per property, up to 47 lifecycle commands, four runs, three
fingerprints and 19 loads, including budget byte boundaries and `usize::MAX`.
The driver treats unexpected error codes as failures; an operational DB failure
cannot satisfy an expected semantic rejection. SQLite I/O uses bounded PBT rather
than a per-input libFuzzer database initialization. Existing pure-state fuzz targets
remain in the bounded campaign.

```sh
cargo test --locked -p hoimin-cli --test quint_session
cargo test --locked -p hoimin-cli --test quint_session --all-features
```

A close/reopen is orderly handle release, not a simulated process crash or power
failure. The models assume valid rows, orderly ownership release and no concurrent
writers. `CandidateChanged` belongs to the selection/recheck race, which is
covered separately by the SQLite fault tests. Existing Lean session oracles and
native-lock tests continue separately.
See the [design](../../docs/development/work/quint-session-protocol/design.md),
[plan](../../docs/development/work/quint-session-protocol/plan.md), and
[review evidence](../../docs/development/work/quint-session-protocol/review.md).

## Public bootstrap stop protocol

The bootstrap-stop extension adds three configurations sharing one model:

| Configuration | Publicly reached entry | Scope |
| --- | --- | --- |
| StopEarly | Pending target resolution or preflight (each trace is replayed at both entries) | No workspace-copy reservation or session |
| StopCopy | Pending run-start output after preflight | Copy reservation and final verification |
| StopSession | Pending run-start output after session begin | Final-report acknowledgement must precede session finish |

Every entry is built with `RunState::new` and public `transition` calls. The
adapter never edits private fields. Stops include cancellation, deadline and disk
policy. Completions cover success, generic failure, output I/O failure, retired,
duplicate, unknown, wrong-kind and mismatched cleanup reservations. Public phase,
exit code, pending IDs, retired IDs, output sequence and terminal payloads are
compared with model expectations or an independent event journal.

The finite model allows two stop requests. PBT runs 256 histories of up to 127
commands, with arbitrary u64 disk readings, then eight successful acknowledgements
to check recovery progress under that explicit assumption. It preserves the first
disk reason, freezes stop causes during and after cleanup, and requires
session finish to follow the final-report acknowledgement.
`reserveEvidenceStep` keeps a zero free-space reading when a measurement error
arrives later. `measurementEvidenceStep` keeps the measurement error message when
a reserve notification follows. The Rust replay checks the emitted diagnostic
and the final report. `diagnosticEvidenceStep` verifies that later effect
failures add unique secondary diagnostics in arrival order. The
negative model deliberately violates the last ordering. `terminalStep` forces a
stop while the final report is pending; `finalFailureStep` forces the discovered
unacknowledged-start/final-output-failure regression.

`cleanupCancelStep` and `cleanupDiskStep` deliver a stop while the
diagnostic from a failed cleanup awaits acknowledgement. Rust had issued
a second cleanup on cancellation and replaced the pending diagnostic on a
disk stop. Both schedules now preserve the diagnostic, single cleanup
record, and exit status before completing the final report. The
`StopReopensCleanup` negative control enables the original stop behavior
and violates the post-cleanup invariant.

A discovered regression is fixed: after an unacknowledged run-start failure,
failure of the eventual final report terminates the protocol instead of issuing
a new run-start. The minimized PBT input is retained both as a regression and a
fuzz seed. Literal cases cover all four entries and three original stop causes.

`run_stop` uses the same public driver in libFuzzer. The first two bytes choose
entry and initial stop cause; subsequent commands occupy nine bytes (tag and
little-endian u64 disk observation), capped at 128. Each history ends with eight
successful acknowledgements. Generated corpus must remain separate from seeds:

```sh
cargo test --locked -p hoimin-core --test quint_stop --test stop_model
mkdir -p fuzz/corpus/run_stop
cargo +nightly-2026-10-08 fuzz run run_stop \
  fuzz/corpus/run_stop fuzz/seeds/run_stop -- \
  -max_total_time=30 -max_len=4096 -timeout=5 -rss_limit_mb=1024
```

Invalid completions consume a cloned input in the replay driver; retaining the
previous state after an error is a replay policy, not a claim about an error state
returned by `transition`. Auxiliary model flags are not private-state observations.
Active workers, subprocess shutdown, actual filesystem deletion and concurrent
session transactions remain outside this bootstrap protocol model.
See the [design](../../docs/development/work/quint-public-stop/design.md),
[plan](../../docs/development/work/quint-public-stop/plan.md), and
[review evidence](../../docs/development/work/quint-public-stop/review.md).

## Ordered candidate discovery

`OrderedDiscovery` starts with two idle workers awaiting a sequential spool
read. It requests IDs `[3, 1]`, accepts up to four discovery replies, and uses
ID `2` as an unselected candidate. Each sequential reply can only request
another sequential read. EOF rejects the first missing requested ID, or starts
indexed reads in request order. `orderedStep` discovers a duplicate, then
rejects an absent and a wrong indexed body before retrying both requests.
`missingStep` reaches EOF with an incomplete selection. The negative model
requests the first body in ascending ID order and violates `Safety`.

The Rust fixture uses public `RunState::with_selection` and `transition` and
ends at the missing-selection error or the handoff to two pending apply
requests. It compares pending read targets, apply order, and exact callback
errors. The fixture retains a clone of the input for corrected replies after
an identity error. Existing worker and stop tests cover execution after handoff.

`OrderedDrain` uses the same public setup and stops during discovery or an
indexed read. Its four schedules cover partial discovery, repeated stops
and retired reads, failure of an indexed read, and a trailing discovery
failure. Surviving candidates are reported once as `NotRun` in requested
order. Failed indexed bodies are skipped. Each history hands off to final
verification or the failure diagnostic after these reports. The negative
model tries to drain an undiscovered candidate.

```sh
cargo test --locked -p hoimin-core --test machine quint_ordered
```

## Resumed candidate lookup

`ResumeCandidate` starts with one pending lookup in a resumed session. The
finite input domain covers all seven stored statuses, an absent result, and
stored results with the wrong candidate identity. Killed and survived results
use synthetic start and finish outputs. Missing or inconclusive results hand
off to real apply work. Synthetic accounting commits at the finish acknowledgement.

The three cancellation schedules cover lookup, synthetic start, and synthetic
finish. They check rejection of the retired callback both before and after its
replacement is acknowledged. Cancellation during lookup produces `NotRun`.
Cancellation during a synthetic output preserves the reused outcome. The
negative model reruns a conclusive result and violates `Safety`.

`failedStartStep` and `failedFinishStep` inject a generic effect failure.
The replacement retains the reused outcome, and an acknowledgement for the
failed request is rejected as a duplicate. After the replacement finish is
acknowledged, the run emits the failure diagnostic, cleans up, then sends its
incomplete report and finishes the session. `reportIoStep` injects a report
write failure and reaches a terminal state after the diagnostic and cleanup,
with zero committed results and no final report or session finish request.
Each history injects at most one cancellation or output failure.

The Rust replay reaches lookup via public `transition` calls, compares emitted
effects and exact errors, and checks counters after every transition. Lookup
and cancellation histories end at a pending apply or the next sequential read.
Output failure histories continue to termination. The SQLite models check
durable lookup content separately.

```sh
cargo test --locked -p hoimin-core --test machine quint_resume
```

## Baseline restoration

`BaselineRestoration` starts after a successful baseline with both the
baseline output and worker reset pending. The two acknowledgements may arrive
in either order. Only the reset releases one analysis request. The model
also checks wrong worker and completion kinds, duplicate acknowledgements,
and cancellation or effect failure before restoration. The negative model
releases analysis on output acknowledgement and violates `Safety`.

Nine ITF histories replay the public transition with two workers and one
target. They compare phase, pending and retired effect IDs, exact callback
errors, exit status, and the initial analysis or finalization effect.

## Ordered verification budget

`VerifyBudgetGate` starts after restoration with one pending budget
observation. For 30 selections on one worker and a 35-second effective
timeout, the threshold is 1050 seconds. The model compares zero remaining
time and the threshold at and within one millisecond on either side.
A shortfall emits a warning and waits for its acknowledgement before
analysis. The warning preserves the successful exit status. Cancellation,
deadline, or failure while waiting sends the run to finalization. The
negative control warns at the exact threshold.

Ten ITF histories compare the public phase, effects, warning fields,
exit status, and duplicate or retired acknowledgement errors.

## Analysis diagnostics

`AnalysisDiagnostics` starts with analysis of the last target in an ordinary
run. It models absent, empty, populated, and truncated spools with lists of up
to two warnings. Warnings are emitted in input order, including repeated
messages, with one outstanding acknowledgement. The candidate read or final
verification is released after the last acknowledgement. A duplicate reply
leaves the next warning pending. Cancellation retires the outstanding warning
and hands off to verification. A generic warning effect failure completes the
failed request and hands off to the failure diagnostic.

Ten ITF histories compare the public effects, warning content and sequence,
exact errors, exit status, and phase. The fixtures use the same populated spool
to check candidate reads after ordinary analysis and final verification after
truncated analysis. The negative model advances past analysis while the first
warning still awaits acknowledgement.

```sh
cargo test --locked -p hoimin-core --test machine quint_baseline
cargo test --locked -p hoimin-core --test machine quint_verify_budget
cargo test --locked -p hoimin-core --test machine quint_analysis
```

## Analysis spool handoff

`AnalysisSpool` models two ordered source targets on one worker in a
regular run or a run with one explicitly selected candidate. An untruncated
first reply without a spool advances to the second target. At truncation,
an ordinary run verifies originals, and explicit selection proceeds with
the available populated spool. Both record an incomplete outcome. Explicit
selection rejects a missing final spool or an empty spool with a selected
candidate still missing. The retry schedule samples corrected replies to
the same request following both errors. A deadline between targets retires
the outstanding analysis request. The negative model reads candidates
after truncation in an ordinary run.

Ten ITF histories compare target paths, the final-target flag, the
candidate spool token and record count, exit status, and exact callback
errors via public transitions. They end at candidate reading or final
verification.

```sh
cargo test --locked -p hoimin-core --test machine quint_spool
```

## Explicit candidate streaming

`ExplicitStream` scans up to four records on one worker with two requested
IDs. It applies each selected ID once in discovery order, skipping an
unrequested ID and repeat appearances. End of spool returns the first
unmatched ID until both are discovered. A wrong-worker reply leaves the
read pending. The negative model applies a repeated selected ID again.

Seven ITF histories compare effects, dispatch order, counts, and exact
errors through public transitions. The `Complete` action groups one
successful apply, killed execution, report acknowledgements, and reset.
`partialStep` supplies a corrected reply to the same read after premature
end of spool.

## Mutant admission at the run limit

`MutantAdmission` models two workers with a run limit of one and a spool
of one or two candidates. Admission reserves the slot when the first
candidate is handed to apply. A second candidate uses synthetic start and
finish outputs even if the first worker is still applying. The incomplete
status is set when the extra candidate is read, and `NotRun` is counted
when its finish output is acknowledged. A spool with exactly one candidate
can complete successfully. Verification waits for end of spool and both
workers to become idle. The negative model charges the limit when the
mutant process is requested and admits two applies before either starts.

Seven ITF histories compare both workers' pending requests, candidate
identities, apply and process requests, counters, and exit status. The
schedules include a synthetic finish before the admitted worker even
acknowledges apply.

```sh
cargo test --locked -p hoimin-core --test machine quint_explicit
cargo test --locked -p hoimin-core --test machine quint_admission
```

## Active worker failures

`WorkerFailure` starts with one admitted candidate in an ordinary run.
A generic failure at apply, start output, or execution reports the
candidate as `NotRun`. A generic failure at finish output preserves the
classified result and its count. Failure after start acknowledgement
replaces only the finish. The failure diagnostic follows the replacement
output. A report write failure proceeds directly to the diagnostic and
cleanup, then terminates with delivery suppressed. The negative model
replaces a classified result with `NotRun`.

Ten ITF histories compare effects, counts, exit status, and exact duplicate
errors for retries of failed effects. They end at the next read on the
normal path or after cleanup and reporting on the failure paths.

## Session process results

`ProcessResult` starts with one running session candidate. It models
successful and unsuccessful process exits, process timeout, and memory
exhaustion, each with complete output or an output-close timeout. An
output-close timeout classifies the result as `Error` and retains the
original termination and an output diagnostic. One matching persistence
acknowledgement records the result. A cancellation during persistence or
finish reporting preserves the classified result exactly once. A callback
for a retired persistence request returns the retired-effect error.
The negative model classifies with the process termination alone.

Ten ITF histories compare persisted and reported evidence, bad run and
candidate IDs, retired or duplicate acknowledgements, summary counts,
and exit status through public transitions. They end at pending
pre-report verification. Persistence is represented by its request and
acknowledgement.

```sh
cargo test --locked -p hoimin-core --test machine quint_worker_failure
cargo test --locked -p hoimin-core --test machine quint_process_result
```

## Original integrity acknowledgements

`IntegrityCheckpointProtocol` requires the post-materialization
checkpoint before the baseline and the pre-report checkpoint before
cleanup. After a mismatched callback, the replay uses the retained input
state to send a corrected reply. A deadline while waiting for pre-report verification
retires that request and issues a replacement. A verification failure
emits the original-change diagnostic. The negative model accepts an
unrelated checkpoint before cleanup.

Seven histories reach both checkpoints through public Rust transitions.
Two histories reproduced premature cleanup before the pre-report guard
was added. They now require the same phase error used at the
post-materialization checkpoint and compare duplicate and retired
callback errors.

## Session finish acknowledgements

`SessionFinishAck` starts at the final report of a clean or incomplete
session. A report delivery failure terminates without closing the
session. A successful report acknowledgement issues the finish request.
The finish acknowledgement must identify the same run and carry its
requested completion flag. A mismatched callback returns
`machine.session.finish_mismatch`, and a callback for a consumed request
returns the duplicate-effect error. A generic finish failure remains
terminal with an infrastructure error. Deadline requests leave the
pending report or finish unchanged.

Ten histories compare the final report, finish request, callback decisions
and exit status through public Rust transitions. The model represents
SQLite operations by their acknowledgements. Durable storage is exercised
by the SQLite suite above.

```sh
cargo test --locked -p hoimin-core --test machine quint_integrity
cargo test --locked -p hoimin-core --test machine quint_finish_ack
```

## Session startup

`SessionStartup` follows preflight in disabled, fresh, and resume modes.
Session load and begin require preflight fingerprint evidence and forward
it with the mutant budget. Resuming selects the saved run ID, and fresh
fallback preserves the selection reason in the run-start header. The
header acknowledgement releases worker creation. A deadline or session
I/O failure during a session request preserves the run-start header before
verification or a failure diagnostic. The negative model creates a
worker on resume before the header is acknowledged.

Ten histories compare the requests and report provenance, the copy
handoff, and errors on missing fingerprints or late or duplicate session
replies.

## Workspace copy preflight

`CopyPreflight` models two configured workers and a 20-byte workspace
copy budget. It accepts zero bytes and exact-budget measurements,
checks aggregate arithmetic and overflow, and requires the preflight
worker count to match the configuration before emitting a run-start
header, loading a session, or starting a new one. The negative model
proceeds after receiving a smaller worker count.

Eight histories replay the public machine with sessions disabled, fresh,
or resumed. They check exact errors and corrected replies, and verify
that both creation requests carry the original preflight ID and the
same aggregate allowance. The overflow representative sends two
workers each reporting `u64::MAX` bytes.

## Workspace worker acknowledgements

`WorkerGrant` starts with two worker creation requests that share a
copy reservation. Post-materialization verification requires matching
effect, worker and reservation IDs for both workers. Either worker can
complete first. A failure or deadline retires the outstanding siblings
and leads to a diagnostic or pre-report verification. The negative model
accepts a worker completion for a different reservation.

Nine histories compare the original preflight ID and allowance in both
requests, pending and retired IDs, and exact completion errors. The
wrong-reservation history originally advanced to materialization
verification and now checks `machine.worker.reservation_mismatch`.

```sh
cargo test --locked -p hoimin-core --test machine quint_session_startup
cargo test --locked -p hoimin-core --test machine quint_copy_preflight
cargo test --locked -p hoimin-core --test machine quint_worker_grant
```

## Active workers and Quint Connect

The registry checks thirty-four finite configurations, twenty-nine
negative controls and 273 committed histories. `Workers` and `WorkersSession` add
two workers and two
already-loaded candidates, reached only through public setup transitions.
They cover apply, start acknowledgement, execution, persistence, finish
acknowledgement, reset, spool EOF and finalization. Results include Killed, Survived and Timeout;
a stopped unexecuted candidate becomes NotRun. The negative model double-counts
an executed result while draining it after stop.

`reverseStep` completes worker 1 before worker 0. `completedStopStep` stops while
a completed result awaits persistence or output acknowledgement. `startStopStep`
and `runningStopStep` stop before start acknowledgement and during execution.
`timeoutStep` carries a process timeout through normal finalization with exit
code 4 and an incomplete report. `timeoutStopStep` cancels after that result
arrives and checks that the timeout survives draining and counts once. With a
session, cancellation can arrive before the timeout's persistence acknowledgement.
The 24-transition scenario bound lets each schedule reach session completion;
fixture tests require that terminal state. Other models retain their existing
scenario bounds.

The public driver journals requests, typed callbacks, completed results and
acknowledged outputs. Counts are derived from that history rather than updating
a mirror of production counters. Retired and duplicate probes replay real typed
callbacks, preferring late process/persistence responses. PBT expands to one
through four workers and 256 histories of up to 95 generated commands. Fuzz
uses the same driver with up to 128 commands. Successful replies are appended
after each schedule to check recovery progress under that explicit assumption.

[Quint Connect 0.1.2](https://docs.rs/quint-connect/0.1.2/quint_connect/) is pinned
in the separate `formal/quint/connect` verification workspace. Its `Driver` calls
the same public implementation driver and its `State` compares the same observable
`Snapshot`; state checking is enabled. A typed `dispatch` sum supplies action names
and arguments through `Config.nondet`, including composed scenario actions.
The library generates fresh ITF traces at test time and checks each state against
real Rust behavior. Live PBT generates seeds (four traces per seed); the separate
command PBT shrinks command histories. A semantic negative control skips a real
apply response and must fail with a state-invariant error.

```sh
cargo test --locked -p hoimin-core --test quint_workers --test workers_model
python tools/verify_quint_connect.py
mkdir -p fuzz/corpus/run_workers
cargo +nightly-2026-10-08 fuzz run run_workers \
  fuzz/corpus/run_workers fuzz/seeds/run_workers -- \
  -max_total_time=30 -max_len=4096 -timeout=5 -rss_limit_mb=1024
```

The live launcher adds the pinned Quint executable, TypeScript backend, Safety
invariant and existing 20-second/2048-MiB process-tree guard. Guard errors propagate;
timeouts cannot satisfy the semantic negative control. The launcher runs serially,
with contracts enabled, and has a five-minute outer limit. It requires POSIX
process monitoring. Ordinary workspace tests replay committed ITF without Node,
Java or the live crate. Product dependencies are unchanged.

`run_workers` input starts with session/worker-count bytes, then eleven-byte
commands (tag, worker, outcome, little-endian u64 disk reading). The outcome byte
can select an injected process timeout. Generated corpus
stays separate from seeds. This tests callback interleavings in the public state
machine. It does not prove Rust atomic memory ordering, actual subprocess behavior
or every OS thread interleaving. TLC's exhaustive result applies to the bounded
abstract model; Rust correspondence is established by the separate replay tests.

See the [design](../../docs/development/work/quint-active-workers/design.md),
[plan](../../docs/development/work/quint-active-workers/plan.md), and
[review evidence](../../docs/development/work/quint-active-workers/review.md).

At long build/verification boundaries, check free space and clean build artifacts
once Cargo, rustc and fuzz processes have stopped. The development workflow uses
periodic `cargo clean`; sources, lockfiles, seeds, fixed ITF and verification logs
remain available:

```sh
cargo clean
cargo clean --manifest-path fuzz/Cargo.toml
```

## Generated testing and fuzz

Literal rstest histories extend to proptest command sequences (256 cases per
property, up to 127 operations), including arbitrary u64 counters, zero, MAX and
boundary values beyond the finite models. One test invocation checks one history;
loops and branches stay in drivers and test oracles. Shipping APIs are unchanged.

```sh
mkdir -p fuzz/corpus/report_lifecycle
cargo +nightly-2026-10-08 fuzz run report_lifecycle \
  fuzz/corpus/report_lifecycle fuzz/seeds/report_lifecycle -- \
  -max_total_time=30 -max_len=4096 -timeout=5 -rss_limit_mb=1024
```

Use `disk_lifecycle` to check the disk lifecycle laws (first byte: owned-root mask,
then 17-byte commands with a tag and two full-width u64 fields, capped at128).
Its committed seeds include failed drains, deferred delivery and boundary readings.
Use `budget_ledger` and its corresponding directories to fuzz the reservation
journal. `report_sequence` remains the existing JSON wire-format target;
`report_lifecycle` adds arbitrary command histories against the independent
journal. Both targets are discovered by `tools/ci_fuzz.py`, which retains the
campaign deadline, bounded saved corpus, RSS and per-input timeout. The private
completion ledger is exercised by PBT instead of adding a shipping test API.

Budget commands occupy 10 bytes (tag, kind, little-endian u64), capped at 256.
Report commands occupy 19 bytes (tag, run, mutant, event u64, identity u64), capped
at 128. Incomplete trailing commands are ignored. Generated corpus is separate
from committed seeds. The fuzz harness's optional `contracts` feature forwards
the production feature and selects the corresponding invalid-event handling.

## References

- [Quint CLI](https://github.com/quint-co/quint/blob/v0.33.1/docs/content/docs/quint.md)
- [TLC](https://github.com/tlaplus/tlaplus/tree/master/tlatools)
- [Initial ledger design and evidence](../../docs/development/work/quint-budget-ledger/review.md)
- [Expanded design](../../docs/development/work/quint-state-machines/design.md)
- [Implementation plan](../../docs/development/work/quint-state-machines/plan.md)
- [Expanded review and evidence](../../docs/development/work/quint-state-machines/review.md)

- [Cleanup/ownership design and verification](../../docs/development/work/quint-finalization/review.md)
