# Lean session persistence, recovery, and ownership audit design

## Objective

Audit Hoimin's existing session persistence, resume, result replacement, and
ownership lifecycle for latent safety bugs. The audit must combine the durable
SQLite state with live run ownership in one model, distinguish Lean proofs from
bounded exploration and implementation observations, and avoid changing
production behavior unless a separately reviewed follow-up is opened.

Schema migration, future-schema handling, and deliberately corrupt databases
are excluded. This audit assumes a valid current schema and focuses on the
operational lifecycle after `SessionHandler::open` succeeds.

## Owned contract

The central claim is:

> In every reachable state over a valid schema, a run has at most one live
> owner; after completion it cannot be resumed, persisted, or finished;
> determinate results are immutable; rejected or failed operations do not
> partially update persistent state.

The audit expands that claim into these owned properties:

- one run has at most one live owner across handlers;
- `load` returns only the newest compatible incomplete run and owns it before
  returning;
- the post-lock eligibility recheck prevents a stale resume candidate from
  becoming observable;
- completing a run releases its ownership and permanently excludes it from
  resume, persistence, and later finish operations;
- an incomplete finish releases ownership while keeping the run resumable, and
  repeating that finish is idempotent;
- `Killed` and `Survived` results cannot be replaced;
- `Timeout`, `OOM`, `ProcessLimit`, `Error`, and `NotRun` results may be
  atomically replaced;
- a failed replacement preserves the complete previous result, candidate, and
  diagnostic state;
- dropping a handler or terminating its process releases every run ownership
  held by that handler;
- a rejected public operation preserves all modeled persistent state.

The model excludes filesystem durability below SQLite, kernel or filesystem
lock implementation defects, machine power loss during hardware writes,
allocator failure, poisoned synchronization primitives, schema migration, and
deliberately corrupt rows. These are infrastructure or separately owned
contracts.

## Same-premise correspondence worksheet

Every implementation comparison must name one of the following exact modes.
The adapter must not call a different-premise result a mismatch.

| Behavior | Mode | Premise and evidence |
| --- | --- | --- |
| `begin`, `load`, `lookup`, `persist`, and `finish` through `SessionHandler` | `strict` | Public in-process API, valid schema, isolated temporary database |
| Two live handlers using the same database and ownership directory | `strict` | Public APIs with the same process and real file locks |
| Ownership reacquisition after normal handler drop | `strict` | Public drop semantics followed by another public `load` |
| Ownership release after subprocess death | `internal-fixture` | Existing ignored child-process entry point driven by its parent integration test |
| Pause after initial resume read and before lock/recheck | `internal-fixture` | Test-only pause hook; it is not a public production operation |
| Arbitrary interleavings inside a public SQLite transaction | `model-only` | Public API cannot pause at each modeled statement boundary |
| Arbitrary crash after a modeled write and before commit or rollback | `model-only` | No production crash-injection hook exposes this exact premise |
| SQLite setup, subprocess launch, corpus parsing, or adapter panic | `infrastructure-error` | Audit harness failure, never a semantic mismatch |

The existing process-death and contention tests remain independent evidence.
The new strict corpus supplements rather than replaces them.

## Formal model

Add a dependency-free `SessionAudit` namespace to the pinned
`formal/HoiminOracle` project. The finite state contains:

- two handler roles and whether each handler is live;
- two run roles, each with a fingerprint role, `finished`, and `complete`;
- ownership from each run to at most one handler role;
- two mutant roles per run and their optional persisted result;
- a result class covering `Killed`, `Survived`, `Timeout`, `OOM`,
  `ProcessLimit`, `Error`, and `NotRun`;
- optional internal operation state for the explicitly modeled resume and
  replacement boundaries.

Public events are atomic model operations for open, begin, load, lookup,
persist, complete or incomplete finish, handler drop, and handler crash.
Separate internal events expose only the boundaries needed to reason about the
resume TOCTOU recheck and replacement rollback. Observations contain the
verdict, stable rejection category, selected run, returned result, durable run
state, durable results, and ownership state.

