# Lean shutdown orchestration audit design

## Objective

Audit Hoimin's asynchronous shutdown path for latent implementation bugs. The
audit follows a stop cause from signal or deadline selection through process
termination, output draining, blocking-I/O completion or detachment, workspace
cleanup, session finalization, metrics finalization, and the final result. The
work prioritizes finding real implementation defects over maximizing the
number of easy Lean/Rust correspondence cases.

The audit change must not modify production code. When it finds a divergence,
it preserves the smallest reproducible witness and supplies enough causal and
code-level evidence to begin a separate repair worktree immediately.

## Chosen approach

Use a vertical shutdown-lifecycle audit rather than a broad survey of every
Tokio task or a process-supervision-only model. This retains the high-risk
interfaces between `interrupt`, `shell`, `process`, workspace ownership,
session finalization, metrics, and `RunState`, while keeping the formal domain
finite.

The existing Lean `RunState` oracle remains independent. It covers effect
identity and the core cleanup/final-output state machine but explicitly omits
asynchronous shell timing, process supervision, filesystem work, and database
operations. The new `ShutdownAudit` namespace owns these outer orchestration
boundaries and imports no executable explorer into ordinary proof modules.

## Audited contract

The central contract is that shutdown selects a stable cause, stops new work,
settles or safely detaches every owned activity, and emits at most one coherent
terminal result without exceeding the first established shutdown deadline.

The owned properties are:

- the first established shutdown cause and deadline cannot be extended or
  silently replaced by a later cancellation, deadline, or cleanup failure;
- a second interrupt forces exit code 130 without waiting indefinitely for a
  blocked session finish, metrics write, output drain, or blocking task;
- after stopping, no new mutant process or ordinary work is dispatched;
- each modeled process is terminated and reaped at most once;
- process exit and termination request races retain one terminal process
  result and do not lose required output cleanup;
- output drain, workspace cleanup, session finish, metrics finalization, and
  final report are each dispatched at most once;
- cleanup and drain failures are appended after the primary failure rather
  than replacing its stable cause or code;
- completed stages never regress to pending or incomplete states;
- shutdown expiry detaches only work whose ownership has been transferred to
  the detached task;
- a returned success implies required process, cleanup, session, and report
  obligations are settled;
- a returned failure leaves either a parseable incomplete session or a
  parseable final report when the corresponding persistence operation began;
- all public completion notifications are accepted at most once, and stale or
  duplicate notifications cannot mutate durable observations.

The audit does not claim that every ordering produces the same exit result.
It defines an allowed outcome for each explicit event order and checks
precedence, ownership, idempotency, and bounded termination.

## Excluded scope

The audit excludes kernel defects, actual memory exhaustion, hard-cgroup
enforcement, Windows Job Object behavior, hardware write durability below
SQLite and atomic rename, allocator failure, and poisoned synchronization
primitives. It does not claim real-time scheduling equivalence between Lean
and Tokio.

OS-specific behavior that cannot run on hosted CI remains existing fixture
evidence or a stated limitation. No self-hosted runner is assumed.

## Implementation correspondence modes

Every comparison has exactly one premise mode:

| Mode | Meaning |
| --- | --- |
| `strict` | Public API or production entrypoint, deterministic input, and no private pause point |
| `internal-fixture` | A barrier, controlled child, paused blocking task, or injected completion is needed, but the production branch remains reachable |
| `model-only` | The boundary is inside an atomic future, transaction, or OS action and cannot be reproduced with the same implementation premise |
| `infrastructure-error` | Corpus parsing, fixture setup, mapping, panic, or harness timeout failed; this is never a semantic mismatch |

Only a same-premise `strict` difference is immediately classified as a
confirmed implementation bug. An `internal-fixture` difference can also be a
confirmed bug when the report demonstrates how the same branch is reachable
in production. Different-premise observations are never silently promoted to
strict mismatches.

## Formal model

Create a dependency-free `ShutdownAudit` namespace in the pinned
`formal/HoiminOracle` Lake project. The finite state contains:

- stop cause: none, interrupt, forced interrupt, deadline, process failure, or
  infrastructure failure;
- the first shutdown deadline role and whether expiry was reported;
- process state: not started, running, termination requested, exited, or
  reaped;
- output state: open, draining, drained, or failed;
- blocking task state: absent, active, completed, or detached;
- workspace state: untouched, materialized, cleanup pending, cleaned, or
  cleanup failed;
- session state: absent, incomplete, finishing, complete, or finish failed;
- metrics/report states: absent, pending/finalizing, written, failed, or
  skipped where appropriate;
- pending effect roles and ownership-transfer flags;
- primary failure, appended cleanup failures, and selected exit-code role;
- counters for process start/terminate/reap, cleanup, session finish, metrics,
  and report dispatches.

Public events represent first and second interrupts, deadline arrival,
process exit, shutdown start, and typed completion/failure notifications.
Internal events expose only the boundaries needed to reason about an active
blocking task, cleanup dispatch, session commit, report replacement, and
simultaneously ready timeout/completion choices.

The model observes cause, phase, exit selection, every component state,
pending effects, ownership transfer, dispatch counters, primary error, and
ordered appended error classes. It does not encode wall-clock values; it uses
ordered deadline roles and explicit expiry events.

## Proof obligations

Lean proof modules establish transition-local properties with explicit
premises, including:

- first-deadline and first-cause retention;
- rejection preserves all observable model state;
- completed component states do not regress;
- dispatch counters never exceed one for singleton obligations;
- reaped processes cannot return to running or termination-pending states;
- primary error retention while cleanup errors append in order;
- detachment requires ownership transfer;
- forced interrupt selects exit 130 and has no wait obligation;
- one-step safety preservation and trace lifting for the tractable structural
  invariant.

