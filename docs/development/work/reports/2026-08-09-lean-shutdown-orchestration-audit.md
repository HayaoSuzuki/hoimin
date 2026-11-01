# Lean Shutdown Orchestration Audit Report

## Result

The audit found no confirmed shutdown-orchestration defect in the Rust
implementation under the premises that could be reproduced. All three strict
Lean-owned cases matched the public CLI, all ten selected existing unit/E2E
fixtures passed, and the reviewed mismatch set remains empty.

The formal work did find and correct one defect in the audit model before it
was used as an oracle: `firstInterrupt` and `deadlineReached` initially replaced
an already installed primary error in an inconsistent input state. Proof of
primary-error retention rejected that definition. Both transitions now retain
the existing primary error. This was a model defect, not evidence of a Rust
defect; production source was not changed.

## Audited contract and excluded scope

The audited interval begins when a run is bootstrapped or a first stop cause is
observed and ends after process, output drain, blocking I/O, workspace cleanup,
session finish, report, and metrics obligations have settled or transferred
ownership. It includes first/second interrupt, total deadline, process exit and
failure, ordered error aggregation, singleton dispatch, final return, and
detachment ownership.

The model deliberately excludes wall-clock duration values, scheduler
fairness, PID reuse, filesystem and SQLite implementation details, individual
mutant identities, and OS-specific signal-delivery internals. Exact timing and
process-tree correspondence is supplied by controlled CLI fixtures, not by the
Lean transition system. Linux hard-cgroup behavior and Windows Job Object
behavior are outside this audit.

Lean proves properties of the model, not of the compiled Rust program. Rust
correspondence is established only where an existing internal fixture or the
public-binary adapter realizes the same premise and observes the stated result.

## Existing-evidence worksheet summary

The companion worksheet closes all twelve claims. Eight have direct strict or
internal-fixture correspondence. Four remain model-only where the production
premise depends on a private completion identity, private spawn-gate boundary,
or a post-commit pause point that the public CLI cannot create. Those rows are
explicit limitations rather than inferred production proofs.

## Formal state and event model

`ShutdownModel.lean` represents:

- one optional stop cause and anchored deadline ordinal;
- process states from not-started through running, termination-requested,
  exited, and reaped;
- output, blocking, workspace, session, report, and metrics component states;
- ownership transfer for detached blocking work;
- stable primary error plus ordered appended errors;
- process/reap and four singleton-dispatch counters;
- completion flags, exit code, and terminal return;
- seven instrumentation fields that make intentionally broken semantics
  observable by `safe`.

There are 31 semantic events. Rejected events return the input state unchanged.
Twelve deterministic cases are generated from exact schedules: three strict,
six internal-fixture, and three model-only.

## What Lean proved

Ten theorems compile without `sorry`, `admit`, or axioms:

- `rejected_preserves_state` — every rejected event is transactional;
- `first_cause_is_retained` — every non-forced step retains an installed cause;
- `first_deadline_is_not_extended` — a started shutdown retains its deadline;
- `forced_interrupt_returns_130` — an active second interrupt returns code 130
  and cannot leave blocking work pending;
- `primary_error_is_retained` — later failures do not replace the primary;
- `step_preserves_structural_invariant` — all singleton counters remain bounded;
- `singleton_dispatches_are_bounded` — cleanup/session/report/metrics dispatch
  counts are each at most one;
- `reaped_never_runs_again` — one model step cannot restart a reaped process;
- `detached_has_transferred_ownership` — every safe detached state records
  ownership transfer;
- `run_preserves_structural_invariant` — the structural bound holds for every
  finite event trace, without a depth restriction.

## What remained bounded

The combined `safe` predicate is checked by finite exploration rather than
proved inductively as a whole. It includes cause/error overwrite counters,
completion regression, ordering violation, forced-wait behavior, ownership
loss, completion-flag consistency, code-130 cause consistency, no pending
blocking work at forced return, and exit-code presence after return.

Depth 9 is sufficient to reach the shortest counterexamples for all seven
broken families and broadly combines initialization, stop, process, and
finalization milestones. It is not an unbounded statement about every possible
event sequence. The separate structural theorems above are unbounded over
finite traces.

## Exploration size, reductions, and cost

Fresh measurements on 2026-08-09:

| Measurement | Result |
| --- | --- |
| Depth | 9 |
| Alphabet | 31 events |
| Reachable exact states | 20,472 |
| Checked transitions | 352,098 |
| Corpus | 12 cases |
| Reduction | none |
| `lake build` | real 2.59 s |
| Corpus freshness | real 0.37 s |
| Statistics/audit | real 0.38 s |
| Sensitivity | real 0.38 s |

All successors are generated before exact-state deduplication. A hash set is
used only to implement exact equality efficiently; no partial-order or semantic
state reduction is applied. The initial list-based implementation took 86.97 s
because it recomputed the exploration and used linear membership. Sharing one
exploration and using exact-state hashing reduced the operation to below one
second without changing the 20,472-state/352,098-transition result.

## Broken-family sensitivity

Every intentionally broken family was detected. The greedy shrunk witnesses
were:

| Family | Minimal detected trace |
| --- | --- |
| Cause overwrite | `first_interrupt, deadline_reached` |
| Duplicate dispatch | `boot, start_cleanup, start_cleanup` |
| Ordering | `start_report` |
| Error precedence | `start_process, process_failed, cleanup_failed` |
| Forced wait | `start_blocking, second_interrupt` |
| Completion regression | `boot, start_cleanup, cleanup_completed, start_session_finish, session_finished, first_interrupt` |
| Ownership loss | `start_blocking, detach_blocking` |

