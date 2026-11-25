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
20 samples). Named scenario actions additionally generate 12-transition histories:
`Report.interleavedStep` starts two mutants and finishes them in reverse order;
`Completion.gapStep` completes four requests in reverse order;
`Completion.wrongKindStep` attempts a wrong kind before a generic failure.
These schedules constrain inputs and reuse the same model transitions to compute
expectations. They avoid depending on chance to cover these paths.

Logs and resource statistics go to `_build/`. The POSIX process-tree guard limits
each command to 20 seconds and 2048 MiB RSS. Java heap is 1 GiB; TLC uses one worker.
The models run sequentially. Timeout, memory exhaustion, syntax errors and missing
Java never count as successful negative-control detection. Freshness checks
preserve expected states and remove only generator metadata.

## Models and implementation correspondence

| Model | Finite domain | Rust correspondence | Negative control |
| --- | --- | --- | --- |
| Budget | Capacity 2 per kind, 3 kinds, amounts 0–3, 2 successful reservations | Public `BudgetLedger::reserve/release`, returned IDs, contents and totals | Overdraft |
| BudgetExtended | Same capacities, 4 successful reservations, release IDs 0–4 | Same public adapter, covering release followed by further allocation | Shared overdraft control |
| Report | 2 run IDs, 2 mutant IDs, 2 mutant identity sequences, accepted event numbers 0–5; attempts use the last or next number | Public `ReportSequence::validate/observe` error categories and priority; the journal also checks exact errors | Finish while mutants remain active |
| Completion | 4 issued requests, 2 completion kinds, generic failure, attempted IDs 0–5 | Internal fixture of production `RunState::allocate_id/register/accept_completion/retire_pending`; pending, next ID, retired, compressed floor/gaps and outcomes | Accept a retired completion again |

Report covers run start/finish, mutant start/finish and a diagnostic as the
representative ordinary report event. Its model-derived replay compares public
validation outcomes; private validator fields are not inspected. The independent
accepted-event journal compares exact errors, exercises rejected `observe` calls
in default builds and probes subsequent validation to detect hidden state changes.
With contracts enabled, invalid consumer input goes through `validate`, because
`observe` intentionally asserts on invalid producer events.

Completion replay compares the internal ledger's state directly. Its independent
PBT oracle uses an uncompressed journal of pending/completed/retired slots and
derives the prefix, rather than copying the production floor/gaps update loop.
This is internal-fixture correspondence, not a proof of the surrounding public
transition protocol or OS scheduling. Cleanup reservation-argument matching and
other completion payloads are outside this finite model.

The reservation/request counts bound workloads; they do not replace production
u64 ID exhaustion with small-number exhaustion. Unbounded histories, fairness,
liveness and OS process effects are outside these models. TLC success alone does
not establish Rust correspondence; ITF replay remains a separate required check.

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
