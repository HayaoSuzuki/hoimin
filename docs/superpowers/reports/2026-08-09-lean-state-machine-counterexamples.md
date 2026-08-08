# Lean State-Machine Counterexample Ledger

## Classification vocabulary

- `confirmed bug`: the public Rust state machine violates an established lifecycle invariant.
- `specification ambiguity`: the model and implementation differ without an owned semantic decision.
- `model defect`: the approved abstraction does not represent the public contract correctly.
- `infrastructure error`: the corpus or adapter could not execute the semantic comparison.

| Case | Classification | Expected | Actual | Impact | Model limitation | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| `cleanup_is_emitted_once` | confirmed bug | After `cancel` schedules one cleanup, a `deadline` received while that cleanup is pending preserves the pending cleanup and emits no effect. | The second stop retires the first cleanup and emits a second cleanup; phase and pending count remain `cleaning` and one, hiding the identity replacement unless effect emissions are compared. | The original cleanup completion becomes `machine.effect.retired`; two cleanup handlers can be dispatched for the same lifecycle obligation, and whichever old completion arrives can turn orderly early-stop cleanup into a machine failure. | The adapter collapses the required `RunStarted` acknowledgement into the semantic first stop and uses the real no-copy-grant early-stop path. It excludes filesystem execution and timing, but both cleanup effects and their effect identities come from the public `transition` API. | Keep the Lean case strict. Add a focused Rust regression for `cancel -> RunStarted ack -> Cleanup pending -> deadline`, then make repeated stop signals during `RunPhase::Cleaning` idempotent without retiring or replacing the pending cleanup. |

## Minimal reproduction

```console
HOIMIN_ORACLE_CASE=cleanup_is_emitted_once \
  cargo test -p hoimin-core --test lean_oracle oracle_correspondence -- --exact --nocapture
```

Lean expects the second step's `emitted` field to be `[]`; Rust observes `["cleanup"]`. All other
fields match. The divergence begins in `crates/hoimin-core/src/machine.rs` where both stop branches
call `retire_pending()` before considering that `RunPhase::Cleaning` already owns a cleanup effect.
With no copy grant, each branch then calls `cleanup_effects()` and allocates a replacement cleanup.

## Reviewed matches

The other twelve initial corpus cases match the public Rust state machine. They cover unknown,
wrong-kind, duplicate, and retired completion rejection; ordinary-work suppression after stop;
cleanup-before-final ordering; final-output uniqueness; and late-stop idempotence while final output
is pending or the run is finished.

There were no infrastructure errors.
