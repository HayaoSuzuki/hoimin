# Lean State-Machine Oracle Investigation Report

## Result

A Lean-generated 13-case corpus found one previously uncovered state-machine bug. When an early
cancellation had already scheduled cleanup and a deadline arrived while that cleanup was pending,
Hoimin retired the original cleanup effect and emitted a replacement. The repair makes a repeated
stop during stopped cleanup idempotent, retaining the original pending cleanup and its effect ID.

After the repair, all 13 strict correspondence cases match, all focused state-machine tests pass,
and there are no unresolved mismatches or infrastructure errors.

## Scope and durable claim

The model covers effect registration and completion identity, typed rejection, stop requests,
cleanup scheduling, final-output scheduling, and late-stop behavior. It excludes parsing,
filesystem and database I/O, wall-clock timing, process supervision, candidate discovery, and full
worker scheduling.

The durable claim is that a completion is accepted at most once; rejected completions are
transactional at the public boundary; stopping prevents new ordinary work; accepted results are
not lost; and cleanup and final output are each emitted at most once in the required order without
a late stop rewriting the selected outcome.

## What Lean established

`formal/HoiminOracle/HoiminOracle/Proofs.lean` proves these properties inside the formal model:

- `rejected_preserves_state`: every typed rejection returns the original model state.
- `accepted_completion_not_pending`: under the allocator freshness condition, an accepted effect
  ID is not left pending.
- `stop_is_first_writer_wins`: from a running state with no stop cause, a later stop cannot replace
  the first cause.
- `late_stop_preserves_final`: stopping is an exact no-op while final output is pending or after the
  run is finished.
- `no_ordinary_emission_after_stop`: a state that has observed a stop emits no ordinary work.

Three deliberately broken variants accept a duplicate, overwrite a late final stop, or schedule
ordinary work after stop. Fixed witnesses must distinguish all three before corpus generation is
allowed. `lake build` checks the theorems and witnesses.

These proofs apply to the Lean model only. They do not establish implementation correspondence.

## What the adapter observed

`crates/hoimin-core/tests/lean_oracle.rs` reads expectations generated once by Lean, constructs each
scenario through public `RunState` transitions, maps semantic roles to real emitted effect IDs, and
compares stable observations. It does not calculate expected phases, emissions, pending counts, or
errors.

The initial report-mode run classified:

- 12 matches;
- 1 semantic mismatch;
- 0 infrastructure errors.

The mismatch was `cleanup_is_emitted_once`. The exact schedule was:

```text
StartRequested -> CancellationRequested -> RunStarted acknowledgement
               -> Cleanup pending -> DeadlineReached
```

Lean expected the deadline to preserve the pending cleanup and emit nothing. Rust emitted another
cleanup. Phase and pending count both remained `cleaning` and one, so ordinary phase-only tests did
not reveal that the original effect identity had been retired and replaced.

The other cases covered unknown, wrong-kind, duplicate, and retired completion rejection; normal
ordinary chaining; stopping ordinary scheduling; cleanup-before-final ordering; final-output
uniqueness; and stop idempotence during pending final output and after completion.

## Root cause and repair

Both stop branches in `crates/hoimin-core/src/machine.rs` set `stop_requested`, called
`retire_pending()`, and selected another cleanup when no copy grant existed. They did not recognize
that `RunPhase::Cleaning` plus an already-set `stop_requested` flag represented the same active stop
and cleanup obligation.

The public transition guard now returns the unchanged state and no effects for that exact repeated
stop condition. It intentionally does not ignore a first stop arriving during a normal cleanup:
both `RunPhase::Cleaning` and the pre-existing `stop_requested` flag are required.

`lean_oracle_regression_cleanup_is_emitted_once` first failed on the duplicate cleanup, then passed
after the repair. It also asserts that the original cleanup ID remains pending and is not retired.
The Lean-owned counterexample remains strict, so the correspondence adapter independently protects
the same semantic slice.

See the [counterexample ledger](2026-08-09-lean-state-machine-counterexamples.md) for the complete
expected/actual comparison, impact, limitation, and decision.

## Model and adapter limitations

- The Lean model abstracts all ordinary state-machine effects into one kind.
- The adapter acknowledges the mandatory real `RunStarted` output inside the semantic first-stop
  translation before observing cleanup.
- The confirmed counterexample uses the real early-stop path with no copy grant. It demonstrates
  effect identity and scheduling through `transition`, not filesystem timing or concurrent handler
  execution.
- A rejected Rust transition consumes its state and returns only `MachineError`; the adapter uses
  the stable pre-event public observation because no mutated rejected state can escape the API.
- No claim is made about OS process control, database transactions, or asynchronous shell timing.

There are no unresolved specification or ownership decisions and no infrastructure failures in the
final corpus run.

## Reproduction

From the repository root:

```console
(cd formal/HoiminOracle && lake build)
(cd formal/HoiminOracle && lake exe generate -- --check corpus/state-machine.jsonl)
cargo test -p hoimin-core --test lean_oracle
HOIMIN_ORACLE_CASE=cleanup_is_emitted_once \
  cargo test -p hoimin-core --test lean_oracle \
  oracle_correspondence -- --exact --nocapture
cargo test -p hoimin-core --test machine \
  lean_oracle_regression_cleanup_is_emitted_once -- --exact
```

The full Rust quality gate is:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Future CI boundary

This branch does not edit CI workflows. A future job can pin the committed Lean toolchain, cache
Elan/Lake artifacts, and call the same three independent gates: `lake build`, corpus freshness, and
the strict Rust adapter. Report-mode exploration should remain nonblocking; only reviewed strict
cases and every infrastructure error should gate CI.
