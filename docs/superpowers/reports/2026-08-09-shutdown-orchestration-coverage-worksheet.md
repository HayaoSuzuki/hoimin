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

| ID | Claim | Mode | Existing evidence | Missing observation | Planned case |
| --- | --- | --- | --- | --- | --- |
| SHUT-01 | The first shutdown cause and deadline are retained when a later stop arrives | `internal-fixture` | `shutdown_budget_first_activation_cannot_be_extended`, `shutdown_budget_total_timeout_deadline_is_anchored_to_run_deadline`, and `repeated_cancellation_does_not_extend_paused_materialization_shutdown` exercise `establish_shutdown_budget` | The cause, chosen exit, and terminal persistence have not been compared with one generated trace — missing — Task 6 | `first_interrupt_precedes_deadline` |
| SHUT-02 | A second interrupt forces exit 130 without waiting for a blocked finalizer | `internal-fixture` | `second_signal_forces_130_without_waiting_for_first_consumer` covers monitor escalation; `second_sigint_forces_130_while_session_finish_is_blocked` exercises the real CLI with a retained SQLite writer lock | No Lean-owned terminal expectation or exact mismatch classification — missing — Tasks 4–6 | `second_interrupt_blocked_finish` |
| SHUT-03 | No new ordinary process or mutant work starts after stopping | `internal-fixture` | Core tests `deadline_and_cancellation_stop_scheduling_new_mutants`, `stop_signals_do_not_reopen_a_finished_run`, and `stop_signals_do_not_reopen_a_pending_final_report` cover `RunState`; `ProcessStartGate` unit tests linearize cancellation and spawn | The outer-shell task-set observation is not represented in the existing Lean oracle — missing — Tasks 2–4 | `post_stop_spawn_is_rejected` |
| SHUT-04 | A started process is terminated and reaped at most once | `strict` | `total_timeout_cancels_and_reaps_descendants_before_cleanup` and `first_sigint_finishes_a_parseable_incomplete_session` use the public binary and real child processes; process unit tests cover wait-after-termination | A generated expectation does not yet compare process terminality and descendant liveness — missing — Task 5 | `total_timeout_reaps_descendant` |
| SHUT-05 | Process exit versus cancellation retains one terminal process result and required output cleanup | `internal-fixture` | `cancellation_and_timeout_precede_a_simultaneous_exit`, `successful_process_requires_successful_output_cleanup`, and `process_error_precedes_output_cleanup_error` exercise the process supervisor's select and combine paths | The event-order expectation and complete error observation are not Lean-owned — missing — Tasks 4 and 6 | `process_exit_vs_cancellation` |
| SHUT-06 | Cleanup, session finish, report, and metrics singleton obligations are dispatched at most once | `model-only` | Core tests cover cleanup/final-output identity; `failed_event_drain_expiry_keeps_effect_code_and_message`, `expired_shutdown_skips_metrics_with_an_incomplete_warning`, and session tests cover selected components | No single public observer exposes all four dispatch counts; a bounded common invariant is missing — missing — Tasks 2–4 | `singleton_dispatches` |
| SHUT-07 | Cleanup and drain failures append after the primary failure without replacing its stable class | `internal-fixture` | `cleanup_failures_preserve_primary_error_and_append_details_in_order`, `wait_error_cleanup_appends_cleanup_failure_to_the_primary_failure`, `shutdown_error_appends_a_drain_failure_after_the_primary_failure`, and `failed_event_drain_expiry_keeps_effect_code_and_message` | Cross-layer order from process failure through cleanup/session/report is not represented by one trace — missing — Tasks 4 and 6 | `cleanup_failure_after_process_failure` |
| SHUT-08 | A completed component does not regress when cancellation, deadline, or failure arrives late | `model-only` | `stop_signals_do_not_reopen_a_finished_run`, `stop_signals_do_not_reopen_a_pending_final_report`, and `late_stop_preserves_final` in the prior Lean oracle cover core phases | Session/report/metrics outer-finalization completion has no same-premise pause seam after commit — missing — Tasks 2–4 and 6 | `session_finish_vs_late_cancellation` |
| SHUT-09 | Detachment happens only after exclusive resource ownership transfers to the detached task | `internal-fixture` | `expired_final_close_detaches_cleanup_without_extending_the_wait`, `successful_run_outer_close_uses_the_original_deadline_and_detaches_cleanup`, and `detached_cleanup_owns_the_workspace_after_expiry` exercise `context.workspace.take()` and cloned process ownership | The common ownership-transfer invariant and broken lost-owner witness are missing — missing — Tasks 2–4 | `blocking_completion_vs_detach` |
| SHUT-10 | Returning success implies process, cleanup, session, report, and metrics obligations are settled | `strict` | Normal E2E and metrics-sidecar tests observe complete report/session/metrics; `finalize_run_with_shutdown` and `finalize_metrics_with_shutdown` own the outer close | No isolated Lean-generated normal terminal observation is replayed through the real binary — missing — Task 5 | `normal_completion` |
| SHUT-11 | Duplicate or stale completion notifications are rejected without changing observable state | `model-only` | The existing state-machine oracle covers duplicate, retired, wrong-kind, and unknown effect completions; `accept_drained_completion` consumes buffered shell completions during drain | Private `ShellCompletion` identities cannot be injected from an external adapter, and the outer projection is missing — missing — Tasks 2–4 and 6 | `duplicate_completion` |
| SHUT-12 | A first interrupt leaves a parseable incomplete report and session while reaping descendants | `strict` | `first_sigint_finishes_a_parseable_incomplete_session` or `first_ctrl_c_event_finishes_a_parseable_incomplete_session` exercises the public binary, real signal, JSON parsing, SQLite state, and descendant liveness | The observation is not generated from the new shutdown model and compared field-for-field — missing — Task 5 | `first_interrupt_running` |

## Production correspondence anchors

- `crates/hoimin-cli/src/interrupt.rs`: `InterruptMonitor::spawn`, `first`, and
  `forced` own signal forwarding and escalation.
- `crates/hoimin-cli/src/shell.rs`: `establish_shutdown_budget`,
  `establish_event_shutdown_budget`, `shutdown_expiry_error`,
  `drain_processes`, `accept_drained_completion`,
  `finish_failed_event_drain`, `close_context_resources`,
  `detach_context_resources`, `finalize_run_with_shutdown`,
  `finalize_metrics_with_shutdown`, and `finish_with_interrupt_monitor` own the
  cross-layer lifecycle.
- `crates/hoimin-cli/src/process/mod.rs`: supervised termination, wait/reap,
  output drain, and cleanup-error combination own the child lifecycle.
- `crates/hoimin-core/src/machine.rs`: `RunState` and `transition` own effect
  identity and the cleanup/session/final-output protocol inside the shell.
- `crates/hoimin-cli/tests/run_e2e.rs`: real-signal, retained SQLite lock,
  descendant PID marker, and parseable-output fixtures expose public behavior.

## Initial exclusions and promotion rule

The worksheet does not promote a private barrier or injected completion to
`strict`. A row starts as `model-only` unless the same premise and complete
observation are already public. Task 6 may promote a row only after recording
the real configuration and observation path; otherwise it closes the row with
an explicit same-premise limitation.
