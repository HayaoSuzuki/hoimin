# Reap After Termination Errors Design

## Context

On cancellation or timeout, `ProcessHandler::run` currently waits for the root
child only when `ProcessSupervisor::terminate` succeeds. If tree termination
returns an error, the live `tokio::process::Child` is dropped without an
explicit wait. On portable Unix, the same failure can leave the process group
and its descendants alive.

This design resolves GitHub Issue #24 and audit finding `RUST-AUDIT-003`. It
does not change the pre-attachment Windows containment window tracked by Issue
#23.

## Decision

Cancellation and timeout use one shared asynchronous cleanup operation. That
operation has three obligations:

1. attempt the ordinary supervisor tree termination;
2. if that attempt fails, start a direct root kill and retry supervisor tree
   termination once; and
3. explicitly wait for the root child to exit, using bounded waits.

The first supervisor termination error remains the primary returned error even
when retry and root reaping succeed. This preserves the operational failure
signal while ensuring an error does not skip cleanup.

The retry is deliberate rather than an unbounded reliability policy. It closes
the transient failure case used by deterministic tests and gives portable
process groups, Linux cgroups, and Windows jobs one final cleanup attempt
without delaying cancellation indefinitely.

## Cleanup Flow

For a successful initial supervisor termination:

```text
terminate tree -> bounded root wait -> return Cancelled or Timeout
```

For a failed initial supervisor termination:

```text
record primary supervisor error
  -> start direct root kill
  -> retry supervisor tree termination once
  -> bounded root wait/reap
  -> return the recorded primary error with any cleanup failures appended
```

Direct root kill and tree retry are both attempted even if one fails. The root
wait is attempted last regardless of earlier cleanup results.

`wait_after_termination` becomes a strict bounded reap helper. It first waits
for the existing post-termination grace period. If that expires, it requests a
root kill and performs one more bounded wait. A timeout or I/O failure from the
second wait is returned as cleanup failure; a kill request alone is not treated
as proof that the root was reaped.

## Error Precedence and Observability

The original `process.resource.terminate` error keeps its existing code,
operation, and leading message. Failures from direct root kill, retrying tree
termination, or bounded root wait are appended to that message in execution
order with explicit cleanup labels.

When the initial supervisor termination succeeds, a root wait failure remains
the returned process error as today. Output draining still runs, and
`combine_process_and_output` continues to prefer the process error over an
output cleanup error.

No cleanup failure is allowed to replace or hide the initial supervisor error.
No successful retry converts the operation into a successful cancellation or
timeout result.

## Deterministic Portable Failure Injection

`PortableBackend` gains a hidden public test constructor,
`for_tests_with_termination_failure()`, whose prepared supervisor fails its
first termination attempt and lets the cleanup retry succeed. The injection
state is shared through an atomic counter so cloning the backend does not
duplicate the one-shot failure budget.

The injection is test-only in intent, does not alter `PortableBackend::new` or
`for_tests`, and is not selectable from CLI configuration. It fails before
signalling the process group, ensuring the regression test exercises the
fallback root kill and supervisor retry rather than merely returning an error
after successful termination.

One-shot injection is sufficient for the behavioral regression. Error
aggregation for persistent cleanup failures is tested at the cleanup helper
boundary with deterministic synthetic failures rather than adding a
production-facing persistent-failure mode.

## Testing

Tests follow red-green order:

- cancellation with the injected first termination failure returns
  `process.resource.terminate`, explicitly reaps the root, and leaves no live
  descendant;
- timeout with the same injection has the same cleanup and error guarantees;
- a unit-level aggregation test proves the first supervisor error remains
  primary and later cleanup failures are appended in order;
- existing successful cancellation, timeout, output-drain, and descendant
  termination tests remain unchanged and pass.

The focused suites are:

```text
cargo test -p hoimin-cli --test process_handler
cargo test -p hoimin-cli --test run_e2e
```

Final verification runs formatting, all-target/all-feature Clippy, and the full
workspace suite after provisioning the worktree's controlled Python
environment.

To improve test quality without a workspace-wide mutation run,
`cargo-mutants` targets only the new cancellation/timeout cleanup helper, the
bounded reap helper, and error aggregation. Every viable mutant in that
limited scope must be caught; survivors require focused behavior tests, while
non-compiling mutants are recorded as inconclusive.

## Non-goals

- Do not change process selection precedence or cancellation/timeout
  classification.
- Do not redesign supervisor types into a general trait abstraction.
- Do not add unbounded termination retries.
- Do not change pre-attachment Windows process containment.
- Do not make portable process supervision a security boundary.
