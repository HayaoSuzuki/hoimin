# Lean Multiple Handler Join Audit

Date: 2026-08-14
Issue: #300, phase 3

## Claim and boundary

The audited claim is that ordinary `except` handlers form an ordered selection
frontier. Every reachable selected handler contributes its categorized exits
after its own target cleanup exactly once; only its non-selected remainder
reaches the next handler. The final join meets exactly reachable fallthrough
paths, preserves abrupt categories, and retains one final unhandled remainder
as terminate.

The reduced model makes selection and non-selection explicit. It does not
model runtime exception subclass matching, handler-type side effects,
exception object lifetime, `except*`, compound patterns, or generated syntax
exploration. Nested match, loop consumption, and `finally` are existing
phase-1/2 contracts and are not re-audited here.

## Pre-model correspondence worksheet

Every implementation-facing row uses an exact Python source, a unique marker,
and a complete observation. The internal adapter is
`binding_flow_try_exit_snapshot(source, marker)` and reads the production
collector's normalized `fallthrough`, `breaks`, `continues`, and `terminates`
vectors. It does not reproduce routing or cleanup.

| ID | Lean representation | Production configuration | Observation | Initial mode |
|---|---|---|---|---|
| `two_handlers_disagree_fallthrough` | Initial `Sequence`/`Mapping`; normal and first handler retain both; second handler shadows `Sequence`; route all reachable selections | Two ordinary handlers whose suites produce the three stated fallthrough facts | Full try-exit snapshot at `# two_handlers_disagree_fallthrough` | `internal-fixture` |
| `three_handlers_preserve_mapping` | Three reachable selected fallthrough exits with different `Sequence` facts and common known `Mapping` | Three ordinary handler suites mutate or reimport only `Sequence` | Full try-exit snapshot at `# three_handlers_preserve_mapping` | `internal-fixture` |
| `per_handler_cleanup_categories` | Two selected handlers target `Sequence`; one falls through and one terminates; cleanup maps every selected category independently | Two `except ... as Sequence` handlers, with fallthrough and return exits after local reimports | Full try-exit snapshot at `# per_handler_cleanup_categories` | `internal-fixture` |
| `different_handler_break_continue` | First selected handler breaks; second selected handler continues; both targets are cleaned | A loop containing two target-bearing handlers with distinct abrupt statements | Full inner try-exit snapshot at `# different_handler_break_continue` | `internal-fixture` |
| `unhandled_remainder_terminates` | Final remainder becomes exactly one terminate state; selected handlers have no terminate exits | The try body contains exactly one `raise`; both handlers fall through | Full try-exit snapshot at `# unhandled_remainder_terminates` | `internal-fixture` |
| `all_handlers_preserve_public_candidate` | Every normal or selected fallthrough path retains known `Sequence` | Two handlers and the normal path all retain or reimport `Sequence` before the following annotation | Complete public plan candidate overlapping `Sequence[int]` | `strict` |
| `one_handler_shadows_public_candidate` | One reachable selected fallthrough shadows `Sequence`, so the meet does not retain it | The second of two handlers assigns `Sequence = object` before the following annotation | Complete public plan observation with no candidate overlapping `Sequence[int]` | `strict` |

The exact source premises are fixed as follows and will be emitted from Lean.

### `two_handlers_disagree_fallthrough`

```python
from typing import Mapping, Sequence
try:  # two_handlers_disagree_fallthrough
    work()
except FirstError:
    from typing import Sequence
except SecondError:
    Sequence = object
after_disagreement: Sequence[int]
preserved_mapping: Mapping[str, int]
```

### `three_handlers_preserve_mapping`

```python
from typing import Mapping, Sequence
try:  # three_handlers_preserve_mapping
    work()
except FirstError:
    Sequence = object
except SecondError:
    from typing import Sequence
except ThirdError:
    pass
after_three: Mapping[str, int]
```

### `per_handler_cleanup_categories`

```python
def run(flag):
    from typing import Mapping, Sequence
    try:  # per_handler_cleanup_categories
        work()
    except FirstError as Sequence:
        from typing import Sequence
    except SecondError as Sequence:
        from typing import Sequence
        return flag
```

