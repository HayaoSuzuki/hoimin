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
adapter compares state fields and acceptance directly with generated expectations.
PBT additionally checks rejected-input snapshot atomicity, terminal freeze, primary
failure stability, diagnostic deduplication and cleanup/finish prerequisites.

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

The corpus through the cleanup and ownership extension contains 52 histories, including `cleanStep`/`deferredStep`
for disk, `cleanStep`/`refusedStep`/`unjoinedStep`/`availableStep` for finalization,
and `reacquireStep` for ownership. Six negative-control models check detection of
unsafe transitions. Every generated history participates in freshness checking.

```sh
cargo test --locked -p hoimin-core --test quint_disk --test disk_model
cargo test --locked -p hoimin-cli --lib shell::finalization::quint
cargo test --locked -p hoimin-cli --lib session::ownership::quint
```

## SQLite persistence and resume selection

The SQLite extension adds two independent domains to the earlier configurations:

| Model | Finite scope | Implementation correspondence | Negative control |
| --- | --- | --- | --- |
| Session | One run, two mutants, all seven result statuses, owner release/reacquisition and completion | Public `SessionHandler::begin/persist/lookup/load/finish` on a real temporary SQLite database; `resume_policy` on recovered results | Replace a determinate result |
| SessionSelection | Two chronological runs, two fingerprints, budgets 1 and 2, complete/incomplete rows | Public handler outcomes, fresh-start reasons and stored rows after every close/reopen, including budget promotion | Choose the oldest eligible run |

`Session.recoveryStep` saves a timeout, reopens and resumes the DB, replaces it
with a killed result, and rejects a later survived replacement. Selection's
`latestStep` and `fallbackStep` force multiple eligible rows and fallback past a
newer run whose budget is too large. `freshPriorityStep` starts with empty history,
finds a matching complete run, then prefers the budget-decrease reason once a
matching incomplete run with a larger budget exists. All five ordinary fresh
reasons participate in the safety invariant and in SQLite outcome comparison.

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
disk reason, freezes stop causes during cleanup/final-report/session completion,
and requires session finish to follow the final-report acknowledgement. The
negative model deliberately violates the last ordering. `terminalStep` forces a
stop while the final report is pending; `finalFailureStep` forces the discovered
unacknowledged-start/final-output-failure regression.

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

## Active workers and Quint Connect

The registry checks seventeen finite configurations, eleven negative controls and
114 committed histories. `Workers` and `WorkersSession` add two workers and two
already-loaded candidates, reached only through public setup transitions.
They cover apply, start acknowledgement, execution, persistence, finish
acknowledgement, reset, spool EOF and finalization. Results are Killed/Survived;
a stopped unexecuted candidate becomes NotRun. The negative model double-counts
an executed result while draining it after stop.

`reverseStep` completes worker 1 before worker 0. `completedStopStep` stops while
a completed result awaits persistence or output acknowledgement. `startStopStep`
and `runningStopStep` stop before start acknowledgement and during execution.
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
commands (tag, worker, outcome, little-endian u64 disk reading). Generated corpus
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
