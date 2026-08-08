# Lean State-Machine Counterexample Ledger

## Classification vocabulary

- `confirmed bug`: the public Rust state machine violates an established lifecycle invariant.
- `specification ambiguity`: the model and implementation differ without an owned semantic decision.
- `model defect`: the approved abstraction does not represent the public contract correctly.
- `infrastructure error`: the corpus or adapter could not execute the semantic comparison.

| Case | Classification | Expected | Actual | Impact | Model limitation | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| `cleanup_is_emitted_once` | confirmed bug | After `cancel` schedules one cleanup, a `deadline` received while that cleanup is pending preserves the pending cleanup and emits no effect. | The second stop retired the first cleanup and emitted a second cleanup; phase and pending count remained `cleaning` and one, hiding the identity replacement unless effect emissions were compared. | The original cleanup completion became `machine.effect.retired`; two cleanup handlers could be dispatched for the same lifecycle obligation, and whichever old completion arrived could turn orderly early-stop cleanup into a machine failure. | The adapter collapses the required `RunStarted` acknowledgement into the semantic first stop and uses the real no-copy-grant early-stop path. It excludes filesystem execution and timing, but both cleanup effects and their effect identities come from the public `transition` API. | Resolved. `transition` now treats a stop received in `RunPhase::Cleaning` with `stop_requested` already set as an idempotent no-op. `lean_oracle_regression_cleanup_is_emitted_once` retains the exact schedule and asserts that the original cleanup ID remains pending and unretired. |

## Minimal reproduction

```console
HOIMIN_ORACLE_CASE=cleanup_is_emitted_once \
  cargo test -p hoimin-core --test lean_oracle oracle_correspondence -- --exact --nocapture
```

Lean expects the second step's `emitted` field to be `[]`; Rust observes `["cleanup"]`. All other
fields match. The divergence begins in `crates/hoimin-core/src/machine.rs` where both stop branches
call `retire_pending()` before considering that `RunPhase::Cleaning` already owns a cleanup effect.
With no copy grant, each branch then calls `cleanup_effects()` and allocates a replacement cleanup.

The repair adds one guard at the public transition boundary. It deliberately requires both
`RunPhase::Cleaning` and the existing `stop_requested` flag: a first stop that arrives during a
normal, non-stopped cleanup is not silently discarded. Focused machine tests and the strict
single-case adapter pass after the change.

## Reviewed matches

The other twelve initial corpus cases and the added accepted-result interleaving case match the
public Rust state machine. They cover unknown,
wrong-kind, duplicate, and retired completion rejection; ordinary-work suppression after stop;
cleanup-before-final ordering; final-output uniqueness; accepted-result retention across
completion/cancellation; and late-stop idempotence while final output is pending or the run is
finished.

There were no infrastructure errors.
