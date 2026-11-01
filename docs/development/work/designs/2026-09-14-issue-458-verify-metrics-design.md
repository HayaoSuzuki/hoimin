# Issue 458: Verify operational metrics

## Contract and choice

Add `verify PLAN --metrics PATH` to explicit IDs and strict/diverse top selections. Resolve relative destinations against the invocation directory, as `run` does. Keep the saved plan, candidate identities, execution limits, fingerprint and result schema unchanged. Reuse the shell metrics collector, destination authorization, atomic finalizer and warnings.

A new output-only field on `VerifyArgs` carries a normalized UTF-8 path. Both public dispatch routes attach it to the successfully prepared `VerifiedPlan` before entering the existing shell. Keep the existing library preparation functions source-compatible. Extract the existing run path conversion into a shared CLI helper so error semantics cannot diverge.

Alternatives considered: extending every preparation API adds unrelated output state to validation and breaks callers; saving metrics in the plan incorrectly persists an operational destination. Attaching it at dispatch is the smaller change and preserves validation behavior.

## Failure and observation boundaries

Manifest, selection, source, fingerprint and rediscovery failures produce no sidecar because execution has not begun. Once the shell begins, baseline failures, termination and metrics write failures follow the existing run behavior. Confirmed source/fingerprint collisions are rejected before baseline; unresolved destinations warn without changing the mutation exit. Metrics describe shell execution stages, not earlier plan validation time.

## Verification

Exercise explicit IDs and strict/diverse top selections with multiple candidates, validate `RunMetrics`, compare actual candidate IDs/verdicts and inherited limits with a no-metrics control. Cover missing parent write warning, selected-source collision, failed manifest and failed baseline; test relative paths and help through the real CLI. JSON and JSONL stdout remain parseable. Native Linux/Windows enforcement is outside local macOS evidence.