This gate prevents a vacuous or insensitive `safe` predicate from making the
correct-model exploration appear successful.

## Strict real-CLI correspondence

The adapter parses the generated JSONL with unrecognized-field rejection and validates
schema, all modes/scenarios/events/states, twelve unique IDs, nonempty schedules,
singleton dispatch bounds, and completion-flag consistency. It invokes only
`CARGO_BIN_EXE_hoimin`, public CLI options, a controlled Python interpreter, OS
SIGINT, JSON output, SQLite reads, and fixture PID markers.

| Case | Expected and observed |
| --- | --- |
| `normal_completion` | code 0; complete report; complete session; no descendant; bounded exit |
| `first_interrupt_running` | code 130; incomplete report; incomplete session; descendant stopped; bounded exit |
| `total_timeout_reaps_descendant` | code 4; incomplete report; incomplete session; descendant stopped; bounded exit |

All cases matched together and under individual
`HOIMIN_SHUTDOWN_ORACLE_CASE` selection. Infrastructure errors: 0. Actual
mismatches: 0. Reviewed mismatches: 0. Pipe drains run concurrently, and child
plus marked fixture processes are reaped on normal and error paths.

## Internal-fixture evidence

The exact selected tests passed for second-signal forced return, first-budget
retention, owned-close preemption, expired-close detachment, shutdown-drain task
accounting, cancellation/timeout precedence over simultaneous exit, and ordered
cleanup-error aggregation. The real E2E fixtures passed for first SIGINT with a
parseable incomplete session, second SIGINT while session finish held a SQLite
writer lock, and total timeout while the same finalizer was blocked.

The production correspondence is strongest around owned values: the first
`ShutdownBudget` is retained through `Option::get_or_insert`; `JoinSet`s and the
completion receiver remain owned across drain; the workspace is taken before
blocking close and either restored after success or left exclusively in the
detached task; the process handle is cloned before transfer; and the interrupt
monitor remains alive until outer finalization resolves.

## Model-only boundaries

- Public output cannot observe identity/count for all four cleanup, session,
  report, and metrics dispatches. Lean proves the common bound; component tests
  provide production evidence separately.
- A public process cannot inject a private `ShellCompletion` or duplicate effect
  identity. Transactional rejection is therefore proved in the shutdown model
  and checked in the existing machine oracle, not claimed as strict CLI equality.
- There is no public pause seam after durable session/report completion but
  before a late stop. Late-completion non-regression remains model-only for the
  outer layers.
- The public CLI cannot pause exactly between `ProcessStartGate` admission and
  task insertion. Existing gate tests cover this linearization boundary.

## Mismatches and classifications

There are no unresolved mismatches, infrastructure errors, or specification
ambiguities. Consequently no counterexample ledger is created. The only
classification encountered was the corrected audit-model defect described in
the Result section; it was resolved before corpus generation and did not alter
Rust behavior or an expected observation to accommodate Rust.

## Model, adapter, timing, and platform limitations

- The state model uses semantic milestones; it does not prove real-time bounds.
- The 12-second adapter bound and one-second public total timeout are controlled
  observations, not a proof for all machine loads.
- Real-signal strict replay runs on Unix in this adapter. Windows still compiles
  and runs parser/projection tests and has an existing console-control E2E
  counterpart, but the new JSONL adapter does not duplicate its console helper.
- PID liveness checks are safe for short controlled fixtures but do not model
  arbitrary PID reuse.
- The explorer starts at `State.initial`; arbitrary inconsistent states are
  addressed only by theorem premises that quantify over them.
- No hard-cgroup, OOM, process-limit, network filesystem, or crash-recovery
  premise is included.

## Repair handoff

No Rust repair is justified by this audit, so production changes and a repair
worktree were intentionally not created. If a future strict mismatch appears,
the first action is to preserve its generated case ID, rerun only that ID,
confirm descendant/child teardown, and classify the difference before changing
either side. A valid Lean expectation must not be weakened merely to match Rust.

The most valuable future evidence upgrades are narrowly scoped test seams, not
behavior changes: expose public evidence for dispatch identity, add a same-premise
post-commit pause using an existing external lock if one becomes available, or
extend the Windows adapter with the already-tested console-control helper. Each
upgrade should first produce a failing correspondence test in a separate
worktree; production hooks remain out of scope unless independently approved.

## Exact reproduction commands

From `formal/HoiminOracle`:

```bash
lake build
lake exe generate_shutdown -- --check corpus/shutdown-orchestration.jsonl
lake exe generate_shutdown -- --stats
lake exe generate_shutdown -- --sensitivity
```

From the repository root:

```bash
cargo test -p hoimin-cli --test lean_shutdown_oracle -- --nocapture
HOIMIN_SHUTDOWN_ORACLE_CASE=normal_completion cargo test -p hoimin-cli --test lean_shutdown_oracle oracle_correspondence -- --exact --nocapture
HOIMIN_SHUTDOWN_ORACLE_CASE=first_interrupt_running cargo test -p hoimin-cli --test lean_shutdown_oracle oracle_correspondence -- --exact --nocapture
HOIMIN_SHUTDOWN_ORACLE_CASE=total_timeout_reaps_descendant cargo test -p hoimin-cli --test lean_shutdown_oracle oracle_correspondence -- --exact --nocapture
cargo clippy -p hoimin-cli --test lean_shutdown_oracle -- -D warnings
```

The exact internal/E2E commands are retained in the approved implementation
plan and summarized in the completed coverage worksheet.
