# Focused Baseline Timeout Design

## Problem

The focused-mutation workflow starts each package baseline during the mutation
phase, after `RunBudget.may_start_mutation` permits more work. It nevertheless
passes `RunBudget.discovery_timeout` to the baseline command. Once the
discovery deadline has elapsed, that timeout is zero even when the mutation
deadline still leaves substantial time, so the baseline fails immediately and
the run ends as `baseline_failed`.

## Design

Use `RunBudget.mutation_timeout` for each package baseline. This keeps the
timeout source aligned with both the phase that owns the command and the
existing `may_start_mutation` admission check. Do not add a baseline-specific
deadline or change reserve calculations; the existing mutation timeout already
preserves the reporting reserve.

The command construction, timeout classification, run-state transitions, and
per-package baseline cache remain unchanged.

## Verification

Add a workflow regression test whose clock is already beyond the discovery
deadline but still inside the mutation window. Capture the timeout supplied to
the baseline runner and assert that it is positive and equals the mutation
phase timeout. The test must fail against the current discovery-timeout call.

Run the focused budget/reporting tests, the full Python test suite, and targeted
mutation verification for the changed timeout selection.

## Non-goals

- Adding a new baseline deadline or configuration option.
- Changing total-budget allocation or reporting reserve policy.
- Changing baseline failure semantics.
