# Lean report-sequence audit design

## Goal

Formally audit the public event-ordering contract implemented by
`ReportSequence::observe`, compare Lean-generated expectations with the Rust
API under the same premises, and repair any confirmed Rust mismatch by first
retaining the minimal Lean witness as a Rust regression test.

The durable claim is:

> An accepted public report-event trace has one run identity, strictly
> increasing report sequence numbers, unique stable mutant identities, matched
> mutant start/finish lifecycles, no active mutant at run finish, and no event
> after run finish. A rejected event leaves every later public observation
> unchanged.

This work is isolated in
`.worktrees/lean-report-sequence-audit` on branch
`audit/lean-report-sequence`. The specification, implementation plan, Lean
artifacts, generated corpus, Rust tests, audit report, and any required Rust
repair all belong to that worktree and branch.

## Scope

Included behavior:

- `RunStarted`, `MutantStarted`, `MutantFinished`, `Diagnostic`, and
  `RunFinished` events;
- run start, run identity, and terminal run lifecycle;
- strictly increasing public report sequence numbers;
- stable mutant ID to mutant-sequence correspondence;
- concurrent active mutants with distinct identities;
- matching mutant start and finish;
- status and termination correspondence on mutant finish;
- rejection precedence when one event violates more than one rule;
- transactional rejection, observed through valid public probe events.

Excluded behavior:

- JSON or JSONL serialization and output-sink I/O;
- `RunState` scheduling and effect completion;
- summary-count and score arithmetic;
- session, metrics, workspace, and process lifecycles;
- allocation or overflow of sequence numbers before events reach
  `ReportSequence`;
- private Rust fields or a new production test seam.

The existing result-lifecycle audit remains independent. It checks agreement
among reports, session rows, metrics, and exit policy. This audit instead
checks the event protocol accepted by `ReportSequence` itself.

## Declared and implicit behavior

Declared behavior comes from the public event types, `ReportSequenceError`,
the `observe` documentation, and the existing `report_policy` integration
tests. It requires a started run, one run ID, strictly increasing sequence
numbers, stable mutant identity, paired starts and finishes, no active mutant
at finalization, and terminal finalization.

The audit makes these implicit behaviors explicit:

- lifecycle and identity errors take precedence over a simultaneous sequence
  error because Rust checks monotonicity after those validations;
- a repeated active start differs from reuse after finish;
- reuse with the same mutant sequence differs from reuse with another mutant
  sequence;
- a finish with a known ID but different sequence differs from a finish that
  was never started;
- an absent termination skips status-classification correspondence, while a
  present termination must map to the supplied status;
- every rejected event is atomic even when validation has already inspected
  several protocol dimensions.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| report event kind | `Event.kind` | public `OutputEvent` variant | `observe` result and later probe | `OutputEvent`, `ReportSequence::observe` | `strict` |
| report sequence | `Event.sequence : Nat` | public event `u64` field | accepted or `NotMonotonic` | public API call | `strict` for corpus values representable as `u64` |
| run identity | reduced `RunId` with two values | public event `String` | accepted or `RunIdMismatch` | public API call | `strict` |
| mutant identity | reduced `MutantId` with two values | public candidate/start `String` | accepted or typed mutant error | public API call | `strict` |
| mutant sequence | reduced natural values `0` and `1` | public `u64` field | accepted or typed mutant error | public API call | `strict` |
| active mutant lifecycle | Lean active set and seen map | sequence of public start/finish events | later finish or run-finish probe | public API call | `strict` |
| status/termination relation | reduced status and termination constructors | public `MutationStatus` and optional `ProcessTermination` | accepted or `MutantStatusTerminationMismatch` | public API call | `strict` |
| rejected-state atomicity | rejected verdict retains exact state | submit invalid event to owned `ReportSequence` | result of a valid follow-up probe | public API call | `strict` for complete probe observations |
| complete private state equality | Lean structure equality | Rust fields are private | not publicly observable in one call | Lean theorem only | `model-only` |

Implementation-facing rows start as `model-only` during authoring and are
promoted to `strict` only after their production configuration and complete
public observation are reviewed. Every final implementation-facing row must
be `strict`. The general Lean state equality theorem remains `model-only`;
the Rust adapter claims only the complete behavior observed by the case's
follow-up probe. Parsing, event construction, panic capture, or incomplete
probe execution is classified as `infrastructure-error`, never as a semantic
mismatch.

## Formal model

Add focused `ReportSequenceModel`, `ReportSequenceProofs`, and
`ReportSequenceCases` modules. Imported modules contain only semantic
definitions and kernel-checked proofs. Exhaustive trace generation,
shrinking, statistics, sensitivity evaluation, and corpus serialization stay
in a non-imported `ReportSequenceAuditMain.lean` executable.

The Lean state records:

- the last accepted report sequence, if any;
- the accepted run ID, if any;
- the active set of `(mutant ID, mutant sequence)` pairs;
- the stable mapping from mutant ID to its first accepted mutant sequence;
- whether run finish has been accepted.

The transition returns either an accepted next state or a named rejection.
Its validation order deliberately matches the production contract:

1. terminal and run-lifecycle validation;
2. mutant lifecycle, stable identity, and status/termination validation;
3. report-sequence monotonicity;
4. state mutation only after every validation succeeds.

The model uses normalized identities and reduced statuses only where those
values preserve every future transition distinction. Corpus rendering maps
each normalized value to an exact public Rust value.

## Proof obligations

Kernel-checked proofs establish, with their premises visible:

