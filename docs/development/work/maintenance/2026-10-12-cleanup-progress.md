# Cleanup progress at the shell boundary

This record covers #836, stacked above #837. It narrows the finalization change
to the representation of two cleanup-progress pairs. Other measurements,
quiescence observations and owned cleanup records retain their existing roles.

## Design self-review

1. Enumerated every assignment and read of execution/delivery attempted and cleaned flags.
2. Confirmed every cleaned assignment follows an attempted assignment in its path.
3. Distinguished local attempt progress from DiskLifecycle admission: a rejected lifecycle request must still suppress a second local attempt.
4. Kept execution and delivery roots independent because their cleanup barriers and lifetimes differ.
5. Compared a large finalization extraction, a new protocol state machine and a three-state replacement; chose the last for a smaller safety review surface.

## Plan self-review

1. Recorded the old reachable pairs before editing and mapped each to one variant.
2. Kept every state-update point before/after its existing operation.
3. Simplified only equivalent attempted/cleaned predicates; no lifecycle event moves.
4. Selected existing clean, failed, deferred, duplicate-request, interruption and final-output tests.
5. Kept safety/measurement evidence separate and required source comparison plus final contracts/whole-stack checks.

## State correspondence

| Old `(attempted, cleaned)` | New progress | Meaning |
| --- | --- | --- |
| `(false, false)` | `NotAttempted` | No attempt has been admitted locally |
| `(true, false)` | `Attempted` | An attempt began; success is not established |
| `(true, true)` | `Cleaned` | The original success branch established cleanup |

`(false, true)` is not produced by the original code and is no longer representable.
`Attempted` also covers a lifecycle rejection before filesystem cleanup, failed
or deferred cleanup, and unproven quiescence. It does not assert that deletion
ran. `Cleaned` tracks the same host-observed success as the old flag, even when
a later lifecycle/report operation fails. DiskLifecycle remains responsible for
the protocol and safety barriers; this enum does not authorize deletion.

Four booleans become two three-state values, reducing the represented joint
progress combinations from 16 to 9. This is a small local reduction, not a claim
that all finalization complexity is removed. There is no new scheduler, API,
resource policy, persisted schema, dependency or version-floor change.

The runtime-lifecycle knowledge and compatibility policy were consulted. Since
event order, deletion conditions, ownership, deadlines and error evidence remain
unchanged, the existing formal model needs no new transition or proof claim.

## Implementation self-review

1. Compared all six attempt assignments and four success assignments with the original positions.
2. Compared the four outer no-retry predicates against all three reachable old pairs.
3. Compared report release and removed-byte reporting predicates against the same correspondence.
4. Checked failed/deferred and unproven-quiescence paths retain `Attempted`, their records and janitor handling.
5. Checked the complete diff: no resource operation, lifecycle event, deadline or error-order change; no old flag identifier remains.

## Testing self-review

1. Baseline cleanup selection passed: 60 tests.
2. Post-change cleanup selection passed: the same 60 tests.
3. Checked the three reachable-state predicate truth tables, including attempted-but-not-clean suppression.
4. Checked existing cleanup tests cover failed/deferred outcomes, secondary evidence, identity failure and cleanup-ready markers.
5. Selected the full shell tests and whole-stack contracts/workspace checks to cover interruption, final output and deadline paths; results follow in the final validation record.

No test that merely repeats enum variants was added. The reduction follows from
the type's three variants and the source correspondence; behavior is checked by
the existing failure-injection and host tests.

The full shell test selection passed: 106 tests, with two existing ignored tests.
Its coverage includes final report admission, cleanup failures, unsafe final
close, interruption, shutdown deadlines and secondary lifecycle evidence.
These are Windows host results; other OS branches remain for CI.
