# Lean State-Machine Oracle Design

## Objective

Investigate Hoimin's most failure-prone state-machine semantics with a small Lean model and
executable expectations. Compare those expectations with observations from the real
`hoimin_core::machine::transition` entry point. If the comparison exposes a production bug,
retain the counterexample, add a failing Rust regression test, and make the smallest repair.

The immediate goal is trustworthy local verification. CI integration is intentionally deferred,
but every verification step will have a deterministic, non-interactive command boundary that a
future CI job can invoke unchanged.

## Durable Claim

For the modeled lifecycle slice, each effect completion is accepted at most once; rejected
completions are transactional; stopping cannot schedule new ordinary work; accepted results are
not lost during stopping; and cleanup and final reporting occur at most once in the required
order without a late stop signal changing the selected outcome.

Lean establishes these properties only for the formal model. Correspondence with Hoimin is a
separate executable comparison through the public Rust state-machine API.

## Scope

### Included behavior

- Effect registration and effect identity.
- Pending, completed, and retired effect classification.
- Successful completion, wrong-kind completion, unknown completion, duplicate completion, and
  completion after retirement.
- Cancellation and deadline requests.
- Cleanup scheduling and acknowledgement.
- Final-report scheduling, acknowledgement, and terminal behavior.
- Selected interleavings between an outstanding completion and a stop request.
- Transactionality of rejected events.

### Excluded behavior

- Parsing, serialization details beyond the generated corpus format, and presentation.
- Filesystem and database I/O.
- Wall-clock behavior and timeout measurement.
- OS process creation, containment, termination, and reaping.
- Candidate discovery, mutation syntax, and report rendering.
- Full worker scheduling and every `RunState` field.

If a model/implementation mismatch reaches CLI ownership or scheduling behavior, the investigation
may trace into `crates/hoimin-cli/src/shell.rs`. That does not expand the Lean model unless the
counterexample cannot be stated correctly without doing so.

## Selected Approach

Use a semantic-slice oracle rather than modeling the entire `RunState` or proving isolated helper
lemmas without schedules. The model will be small enough to review and prove, while retaining the
cross-feature interleavings most likely to expose latent defects.

A full `RunState` model would duplicate incidental implementation structure and make
correspondence harder to audit. Helper-only proofs would miss bugs caused by otherwise-correct
rules interacting in an unexpected order.

## Architecture

The workflow is:

```text
Lean semantic model -> theorems and broken witnesses -> generated JSONL corpus
                    -> Rust adapter using RunState + transition
                    -> match / mismatch / infrastructure error
                    -> report and counterexample ledger
```

The Lean project will live under `formal/HoiminOracle/` with its toolchain and Lake configuration
versioned alongside the model. The generated corpus will be committed beneath that project and
will never be edited by hand.

The Rust adapter will live in the `hoimin-core` test boundary. It will translate one corpus case
into public `RunState` construction and `transition` calls, then normalize only stable public
observations such as phase, emitted effect kinds, pending status, exit code, and typed error code.
Expected observations will not be duplicated in Rust fixtures or helper code.

## Formal Model

The model state contains only:

- lifecycle phase;
- pending effects and their completion kinds;
- completed and retired effect identities;
- immutable first stop cause;
- whether cleanup is required, pending, or complete;
- whether final output is absent, pending, or accepted;
- the selected abstract outcome.

The model transition function returns either a new state plus abstract effects or a typed
rejection plus the unchanged state.

The proof obligations are:

1. A completion is accepted at most once.
2. Unknown, duplicate, retired, and wrong-kind completions do not change state.
3. After the first stop request, no new ordinary-work effect is emitted.
4. Cleanup and final output are each emitted at most once.
5. Final output is emitted only after the modeled cleanup obligation is resolved.
6. A stop signal after final-output scheduling or acceptance leaves the selected outcome intact.
7. For explicitly allowed completion/stop interleavings, accepted result information is retained.

Each important rule receives a positive example, a negative example, and a broken-model witness
that demonstrates the generated corpus would detect removal or reversal of the rule.

## Generated Corpus

Lean defines cases once and writes deterministic JSON Lines. Each case records:

- a stable case identifier and strict/report classification;
- initial abstract configuration;
- the event schedule;
- expected transition verdicts;
- stable observations after each step;
- the final observation.

Generation order and field order are stable. A freshness command regenerates into a temporary
location and compares bytes with the committed corpus. The adapter never rewrites the corpus or a
strict-mode report.

Each case runs in isolation. Parse failures, setup failures, panics, timeouts, and unexpected exits
are infrastructure errors rather than semantic mismatches.

## Correspondence Adapter

The adapter exercises `RunState` and the public `transition` function directly. It constructs the
smallest real state-machine path needed for each schedule, using real public event and effect
types. It does not replace state-machine behavior with a fake executor.

Some abstract states cannot be constructed directly because `RunState` correctly hides internal
fields. The adapter will reach those states through public transitions and match emitted effect
identities back into subsequent completion events. Corpus schedules describe semantic roles rather
than hard-coding allocator IDs where the real API owns identity allocation.

When the model intentionally abstracts several production events into one semantic event, the
adapter owns only that translation. It must not calculate expected outcomes.

## Mismatch Handling and Repair

Every comparison is classified as `match`, `mismatch`, or `infrastructure error`. A mismatch report
contains:

- the smallest schedule and exact reproduction command;
- Lean's expected observation;
- the Rust observation and differing fields;
- relevant source locations;
- user-visible or safety impact;
- the model boundary and limitations;
- any unresolved domain decision.

Lean is not weakened to mirror current behavior. Before production code changes, the mismatch is
reproduced with a focused Rust test that fails for the same reason. The repair is the smallest
change that restores the agreed invariant. The counterexample remains in the corpus and moves to
strict verification after the repair.

If the model exposes ambiguity rather than a defect, the case remains in report mode and in the
counterexample ledger until ownership and intended semantics are resolved.

## Verification Order

Verification runs in this order:

1. Lean build and theorem checks.
2. Broken-model witnesses.
3. Corpus freshness.
4. Adapter unit tests.
5. Strict comparison for established semantics, without rewriting reports.
6. Report-mode comparison for all cases.
7. Single-case reproduction for every mismatch.
8. Focused Rust regression tests for confirmed defects.
9. `cargo fmt --check`.
10. `cargo clippy --workspace --all-targets -- -D warnings`.
11. `cargo test --workspace`.

The worktree uses the repository checkout's controlled `.venv` through an uncommitted local
symlink for Rust integration tests that require Python.

## Documentation and Deliverables

All deliverables remain inside the dedicated worktree and branch:

- this design under `docs/superpowers/specs/`;
- the implementation plan under `docs/superpowers/plans/`;
- the self-contained investigation report under `docs/superpowers/reports/`;
- a counterexample ledger referenced by the report;
- the Lean model, proofs, generated corpus, adapter, and reproduction commands.

The report will clearly distinguish what Lean proved inside the model from what the adapter
observed in Hoimin. It will record unresolved specification decisions and infrastructure failures
separately.

## Future CI Boundary

This change does not edit CI workflows. The local commands will nevertheless separate Lean build,
corpus freshness, and strict correspondence so a future job can cache the Lean toolchain and call
the same gates. Report-mode exploration will not be required for an eventual green CI gate; only
promoted strict cases will be blocking.