The finite domain uses semantic roles rather than real UUIDs. Its default
exploration depth is eight public or internal transitions. Event order and
state serialization are stable so the first breadth-first counterexample is
reproducible. State deduplication is permitted only after every outgoing event
has been checked; the report must disclose reachable-state and transition
counts and must call this bounded exploration rather than proof.

## Proof obligations

Lightweight imported Lean modules prove transition-local and trace-lifted
properties with explicit premises:

- owner uniqueness;
- a completed run has no owner and is not resume eligible;
- determinate result immutability;
- rejection preserves the complete modeled durable state;
- successful inconclusive replacement changes exactly the selected result;
- failed replacement preserves the old result;
- `load` selects only the newest compatible incomplete run and establishes its
  ownership;
- handler drop or crash removes exactly that handler's ownerships;
- every accepted transition preserves the session invariant;
- arbitrary traces from an initial state preserve the invariant.

Large `native_decide` checks, breadth-first exploration, shrinking, statistics,
and corpus generation live in an executable module that the library and proof
modules do not import. This keeps normal `lake build` evaluation predictable.

## Sensitivity

The audit retains deliberately broken transitions from three distinct fault
families:

- **atomicity/transactionality:** delete an inconclusive result before a failed
  replacement and fail to restore it;
- **uniqueness/idempotency:** admit a second owner for one run or allow a
  determinate result to be replaced;
- **boundary/precedence:** return the initially read resume candidate without
  rechecking eligibility after ownership acquisition.

Each family must be rejected by a fixed witness or the bounded explorer. The
audit records the shortest trace and the exact invariant that fails. A broken
variant that is not detected is an audit failure, not an acceptable limitation.

## Strict corpus and Rust adapter

Lean owns a deterministic JSONL corpus. Each case declares schema version,
case ID, exact mode, schedule, and expected observations. The corpus contains
only `strict` cases. `internal-fixture` and `model-only` evidence is recorded
separately so it cannot be mistaken for public-API correspondence.

The Rust adapter lives with the CLI integration tests because `SessionHandler`
is implemented by `hoimin-cli`. For every case it:

1. creates an isolated temporary session database;
2. maps handler, run, fingerprint, and mutant roles to real values;
3. invokes only the named public operations;
4. observes returned values and persistent state without recomputing expected
   semantics;
5. compares the complete observation to the Lean-owned expectation;
6. classifies a panic or harness/setup failure as `infrastructure-error`.

Corpus validation rejects unknown schemas, modes, events, observations, and
duplicate case IDs. Single-case replay is available for diagnosing a minimal
witness. Corpus freshness is checked by regenerating it and requiring an empty
diff.

## Deliverables

- `SessionModel.lean` for state, events, observations, and correct and broken
  transitions;
- `SessionProofs.lean` for lightweight proof obligations;
- `SessionCases.lean` for deterministic strict cases and JSON encoding;
- `SessionAuditMain.lean` for corpus generation, bounded exploration,
  shrinking, sensitivity witnesses, and statistics;
- `corpus/session-recovery.jsonl` generated by Lean;
- `crates/hoimin-cli/tests/lean_session_oracle.rs` for strict correspondence;
- a self-contained audit report under `docs/superpowers/reports/`;
- a counterexample ledger if any implementation mismatch is found;
- this design and a task-by-task implementation plan under
  `docs/superpowers/`.

## Verification and cost reporting

The audit has independent gates:

1. pinned Lean build and theorem checks;
2. deterministic corpus generation and freshness diff;
3. strict Rust correspondence, including single-case replay;
4. existing process-death and contention fixtures;
5. all three broken-model sensitivity families;
6. focused session integration tests;
7. Rust formatting, Clippy, full workspace tests, and diff hygiene.

The report records commands, elapsed times, corpus size, reachable states,
checked transitions, maximum depth, reduction rules, and each broken witness.
It also identifies where expensive evaluation occurs and confirms that the
large explorer is not imported by ordinary proof modules.

## Result handling

Any strict mismatch remains as a reproducible case and is classified as a
confirmed implementation bug, specification ambiguity, model defect, or
infrastructure error. This audit does not silently adjust the model to match
the implementation and does not repair production code in the same change.