Properties that make the proof unwieldy remain named finite checks in the
explorer. The report must distinguish theorem-backed claims from bounded
checks and Rust observations.

## Sensitivity

Retain deliberately broken transitions for at least these families:

1. a later deadline overwrites the first interrupt cause or extends its
   deadline;
2. cleanup or final report is dispatched twice;
3. final report is committed before process reap or required cleanup;
4. a cleanup failure replaces the primary failure;
5. forced interrupt waits for a permanently active blocking task;
6. a successful session finish regresses to incomplete;
7. a detached task retains no ownership while the caller also drops it.

Each family has a fixed witness and must also be detectable by the bounded
explorer where it violates the common safety predicate. Failure to detect a
broken family blocks corpus generation.

## Exploration strategy

Use deterministic breadth-first exploration over a stable event alphabet.
Check every outgoing transition before deduplicating successor states. State
normalization may erase only values proven irrelevant to future observations,
and every reduction rule is recorded in the report.

The initial target is depth 9. If state growth is excessive, use phase-aware
partial-order reduction only for events that commute in both state and
observation; validate each reduction against an unreduced smaller-depth run.
Record depth, alphabet size, reachable states, checked transitions, reduced
transitions, shortest witnesses, and wall-clock cost.

Priority schedules cover:

- first interrupt versus deadline;
- forced interrupt versus blocked session finish;
- process exit versus termination request;
- output EOF versus drain expiry;
- cleanup completion versus shutdown expiry;
- cleanup failure following a process failure;
- successful session finish followed by late cancellation;
- report write followed by a late error;
- blocking completion versus detachment.

## Rust correspondence and fixtures

Generate deterministic JSONL expectations from Lean. A strict Rust adapter
must parse with `deny_unknown_fields`, reject unknown schemas, modes, roles,
events, outcomes, and duplicate case IDs, and support single-case replay by an
environment variable.

The adapter replays only observable public or production-entrypoint behavior.
It never recomputes expected semantics. Internal-fixture tests use the
narrowest existing seams around `finish_with_interrupt_monitor`,
`ShutdownBudget`, `drain_processes`, `finalize_run_with_shutdown`,
`finalize_metrics_with_shutdown`, process supervision, and the controlled E2E
signal fixtures. Audit-only helpers may live in integration-test files. Because
this audit forbids changes under `crates/*/src/`, it may use only existing
`cfg(test)` seams in production modules; a missing seam is reported as a
correspondence limitation and proposed for the later repair worktree rather
than added here.

Each case executes behind an unwind boundary. An ordinary API error remains a
semantic observation; setup errors, panics, and fixture timeouts are
infrastructure errors. Timeout bounds are generous relative to the controlled
fixture and are not used to infer model semantics.

## Existing evidence to retain

The audit maps, rather than duplicates without purpose, existing tests for:

- first and second interrupt forwarding/forced exit;
- first shutdown-budget activation and total-timeout anchoring;
- paused materialization under cancellation/deadline;
- process/output error precedence and cleanup detail ordering;
- process termination and wait cleanup;
- drain expiry task counts and buffered ownership accounting;
- detached workspace/resource cleanup;
- expired shutdown metrics skipping;
- incomplete parseable session behavior in real-signal E2E tests.

The report records exact test names and identifies coverage gaps before adding
any audit fixture.

## Divergence handling and repair handoff

Every divergence is classified as `confirmed bug`, `specification ambiguity`,
`model defect`, or `infrastructure error`. The model is corrected only when its
premise or transition is wrong; it is never weakened merely to match Rust.

For every unresolved confirmed bug, a counterexample ledger must include:

- the violated claim and correspondence mode;
- the minimal event trace and full observation after every step;
- Lean expectation and Rust observation;
- the production-reachable path;
- root-cause hypothesis tied to exact functions, branches, and owned values;
- expected user impact and necessary environmental conditions;
- recommended repair and at least one alternative;
- compatibility, deadlock, data-loss, and cancellation risks of each repair;
- the first RED regression test for the repair worktree;
- focused and full verification commands;
- unresolved owner decision, if any.

Known mismatches remain named in the corpus adapter. The test asserts the exact
reviewed mismatch set, so a new mismatch, infrastructure failure, or unexpected
disappearance of an existing witness fails CI.

## Deliverables

- this design and a detailed implementation plan;
- Lean model, proof, cases, and executable explorer modules;
- Lean-generated shutdown corpus;
- strict Rust correspondence adapter and minimal internal fixtures;
- coverage worksheet for existing tests;
- final audit report with theorem scope, exploration costs, and limitations;
- a counterexample ledger when any implementation mismatch remains.

Pure documentation commits include `[skip ci]`. The final branch tip must
trigger CI. Production files under `crates/*/src/` must not change, including
test-only additions inside production modules.

## Verification gates

The audit has independent gates:

1. pinned Lean build and theorem checks;
2. deterministic corpus regeneration and freshness diff;
3. all broken-family sensitivity witnesses;
4. strict correspondence and named single-case replay;
5. relevant internal fixtures and real-signal E2E tests;
6. existing Lean state-machine and budget corpora/adapters;
7. Rust formatting, all-target/all-feature Clippy, full workspace tests, and
   diff hygiene;
8. changed-path verification showing no production source modification.

The audit reports failures truthfully and stops at the audit boundary. Repair
begins only in a separate worktree and pull request after the finding is
reviewed.
