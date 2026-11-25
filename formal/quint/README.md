# BudgetLedger: Quint / TLC / MBT / PBT

This follows the finite model and negative control in `pytest-allpair/formal/quint`,
targeting the public `hoimin_core::BudgetLedger` reservation/release API.
Quint describes a TLA state machine; TLC exhaustively enumerates its reachable
finite states. Apalache 0.62.4 translates the model to TLA+, while TLC performs
the check. This complements existing Lean and Creusot arithmetic proofs.

## Run

Requirements: Node 22, Java 21, Python 3.14+, and the repository's Rust toolchain.
The npm lock pins Quint to 0.33.1. The first TLC check downloads Apalache unless
it is installed under `QUINT_HOME` (default: `~/.quint`).

```sh
npm ci --prefix formal/quint --ignore-scripts --no-audit --no-fund
python tools/verify_quint.py
cargo test --locked -p hoimin-core --test quint_budget --test budget_model
```

After changing the model, regenerate expected histories with
`python tools/verify_quint.py --update`. The local editing shortcut
`--simulate-only --update` does not require Java and does not run TLC.
The normal check regenerates four ITF histories using seed 1172 and 24 transitions
and compares them with committed fixtures. Generator metadata (timestamps and
source paths) is removed; expected states are preserved. Ordinary `cargo test`
uses committed fixtures without requiring Node or Java.

Logs and resource statistics go to `_build/`. The existing POSIX process-tree guard
limits each command to 20 seconds and 2048 MiB RSS. Java heap is 1 GiB; TLC uses
one worker. Timeout, memory exhaustion, syntax errors and missing Java never count
as successful negative-control detection. CI installs the TLA toolchain before
this short checking budget, verifies the release archive's SHA-256, and caches it.

## Evidence and limits

| Path | Checks | Scope |
| --- | --- | --- |
| Quint → TLC | Capacity, unique IDs, disjoint active/released IDs | Capacity 2 per kind, 3 kinds, amounts 0–3, at most 2 successful reservations, release IDs 0–2 |
| Overdraft negative control | Checker detects invariant violation | Reserving amount 3 yields a counterexample |
| Quint → ITF → Rust MBT | Status, ID, reservation contents and totals against public implementation | Four deterministic histories |
| rstest → proptest | Literal histories extended to generated, shrinkable command sequences | 256 cases, up to 127 operations, u64 boundaries, independent journal model |
| libFuzzer | Arbitrary byte histories against independent journal | Up to 256 operations, seeds for zero, full-width amounts and repeated release |

The two-reservation limit bounds the model's workload. It does not replace
production `ReservationIdsExhausted` with small-number exhaustion. ID exhaustion,
unbounded histories, OS effects, concurrency and liveness are outside this model.
TLC success alone does not establish correspondence with Rust; ITF replay is
therefore a separate required check. The journal oracle retains all reservation
events and sums with u128, unlike the production active-ID map. No verification
API is added to shipping code.

## Fuzz

```sh
mkdir -p fuzz/corpus/budget_ledger
cargo +nightly-2026-10-08 fuzz run budget_ledger \
  fuzz/corpus/budget_ledger fuzz/seeds/budget_ledger -- \
  -max_total_time=30 -max_len=4096 -timeout=5 -rss_limit_mb=1024
```

Existing `tools/ci_fuzz.py` discovers this target from the manifest and applies
the campaign deadline, bounded saved corpus, per-target RSS and per-input timeout.
Each operation occupies 10 bytes: operation tag, kind, and little-endian u64.
Incomplete trailing operations are ignored. Keep generated corpus separate from
committed seeds.

## References

- [Quint CLI](https://github.com/quint-co/quint/blob/v0.33.1/docs/content/docs/quint.md)
- [TLC](https://github.com/tlaplus/tlaplus/tree/master/tlatools)
- [Design](../../docs/development/work/quint-budget-ledger/design.md)
- [Implementation plan](../../docs/development/work/quint-budget-ledger/plan.md)
- [Review and execution evidence](../../docs/development/work/quint-budget-ledger/review.md)
