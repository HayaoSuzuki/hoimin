# Lean Compound Pattern and Guard Audit

Date: 2026-08-14
Issue: #300, phase 4

## Status

Audit in progress. This report freezes the correspondence premises before the
Lean model and Rust adapter are implemented.

## Claim and boundary

Hoimin must derive a match case's known-import environment from reachable
compound-pattern and guard outcomes. A capture affects routes that reach its
binding point. An earlier pattern failure retains its incoming fact. OR arms
start from the same input and every reachable arm participates in the relevant
meet. A false guard carries the successful pattern bindings and guard writes to
the next case.

Lean proves a reduced two-name environment model. Rust tests observe the real
analyzer at fixed source sites. This audit excludes runtime extraction,
equality and attribute exceptions, parser recovery, arbitrary generated
patterns, nested exit categories covered by phase 2, and `except*` from phase
5.

## Pre-model correspondence worksheet

The internal adapter calls
`binding_flow_marker_snapshot(source, marker)` and compares the complete sorted
known-import fact vector at the selected entry. The public adapter calls
`hoimin_cli::run_with_io`, enables only `type_list_sequence`, and compares the
complete candidate overlapping the unique marker.

| ID | Lean premise | Production premise and marker | Complete observation | Initial mode |
|---|---|---|---|---|
| `or_success_meets_arms` | Both OR arms start from one environment; each success captures `Sequence`; all successes meet | `orSource`, `or_body_marker` | Case-body entry facts | `internal-fixture` |
| `or_failure_meets_arms` | Each arm can fail before its capture; all failures meet | `orSource`, `or_failure_marker` | Following-case entry facts | `internal-fixture` |
| `as_child_failure_precedes_alias` | `[0]` can fail before the AS alias binds | `asSource`, `list[int]` | Following-case entry facts | `internal-fixture` |
| `mapping_child_failure_precedes_rest` | Required key/value checks can fail before `**Sequence` binds | `mappingSource`, `list[int]` | Following-case entry facts | `internal-fixture` |
| `class_early_failure_precedes_capture` | Class test or positional literal can fail before the keyword capture | `classSource`, `list[int]` | Following-case entry facts | `internal-fixture` |
| `false_guard_uses_post_guard` | Pattern captures `Mapping`; false guard assigns `Sequence` | `falseGuardSource`, `false_guard_marker` | Following-case entry facts | `internal-fixture` |
| `compound_preserves_mapping` | AS changes `Sequence`; `Mapping` is unrelated | `preservedSource`, `tuple[Mapping]` | Following-case entry facts | `internal-fixture` |
| `as_failure_public_candidate` | AS child failure retains known `Sequence` | `asSource`, `list[int]` | Count, path, span, operator, original, replacement, symbol | `strict` |
| `mapping_failure_public_candidate` | Mapping child failure retains known `Sequence` | `mappingSource`, `list[int]` | Count, path, span, operator, original, replacement, symbol | `strict` |
| `class_failure_public_candidate` | Class early failure retains known `Sequence` | `classSource`, `list[int]` | Count, path, span, operator, original, replacement, symbol | `strict` |
| `unequal_or_capture_sets` | Two abstract successful arms retain different known facts and meet them | Python rejects unequal OR capture sets before a valid AST exists | Lean witness only | `model-only` |

## Fixed production sources

### `orSource`

```python
from typing import Mapping, Sequence
match value:
    case [0, Sequence] | {"item": Sequence}:
        or_body_marker: list[str]
    case _:
        or_failure_marker: list[str]
```

### `asSource`

```python
from typing import Sequence
match value:
    case [0] as Sequence:
        pass
    case _:
        as_failure_marker: list[int]
```

### `mappingSource`

```python
from typing import Sequence
match value:
    case {"tag": 0, **Sequence}:
        pass
    case _:
        mapping_failure_marker: list[int]
```

### `classSource`

```python
from typing import Sequence
match value:
    case Point(0, tail=Sequence):
        pass
    case _:
        class_failure_marker: list[int]
```

### `falseGuardSource`

```python
from typing import Mapping, Sequence
match value:
    case [Mapping] if ((Sequence := local_sequence) and False):
        pass
    case _:
        false_guard_marker: list[str]
```

### `preservedSource`

```python
from typing import Mapping, Sequence
match value:
    case [0] as Sequence:
        pass
    case _:
        preserved_mapping_marker: tuple[Mapping]
```

The strict rows use `target.py`. A present candidate has operator
`type_list_sequence`, original `list[int]`, replacement `Sequence[int]`, and no
symbol. Lean computes its byte span from the owned source.

## Observation and error contract

The corpus accepts only `strict`, `internal-fixture`, `model-only`, and
`infrastructure-error`. It owns all eleven identities and source bytes. The
Rust parsers reject unknown fields, duplicate identities, crossed
family/observation pairs, changed sources, non-unique markers, unsorted facts,
and incomplete candidate records.

Fixture creation, parsing, marker selection, timeout, RSS termination, panic,
nonzero CLI exit, stderr on success, malformed JSON, and source/span
disagreement are infrastructure errors. They support no semantic conclusion.

## Mismatch ledger

No production comparison has run yet.

## Lean model evidence

The structural model keeps reachable successes and failures as separate lists.
Sequential composition retains prefix failures and runs the next pattern only
from prefix successes. AS and mapping-rest captures therefore occur after their
children. OR concatenates every arm's reachable outcomes before `meetAll?`
summarizes them. Guard evaluation receives only the successful-pattern meet.

The proof module exports ten checked results for prefix failure retention, AS,
mapping rest, class prefix capture, OR success and failure inclusion, two-arm
known-fact retention, false-guard state, unrelated-name preservation, and
unreachable outcomes. The proof consumer imported and checked all ten names.

## Resource ledger

Every Lean and Lake command uses a 20-second deadline, 786,432 KiB combined
root-and-descendant RSS cap, and 250 ms samples. Lake builds use one job. The
fixed audit reports zero generated depth, event alphabet, explored states, and
transitions.

| Command | Child exit | Elapsed | Peak RSS |
|---|---:|---:|---:|
| `lake -Kjobs=1 build HoiminOracle.CompoundPatternGuardModel` | 0 | 3,529 ms | 681,664 KiB |
| `lake -Kjobs=1 build HoiminOracle.CompoundPatternGuardProofs` | 0 | 2,725 ms | 592,208 KiB |
| `lake env lean /tmp/hoimin-compound-pattern-proof-consumer.lean` | 0 | 576 ms | 661,408 KiB |
