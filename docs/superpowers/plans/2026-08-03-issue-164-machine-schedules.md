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
- sessionless and session-backed runs;
- killed, survived, timeout, out-of-memory, process-limit, and cancelled process results;
- arbitrary pending-effect completion order, cancellation, deadline, and non-report effect
  failure.

Report-write failure is excluded from `ScheduleAction::Fail`: a failed `EmitOutput` is not
an emitted event and therefore cannot simultaneously satisfy the property's premise that
the complete emitted stream contains one `RunFinished`. Report failures have dedicated
machine tests; all other pending effect variants remain eligible for generated failure.

## Independent oracles

The harness does not copy the transition table. It only translates concrete pending
effects into their public completion events. Expected mutant counts come from a test-only
ledger populated by the generated completion class (or `NotRun` when a generated stop
drains an unfinished candidate). All seven counters are incremented explicitly in test
code; the production `MutationSummary::record` helper is not reused.

For every completed schedule, the property checks:

| Invariant | Oracle |
| --- | --- |
| terminal state | `RunPhase::Finished` after the bounded drain |
| one terminal report | exactly one non-retired `RunFinished` output effect |
| effect identity | every returned `RunEffect::id()` inserts once into a `BTreeSet` |
| candidate lifecycle | independently counted `MutantStarted` and `MutantFinished` IDs are identical and each count is one |
| seven summary counts | generated completion ledger compared field by field with `RunFinished.counts` |
| report ordering | the complete non-retired output stream is accepted, in order, by a fresh `ReportSequence` |

## Explicit generated regressions

- [#111](https://github.com/tokyogas-tech/hoimin/issues/111): generated ordered
  candidate sets are interrupted after a generated number of completions, then boundedly
  drained.
- [#112](https://github.com/tokyogas-tech/hoimin/issues/112): a zero-record ordered spool
  with an empty selection is exercised with generated jobs and schedules and must finalize
  cleanly.
- [#113](https://github.com/tokyogas-tech/hoimin/issues/113): a generated process
  classification is advanced to `PersistResult`, interrupted, and required to retain that
  classification in the report and summary.

## RED evidence

The property found a real failure before the production fix. A session-backed schedule
with two candidates shrank to an `EffectFailed` at `PersistResult`: the old generic failure
path retired the candidate lifecycle and emitted `RunFinished` while a mutant had started
but never finished. The source-parallel seed is committed beside `machine.rs`. The minimal
fix sends failures raised during `RunPhase::Mutants` through the existing stopped-candidate
drain and emits the saved diagnostic after the drain, before cleanup.

Temporary production mutations also established that the property is sensitive to each
major oracle:

- freezing `allocate_id` produced `DuplicateEffect(EffectId(1))`;
- replacing the terminal `Finished` phase with `Finalize` exhausted the bounded driver
  without reaching a terminal state;
- recording `Killed` as `Survived` failed the independent field-by-field ledger check;
- freezing `output_sequence` was rejected by `ReportSequence` as `NotMonotonic`.

All intentional mutations were reverted. Only the genuine persistence-failure seed is
retained.

## Verification

```bash
cargo test -p hoimin-core --test machine adversarial_schedule
cargo test -p hoimin-core
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```
