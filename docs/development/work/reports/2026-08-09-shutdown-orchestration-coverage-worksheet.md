# Shutdown Orchestration Coverage Worksheet

## Contract boundary

This worksheet maps the shutdown audit's premises and observations before the
Lean state is fixed. The owned path begins when the first stop cause is
observed and ends when Hoimin returns after settling or transferring ownership
of process, blocking-I/O, workspace, session, report, and metrics obligations.

Declared behavior comes from test names, typed events, and comments around
`ShutdownBudget`, `drain_processes`, process cleanup, and outer finalization.
Implicit behavior includes select-branch precedence, the lifetime of the
interrupt monitor during finalization, which task owns detached resources, and
whether late failures append to or replace the primary cause.

The Lean representation uses semantic state classes rather than wall-clock
durations, PIDs, paths, or real effect IDs. A strict case must configure the
same premise through the public CLI and observe all compared fields. Existing
private barriers and controlled locks are `internal-fixture`; instruction-level
future boundaries without a public or existing fixture seam are `model-only`.

## Claim-to-evidence matrix

| ID | Claim | Mode | Final classification and evidence | Lean/CLI case |
| --- | --- | --- | --- | --- |
| SHUT-01 | The first shutdown cause and deadline are retained when a later stop arrives | `internal-fixture` | **covered** — `first_cause_is_retained` and `first_deadline_is_not_extended` prove retention for every non-forced model step; `causeWitness` detects overwriting, while `shutdown_budget_first_activation_cannot_be_extended` confirms the production `Option::get_or_insert` path. | `causeWitness` |
| SHUT-02 | A second interrupt forces exit 130 without waiting for a blocked finalizer | `internal-fixture` | **covered** — `forced_interrupt_returns_130`, the detected `forced_wait` broken family, the interrupt unit test, and the retained-SQLite-lock E2E all agree. The Lean terminal state is forced, returned, code 130, and detached when blocking work was pending. | `second_interrupt_blocked_finish` |
| SHUT-03 | No new ordinary process or mutant work starts after stopping | `model-only` | **model-only — no same-premise seam** — the model rejects `startProcess` after a cause and proves a reaped process never runs again. Production `ProcessStartGate` tests linearize the private spawn gate, but the public CLI cannot pause exactly between gate admission and task insertion, so no stronger strict projection is claimed. | bounded explorer plus `reaped_never_runs_again` |
| SHUT-04 | A started process is terminated and reaped at most once | `strict` | **covered** — `StructuralInvariant` bounds reap count, and the generated strict timeout case matches the public CLI: exit 4, incomplete JSON/report session, no live marked descendant, and bounded exit. The first-interrupt strict replay independently produces code 130 with the same liveness result. | `total_timeout_reaps_descendant` |
| SHUT-05 | Process exit versus cancellation retains one terminal process result and required output cleanup | `internal-fixture` | **covered** — the generated event order ends with one reaped process and completed output drain. `cancellation_and_timeout_precede_a_simultaneous_exit`, `successful_process_requires_successful_output_cleanup`, and `process_error_precedes_output_cleanup_error` cover the corresponding private supervisor branches. | `process_exit_vs_cancellation` |
| SHUT-06 | Cleanup, session finish, report, and metrics singleton obligations are dispatched at most once | `model-only` | **model-only — no same-premise seam** — `step_preserves_structural_invariant`, `singleton_dispatches_are_bounded`, and the 20,472-state depth-9 exploration prove all four modeled counters stay at most one. No public output exposes dispatch identity/count for all four layers, so strict equality would overstate observability. | bounded explorer |
| SHUT-07 | Cleanup and drain failures append after the primary failure without replacing its stable class | `internal-fixture` | **covered** — `primary_error_is_retained`, the detected `error_precedence` broken family, the multi-stage generated trace, and `cleanup_failures_preserve_primary_error_and_append_details_in_order` agree. `finish_failed_event_drain` retains the effect failure string before appending drain failure. | `cleanup_failure_after_process_failure` |
| SHUT-08 | A completed component does not regress when cancellation, deadline, or failure arrives late | `model-only` | **model-only — no same-premise seam** — late-cancellation and late-error schedules stay safe, while the `completion_regression` broken family is detected. Session/report commit points have no public pause seam after durable completion and before late cancellation; existing core phase tests remain the closest production evidence. | `session_finish_vs_late_cancellation`, `report_write_vs_late_error` |
| SHUT-09 | Detachment happens only after exclusive resource ownership transfers to the detached task | `internal-fixture` | **covered** — `detached_has_transferred_ownership` and the detected `ownership_loss` family match `context.workspace.take()` plus the cloned process handle before `spawn_blocking`. The owned-close preemption and expired-close unit fixtures passed. | `blocking_completion_vs_detach` |
| SHUT-10 | Returning success implies process, cleanup, session, report, and metrics obligations are settled | `strict` | **covered** — the generated normal case and public CLI match exactly on code 0, complete report/session, no descendant, and bounded exit; the Lean observation additionally records reaped process and complete output/workspace/report/metrics with singleton dispatches. | `normal_completion` |
| SHUT-11 | Duplicate or stale completion notifications are rejected without changing observable state | `model-only` | **model-only — no same-premise seam** — `rejected_preserves_state` proves transactional rejection for every modeled event, and the existing machine oracle covers duplicate/retired/wrong-kind identities. `ShellCompletion` and effect IDs are private channel values, so a public-binary adapter cannot inject the same premise without a production hook. | `rejected_preserves_state` |
| SHUT-12 | A first interrupt leaves a parseable incomplete report and session while reaping descendants | `strict` | **covered** — the generated strict projection matches the public binary field-for-field: exit 130, parseable incomplete report, incomplete SQLite run, stopped descendant, and bounded exit. The existing real-SIGINT E2E passed independently. | `first_interrupt_running` |

