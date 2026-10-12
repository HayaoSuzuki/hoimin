# Enum state boundaries

## Scope and design

Review baseline: `b2759feb9f03c2c307f335d44e6b3790885046ee`.
Use enums for mutually exclusive states whose associated data must move with
the state. Preserve CLI, JSON, persisted formats, error precedence, and cleanup
ordering. This is an internal refactor, not a performance claim.

Selected boundaries:

- Metrics: running/finished stages and idle/queued/running worker processes.
- Report sequence: not started, running, and finished, with the run identity.
- Report encoding: human, JSON Lines, JSON with its spool, and released JSON.
- Owned report delivery: idle with a handler, running with a join handle, and
  unavailable after a task panic. Cancellation retains the running join handle.
- Python boundary runner: a closed set of result statuses using `StrEnum`,
  retaining the existing JSON strings and raw observation input validation.

Independent observations remain independent: outcome and exit-policy flags can
coexist; managed-root cleanup readiness and cleanup attempts can occur in either
order; worker drop policy differs from cleanup completion; telemetry fields can
arrive together; optional configuration and diagnostic fields are not phases.
Core worker execution already has a phase enum; moving every optional payload
into its many variants would require a separate, higher-risk protocol rewrite.
Python registry modes already use a discriminated `Literal` union. These are
deliberate exclusions, not a requirement to eliminate every bool or string.

## Design self-reviews

1. **Exclusivity:** selected states have a single owner; independent flags remain.
2. **Failure behavior:** retain unavailable writers and released JSON explicitly;
   do not silently turn them into an idle or streaming output state.
3. **Cancellation:** keep the join handle in the driver across cancelled awaits;
   do not move it into a temporary future that could lose ownership.
4. **Compatibility:** retain status strings, error messages and precedence;
   exported output/configuration enums and schemas are unchanged.
5. **Complexity:** prefer private enums with payloads over parallel Options;
   add no generic state-machine framework, dependency or public configuration.

## Implementation plan and self-reviews

1. **Baseline:** run existing Rust and Python behavior checks before edits;
   use them to verify the refactor rather than add tests mirroring variants.
2. **Order:** change metrics, report sequence, encoding, delivery, then Python;
   inspect each diff before the next boundary.
3. **Focused risks:** check rejected metric transitions, report error precedence,
   released spools, writer panic, cancelled delivery and JSON status round trips.
4. **Resources:** use a dedicated target, two build/test workers, no incremental
   artifacts or debug symbols; clean that exact target after final verification.
5. **Completion:** run workspace tests, contracts checks, strict lint and docs
   checks; obtain a fresh independent read-only review, then commit and create
   one cohesive PR. No runtime or public-feature dependency requires a stack.

## Implementation and verification

Pending. Record actual commands, results, five implementation reviews and five
verification reviews here before delivery.
