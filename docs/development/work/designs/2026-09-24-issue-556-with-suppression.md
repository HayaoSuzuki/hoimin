# Issue 556: import facts after context-manager suppression

The analyzer must emit a typing mutation only if every modeled path reaching the
annotation retains the same typing import. A context manager can turn a body
exception into normal continuation. The existing `visit_with` only forwards body
fallthrough and enables implicit exception collection indirectly through finally.
Consequently a call before an import incorrectly establishes a definite import.

## Design

Enable implicit exception tracking locally while visiting every synchronous or
asynchronous with body, restoring the enclosing tracking flag afterward. Intersect
normal completion with explicit and implicit exception states that any entered
manager might suppress. Keep exceptions available to enclosing handlers/finally
because arbitrary managers can decline suppression. Do not infer behavior from a
manager's spelling, including `suppress`, custom managers, and dynamic factories.

Separate explicit raises from return terminations in `ControlFlowExits`; preserve
that category through suites, loops, handlers, and finally. Existing test-only
oracle projections may still expose both as the legacy `terminate` category.
Only raises enter the suppressed successor, never a successful return, break, or
continue. Exceptions from evaluating a return value remain suppressible.

Process multiple with items in order. Failure entering the first manager cannot
be suppressed by itself. Failure evaluating/entering later items can be suppressed
by an earlier entered manager. Target binding runs after entry, so target failures
are suppressible even for the first item. Preserve conservative partial-target
invalidation. An inner exit failure can be suppressed by an outer manager; a
single manager's own exit failure cannot become its own normal successor.

The import-failure and arbitrary dynamic-hook exclusions remain unchanged. Thus
import-before-call and normal-import controls remain eligible. This is conservative
static flow analysis, not an interpreter or a proof of arbitrary Python execution.

## Alternatives

Recognizing only `contextlib.suppress` misses custom managers and rebinding.
Merging all abrupt exits would incorrectly allow continuation after return,
break, and continue. Treating every with entry as suppressible loses the required
normal-import positive control. Distinct raised exits and ordered manager entry
provide the necessary distinction without adding a manager type system.

## Validation

Unit fixtures cover call ordering, explicit/bare raise, multiple items, custom and
async managers, nested finally/handlers, and return/break/continue boundaries.
Public CLI plan replays the five committed Lean corpus fixtures and checks exact
candidate counts; CPython 3.14 executes both hazard outcomes. A public CLI run of
the issue reproduction must report zero killed mutants. Broader Rust tests and
existing flow-oracle tests guard category preservation; fmt and clippy guard style.
