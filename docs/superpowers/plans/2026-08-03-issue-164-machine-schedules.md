# Issue #164: Adversarial machine schedules

Issue: [#164](https://github.com/tokyogas-tech/hoimin/issues/164)

## Scope

The integration property in `crates/hoimin-core/tests/machine.rs` drives the real, pure
`transition` function. It keeps the current `RunEffect` values as the only pending work,
maps each generated index modulo that set, applies at most 64 generated actions, and then
drains the machine deterministically with a 1,024-step ceiling.

The generator varies:

- zero through six candidates;
- one through three workers;
- unfiltered, explicit-ID, and ordered-ID runs;
- the Cartesian product of all three filters with sessionless and session-backed runs;
- killed, survived, timeout, out-of-memory, process-limit, and cancelled process results;
- arbitrary pending-effect completion order, cancellation, deadline, and phase-aware effect
  failure.

`ScheduleAction::Fail` includes lifecycle output effects (`MutantStarted` and
`MutantFinished`) so both real and synthetic Starting/Finishing retry paths are explored.
It excludes only diagnostics and terminal/report outputs: a failed terminal output is not
an emitted `RunFinished` and therefore cannot satisfy the property's emitted-stream
premise. Failed lifecycle outputs are independently tracked and omitted from the accepted
output stream; their replacement outputs must still form exactly one pair.

## Independent oracles

The harness does not copy the transition table. It only translates concrete pending
effects into their public completion events. A separate `scheduled_candidates` set is
populated when the harness hands `CandidateLoaded(Some(candidate))` to `transition`, never
from machine output. Expected mutant counts come from a test-only ledger populated by the
generated completion class; cancellation, deadline, or failure records every scheduled
candidate without a completed classification as `NotRun`. All seven counters are
incremented explicitly in test code; the production `MutationSummary::record` helper is
not reused.

For every completed schedule, the property checks:

| Invariant | Oracle |
| --- | --- |
| terminal state | `RunPhase::Finished` after the bounded drain |
| one terminal report | exactly one non-retired `RunFinished` output effect |
| effect identity | every returned `RunEffect::id()` inserts once into a `BTreeSet` |
| candidate lifecycle | independently counted start keys, finish keys, and `scheduled_candidates` are identical and each output count is one |
| seven summary counts | generated completion ledger compared field by field with `RunFinished.counts` |
| report ordering | the complete non-retired output stream is accepted, in order, by a fresh `ReportSequence` |

## Explicit generated regressions

- [#111](https://github.com/tokyogas-tech/hoimin/issues/111): generated ordered
  candidate sets are interrupted after a generated number of completions, then boundedly
  drained.
- [#112](https://github.com/tokyogas-tech/hoimin/issues/112): a zero-record ordered spool
  with an empty selection is exercised with generated jobs and schedules and must finalize
  cleanly.
- [#113](https://github.com/tokyogas-tech/hoimin/issues/113): an ordered, session-backed
  three-candidate run advances a generated process classification to `PersistResult`, is
  interrupted, and must retain that classification while draining the ordered remainder.

Four explicit lifecycle-failure regressions cover real `MutantStarted`, real
`MutantFinished`, synthetic `MutantStarted`, and synthetic `MutantFinished` output
effects. Each requires exactly one `schedule.effect` diagnostic, followed by cleanup and
then the terminal output.

## RED evidence

The property found a real failure before the production fix. A session-backed schedule
with two candidates shrank to an `EffectFailed` at `PersistResult`: the old generic failure
path retired the candidate lifecycle and emitted `RunFinished` while a mutant had started
but never finished. The source-parallel seed is committed beside `machine.rs`. The minimal
fix sends failures raised during `RunPhase::Mutants` through the existing stopped-candidate
drain and emits the saved diagnostic after the drain, before cleanup.

The strengthened scheduled-candidate oracle found a second real failure: if a lifecycle
output failed while an ordered synthetic drain was already in progress, restarting the
drain replaced the existing `stopped_candidates` queue and silently lost its remaining
candidates. The fix carries the existing queue into the rebuilt, stably sorted drain.

Temporary production mutations also established that the property is sensitive to each
major oracle:

- freezing `allocate_id` produced `DuplicateEffect(EffectId(1))`;
- replacing the terminal `Finished` phase with `Finalize` exhausted the bounded driver
  without reaching a terminal state;
- recording `Killed` as `Survived` failed the independent field-by-field ledger check;
- freezing `output_sequence` was rejected by `ReportSequence` as `NotMonotonic`.
- moving `WorkerPhase::Applying` out of the stopped-candidate drain made the explicit
  synthetic-start regression fail before it could produce the required lifecycle output,
  demonstrating that the independent scheduled-candidate contract detects a missing drain
  phase.

All intentional mutations were reverted. Source-parallel seeds for both genuine
production failures are retained.

## Verification

```bash
cargo test -p hoimin-core --test machine adversarial_schedule
cargo test -p hoimin-core
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```