## Production branch-order correspondence

- `shell.rs:107` constructs a candidate in `establish_event_shutdown_budget`,
  but `establish_shutdown_budget` installs it with `Option::get_or_insert`.
  Before the call, `active` owns either no budget or the first budget; after it,
  both returned and stored values are the same first cause/deadline.
- `shell.rs:2289–2399` owns both `JoinSet`s, the completion receiver, and
  `in_flight` across `budget.wait`. Accepted buffered completions return blocking
  ownership and decrement `in_flight`; expiry records counts, aborts both sets,
  drains the receiver, and sets `in_flight` to zero without extending the budget.
- `shell.rs:2402` consumes each private `ShellCompletion` once.
  `shell.rs:2438` combines a previously captured effect failure with a later
  drain failure in primary-then-detail order.
- `shell.rs:2479–2558` calls `context.workspace.take()` and clones the process
  handle before detached or bounded blocking cleanup starts. A successful await
  restores the returned workspace; an expired await drops only the join handle,
  leaving the spawned task as the sole workspace owner.
- `shell.rs:2584` moves the metrics collector and output path into one owned
  blocking operation under the same budget. Expiry preserves prior warnings and
  reports incomplete metrics rather than starting a second write.
- `shell.rs:146` retains the interrupt monitor as an owned argument until outer
  finalization resolves; the second-signal path therefore remains live while a
  session/report close is blocked.
- `process/mod.rs:931–1105` fixes result precedence around simultaneous exit,
  cancellation, output cleanup, and cleanup failure. The generated schedules
  abstract those `select!` branches to semantic milestones rather than polling
  or instruction-level timing.

## Executed evidence

All seven requested unit fixtures ran with one exact matching test and passed.
The real-SIGINT incomplete-session fixture, the blocked-session second-SIGINT
fixture, and the total-timeout-plus-grace fixture also each ran once and passed.
The new strict adapter ran all three cases together and separately through
`HOIMIN_SHUTDOWN_ORACLE_CASE`; infrastructure errors and reviewed mismatches
were both empty. The strict signal adapter is executed on Unix; Windows keeps
the schema and projection checks and continues to rely on the existing
`ctrl_c_event` counterpart for real console-control delivery.

## Retained exclusions and promotion rule

The worksheet does not promote a private barrier or injected completion to
`strict`. A model-only result can be promoted only when the same premise and
complete observation become available through public CLI arguments, OS
signals, SQLite locks, filesystem controls, or controlled child processes.
