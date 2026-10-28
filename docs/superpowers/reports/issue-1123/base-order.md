# Multiple-inheritance base-order experiment

This follow-up lets reviewers assess a `base_order_shuffle` research prototype
alongside the original inheritance proposals. It adds no product operator.
The six examples distinguish missing assertions about dispatch/callback order,
permitted reorderings and an invalid method resolution order (MRO).

## Mutation and replay

For two distinct direct bases, swap them. For three or more, use a fresh
`random.Random(seed)` to shuffle their names. If the resulting list equals the
original, swap its first two entries. This bounded fallback guarantees a changed
order and preserves each base exactly once. It does not sample nonidentity
permutations uniformly. A seed is reset for each class so fixture enumeration
does not influence its edit. The receipts record the Python version, seed,
original/replacement descriptors and the before/after base lists.

Seed 0 transforms `Child(A, B)` into `Child(B, A)` and `Child(A, B, C)` into
`Child(A, C, B)` on both measured interpreters. Seed 5 leaves a three-element
shuffle unchanged; a regression test verifies that the fallback produces
`Child(B, A, C)`. Tests cover three through eight bases with 32 seeds each,
same-seed replay, preservation of the class body and generation without target
import. Each fixture emits one candidate, without enumerating all permutations.
Rotation or reversal would also probe order dependence, but cover fewer orders
over repeated runs; the seeded shuffle remains small and reproducible.

The prototype accepts only the known ASCII examples with distinct simple-name
bases. It replaces the span from the first base to the last; comments and
formatting within that span are not preserved. This is not a production source
editor, eligibility checker or a runtime MRO analyzer.

## Comparison results

As in the original experiment, each cell is **pool / useful leads / passes both /
errors**. A useful lead passes the weak assertion and fails the strong assertion;
an exception in either execution is counted separately. The nearest pool uses
the same four operators, and the default pool includes every generated candidate.
Every pool fits the 64-candidate ceiling: zero candidates remain unexecuted.

| Fixture | Nearest pool | Defaults | Prototype |
| --- | --- | --- | --- |
| base_order_dispatch | 0 / 0 / 0 / 0 | 4 / 0 / 3 / 1 | 1 / 1 / 0 / 0 |
| base_order_super_callback | 6 / 0 / 1 / 0 | 6 / 0 / 1 / 0 | 1 / 1 / 0 / 0 |
| base_order_three_super_callback | 9 / 0 / 1 / 0 | 9 / 0 / 1 / 0 | 1 / 1 / 0 / 0 |
| base_order_commuting_control | 6 / 3 / 1 / 0 | 8 / 4 / 1 / 0 | 1 / 0 / 1 / 0 |
| base_order_explicit_call_control | 7 / 0 / 0 / 0 | 6 / 0 / 0 / 0 | 1 / 0 / 1 / 0 |
| base_order_mro_risk | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 1 / 0 / 0 / 1 |

[CPython 3.14.9](results-base-order-python314.json) and
[CPython 3.15.1](results-base-order-python315.json) agree on candidate descriptors
and verdicts. Each run executes 12 original checks and 134 mutant checks across
67 candidates, including repeated pools. Neither existing pool contains the
same edited source as a prototype. The three positive examples expose gaps not
found by the existing pools in those fixtures. No general novelty, prevalence
or performance claim follows from these small synthetic examples.

The dispatch contract requires the leftmost provider's value when providers
differ; the weak check supplies equal values. The callback contracts require
each event exactly once in declared order, while weak checks observe only
membership and multiplicity. The cooperative methods already use `super()`
correctly; changed MRO alone changes their sequence. The set-valued control
permits either order. Explicit `A.run(self, emit)` and `B.run(self, emit)` calls
anchor delegation independently of MRO, so the explicit-call control survives.

For the risk control, `B` derives from `A`: `Child(B, A)` is valid, but
`Child(A, B)` raises `TypeError` when the class is created. Compilation alone
does not catch that error. Both executions classify it as an error, never an
assertion gap.

## What this says about super

Python's [multiple-inheritance rules](https://docs.python.org/3/tutorial/classes.html#multiple-inheritance)
make `super()` follow MRO. The mutation probes whether tests observe an intended
dispatch order or cooperative call sequence. It does not establish that the
original uses `super()` correctly: correct cooperative methods can produce
kills, and explicit base calls can survive. A survivor alone cannot justify
adding an order assertion when the API permits either order.

The evidence supports further evaluation of base-order mutation separately from
the original two single-inheritance operators. A product proposal would need
bounded static class/binding analysis, invalid-MRO exclusions, handling for
dependent bases and descendants, source-preserving edits and selector/plan
replay tests. Imported/dynamic bases, metaclasses and `__init_subclass__` effects
need explicit policy. These are future requirements, not implemented guards.

## Environment and reproduction

The new receipts were produced on Windows with a local hoimin 0.14.0 executable,
identified by binary SHA-256 in each JSON. Its build source revision was not
independently attested. `source_revision` records the runner checkout at
`0033b008582b24bead8bdf28f8980275e96500a3`; runner/helper hashes identify the
modified experiment bytes. Consequently the new pools are observations of this
binary, not a claim to reproduce the historical macOS build exactly. Historical
`results-python314.json` and `results-python315.json` remain unchanged.

The shared helper now writes source with `newline=""`. Its previous default
converted LF to CRLF on Windows, causing CLI byte offsets to disagree with the
in-memory fixture. A regression test first reproduced this discrepancy and then
passed after the two source-write sites were corrected. No product code changes.

Use `HOIMIN_BINARY` for the executable path and `BASE_ORDER_RESULTS.json` for a
fresh output filename; do not overwrite historical receipts. Run from the
repository root with each desired Python interpreter:

```sh
python -B docs/superpowers/reports/issue-1123/experiment.py --hoimin HOIMIN_BINARY --base-order-only --seed 0 --output BASE_ORDER_RESULTS.json
python -m pytest docs/superpowers/reports/issue-1123/test_base_order.py -q
```

The original resource ceilings still apply: one-job plans, 64 executed candidates
per pool, five seconds per fixed check, 45 seconds per planning subprocess and
more than 10 GiB free disk before execution. Each check runs in a fresh Python
process. Both recorded temporary roots were removed. No worker memory-isolation,
full-project benchmark or Rust workspace test result is inferred from these probes.