### `different_handler_break_continue`

```python
from typing import Mapping, Sequence
while active:
    try:  # different_handler_break_continue
        work()
    except FirstError as Sequence:
        from typing import Sequence
        break
    except SecondError as Sequence:
        from typing import Sequence
        continue
```

### `unhandled_remainder_terminates`

```python
def run():
    from typing import Mapping, Sequence
    try:  # unhandled_remainder_terminates
        raise UnknownError
    except FirstError:
        pass
    except SecondError:
        pass
```

The one terminate state is attributable to the unhandled remainder because the
fixture has exactly one raising try-body exit and neither handler can
terminate. The implementation observation remains the complete terminate
vector; it does not attach invented provenance to that vector.

### `all_handlers_preserve_public_candidate`

```python
from typing import Sequence
try:
    work()
except FirstError:
    from typing import Sequence
except SecondError:
    from typing import Sequence
all_handlers_preserve: Sequence[int]
```

### `one_handler_shadows_public_candidate`

```python
from typing import Sequence
try:
    work()
except FirstError:
    from typing import Sequence
except SecondError:
    Sequence = object
one_handler_shadows: Sequence[int]
```

Public rows use the public `plan` path with only the exact type-list operator.
They compare candidate count, root-relative path, byte start and length,
operator, original, replacement, and symbol. Fixture creation, parsing,
marker lookup, command launch, timeout, abnormal exit, stderr on success,
manifest decoding, and span validation failures are `infrastructure-error` and
make no semantic claim.

## Ownership decisions

- Lean owns the explicit reachability and handler-fold contract.
- Rust owns the conservative syntactic handler paths exposed by
  `AnnotationCollector::visit_try`.
- A different-premise observation begins as `model-only`; it is promoted only
  after the exact premise is configurable and observable.
- A semantic difference is classified as confirmed bug, specification
  ambiguity, model defect, or infrastructure error before any correction.
- Production semantics change only after a focused same-premise failing Rust
  regression is retained.

## Handler-fold theorem surface

The imported model defines `HandlerStep`, `HandlerRoute`, `cleanSelected`,
`routeHandler`, `routeHandlers`, and `finishHandlers`. The imported proof
module establishes:

1. once the remainder is `none`, every suffix handler is unreachable;
2. a reachable step merges exactly its cleaned selected exits;
3. target cleanup reaches the selected fallthrough, break, continue, and
   terminate category;
4. target cleanup preserves the other representative name;
5. a fact retained by the outgoing reachable-state meet occurs on every
   reachable fallthrough path;
6. selected abrupt exits retain their category; and
7. one final remainder produces exactly one terminate state.

The first proof consumer failed as required because
`MultipleHandlerJoinProofs.olean` did not exist. The first proof build then
reported an authoring defect: the extension declaration
`NestedTryFlow.Exits.categoryStates` required its fully qualified namespace,
and the `Exits.states` append membership required three nested left injections.
A guarded declaration probe confirmed the exact names before the minimal
syntax/proof-shape correction. No theorem premise or model transition was
weakened.

Initial guarded measurements, retained as authoring-stage evidence:

| Command | Exit / reason | Elapsed ms | Peak RSS KiB |
|---|---|---:|---:|
| `lake -Kjobs=1 build HoiminOracle.MultipleHandlerJoinModel` | 0 / `child_exit` | 3,219 | 668,000 |
| `lake -Kjobs=1 build HoiminOracle.MultipleHandlerJoinProofs` | 0 / `child_exit` | 2,435 | 652,720 |
| `lake env lean /tmp/hoimin-multiple-handler-proof-consumer.lean` | 0 / `child_exit` | 2,735 | 633,520 |

All used the fixed 20-second deadline, 786,432 KiB RSS ceiling, and 250 ms
sampling. The first sandboxed guard launch returned exit 126 / `monitor_error`
after 34 ms because process-tree inspection was denied; the unchanged command
was rerun with monitoring permission. This was infrastructure evidence, not a
Lean failure.