- `rejected_preserves_state`: every rejected step returns the original state;
- `accepted_sequence_advances`: an accepted event's sequence is greater than
  the previous accepted sequence;
- `accepted_run_id_is_stable`: after start, every accepted event has the same
  run ID;
- `seen_identity_is_stable`: an accepted mutant ID maps to only its first
  mutant sequence;
- `finish_removes_active`: an accepted mutant finish removes exactly its
  matching active identity;
- `run_finish_requires_no_active`: accepted run finish has an empty active
  set;
- `finished_is_terminal`: every step after accepted run finish is rejected;
- `run_preserves_invariant`: arbitrary Lean traces preserve the named state
  invariant from the initial state.

No theorem is described as proof of Rust. Rust correspondence is established
separately by executable replay of Lean-owned cases.

## Bounded refutation and sensitivity

Search traces shortest-first in a stable event order. Start with these finite
representatives:

- two run IDs;
- two mutant IDs;
- mutant sequences `0` and `1`;
- report sequences `0`, `1`, and `2`;
- present and absent termination, with representative matching and mismatching
  statuses.

Begin at the smallest trace depth containing every fixed witness. Measure
cases, retained semantic states, transitions, elapsed time, and peak aggregate
RSS. Increase the depth by at most one only when the prior run is comfortably
within its limits and the additional depth covers a named claim. Bounded
search is reported as a finite check, not a proof.

The executable must detect these deliberately broken variants before it may
write or validate a corpus:

| Risk family | Broken behavior | Required fixed witness |
| --- | --- | --- |
| atomicity/transactionality | record a start or advance `last` before all validation succeeds | rejected invalid event followed by a valid probe distinguishes the polluted state |
| uniqueness/idempotency | permit reuse of a mutant ID after its first accepted start | start, finish, then reuse the same ID |
| boundary/precedence | accept an equal sequence by replacing `<=` with `<` | two otherwise valid events at the same report sequence |
| boundary/precedence | check sequence before lifecycle and identity | an event violating both dimensions returns the wrong typed rejection |

Fixed witnesses remain outside any semantic-state deduplication. If a broken
variant is not distinguished, corpus generation fails.

## Corpus and Rust adapter

Lean emits deterministic versioned JSON Lines. Each case contains:

- a stable case ID and exactly one allowed mode;
- the complete event prefix;
- the target event and expected accepted/rejected result;
- the normalized expected error code and relevant fields;
- a follow-up probe event and expected result when atomicity must be observed;
- the finite-domain premise identifiers used by the case.

A new `hoimin-core` integration test validates the corpus schema, rejects
duplicate IDs and unknown modes, maps each model event to the corresponding
public `OutputEvent`, and calls `ReportSequence::observe`. Rust does not
recompute expected semantics. Each case runs behind panic capture; a panic,
parse failure, unsupported value, missing probe, or incomplete observation is
an `infrastructure-error`.

During authoring, `model-only` rows may print differences without drawing a
production conclusion. Each difference includes the case ID, full event
prefix, expected and actual typed result, probe result, and differing
observation fields. A row becomes a gating `strict` case only after
same-premise correspondence and ownership review.

## Reconciliation and Rust repair policy

Classify every witness as one of:

- `confirmed bug`: public Rust behavior violates the owned report protocol;
- `specification ambiguity`: the intended precedence or lifecycle is not
  owned by current public behavior and tests;
- `model defect`: Lean omitted or changed a contract-relevant premise;
- `infrastructure error`: setup, parsing, construction, panic capture, or
  observation failed.

For a confirmed bug, retain the minimized Lean witness in the corpus, add a
named Rust regression test that fails for the same semantic reason, and then
make the smallest production change in `report.rs`. Do not refactor unrelated
report code and do not weaken Lean to match current Rust behavior.

For a specification ambiguity, preserve the witness as `model-only` and
record the exact owner question in the report. Do not change production
behavior without a separate decision. If strict correspondence is green, do
not make a speculative Rust change.

## Resource limits and verification

Run at most one potentially expensive Lean command at a time. Use the existing
resource guard with a 20-second wall-clock timeout, a 768 MiB aggregate RSS
limit, 250 ms sampling, and Lake `-Kjobs=1`. Give non-trivial theorems local
heartbeat limits, initially 100,000. Never use unlimited heartbeats.

The Rust worktree baseline is:

- `cargo build --workspace`: pass;
- initial `cargo test --workspace`: setup failure because the new worktree did
  not yet contain its local `.venv` link;
- after the standard untracked `.venv -> ../../.venv` setup,
  `cargo test --workspace`: pass;
- the initially failing candidate-ranking test also passes in isolation after
  the same setup, confirming an environment prerequisite rather than a
  semantic baseline failure.

The `.venv` link is worktree-local setup and must never be staged. Final
verification includes guarded Lean module builds, a separate proof consumer,
sensitivity checks, bounded statistics, corpus freshness, the strict Rust
adapter, existing report-policy tests, formatting, clippy with warnings
denied, workspace tests, placeholder scans, and `git diff --check`.

## Deliverables

- Lean model, proof, case, and non-imported audit executable modules;
- deterministic versioned JSONL corpus;
- strict public Rust correspondence adapter;
- a named Rust regression test and minimal production repair if a mismatch is
  confirmed;
- implementation plan and this design document;
- a self-contained audit report containing the claim and boundary,
  correspondence worksheet, finite bounds and costs, theorems, sensitivity
  matrix, minimal witnesses, per-mode results, reconciliation decisions,
  resource limits, and exact reproduction commands.
