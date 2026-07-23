# Findings Ledger

## Status vocabulary

- `lead`: requires semantic review
- `accepted`: actionable root cause
- `rejected`: not actionable, with rationale
- `issue_created`: accepted and linked to GitHub

Classification accepts only `confirmed bug`, `high-risk design`, or `maintainability`.
Severity accepts only `P0`, `P1`, `P2`, or `P3`.

| ID | Area | Classification | Severity | Status | Locations | Evidence | Root cause / boundary | Disposition |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| RUST-001 | isolation | high-risk design | P1 | lead | `crates/hoimin-cli/src/process/mod.rs:341`; `crates/hoimin-cli/src/resource/portable.rs:106-121` | `Command::spawn` returns a running child before `PortableSupervisor::attach` assigns it to the kill-on-close Windows Job Object. The source itself documents that the child can execute in this interval. | On Windows portable mode, process creation is not suspended or otherwise atomic with Job Object assignment. A fast child can perform work or spawn descendants before the process tree is placed under the configured lifetime/resource boundary; attach failure only kills the root handle and cannot prove pre-assignment descendants are contained. | Confirm with a Windows stress test using a child that immediately spawns a detached descendant, then design suspended startup/assignment before resuming. |
| RUST-002 | core | maintainability | P3 | lead | `crates/hoimin-core/src/budget.rs:91-93` | `BudgetLedger::reserve` assigns `ReservationId(self.next_id)`, advances with `saturating_add(1)`, then unconditionally inserts into the active-reservation map. No boundary test exercises ID exhaustion. | Once `next_id` reaches `u64::MAX`, the first reservation at that ID leaves `next_id` saturated. A later successful reservation reuses `ReservationId(u64::MAX)` and `BTreeMap::insert` replaces the still-active entry. `reserved()` then undercounts the grants and one release loses the other obligation. The current machine caller creates only one copy reservation per run, so this is dormant there, but the public ledger API does not enforce that bound. | Replace saturation with checked allocation and a typed exhaustion error (or make uniqueness an explicit bounded construction invariant); add a focused boundary unit test that seeds the allocator at `u64::MAX`. |
| RUST-003 | orchestration | high-risk design | P1 | lead | `crates/hoimin-cli/src/process/mod.rs:422-433`; `crates/hoimin-cli/src/process/mod.rs:548-569`; `crates/hoimin-cli/src/resource/portable.rs:125-142` | On cancellation or timeout, `ProcessHandler::run` calls `wait_after_termination` only when `terminate_supervised` succeeds. A supervisor termination error returns directly to the process-result slot; output collection still runs, and the live `Child` is eventually dropped without an explicit wait. Existing cancellation/timeout descendant tests exercise only successful termination. | The failure branch conflates “tree termination was attempted” with “the root no longer needs to be reaped.” `kill_on_drop(true)` is only a best-effort root kill and is not a wait/reap operation; on portable Unix, a process-group kill error can also leave descendants running. This is separate from RUST-001: RUST-001 concerns containment before attach, while this lead begins after successful attach and concerns cleanup/reap after terminate failure. | Inject a portable-supervisor termination failure and assert that the original termination error remains primary, root reaping is attempted, and descendants do not outlive the handler. Then make root kill/wait an unconditional bounded cleanup obligation and append any cleanup failure to the supervisor error. |
| RUST-004 | isolation | high-risk design | P2 | lead | `crates/hoimin-cli/src/workspace/mod.rs:169-192`; `crates/hoimin-cli/src/workspace/mod.rs:310-349`; `crates/hoimin-cli/src/workspace/mutation.rs:14-61`; `crates/hoimin-cli/src/workspace/reset.rs:49-88`; `crates/hoimin-cli/src/workspace/reset.rs:183-194` | `resolve_worker_path` validates existing components with `symlink_metadata`, returns `root.join(path)`, and public `read`/`write`/`remove`/`exists` plus mutation later use ordinary pathname APIs. Reset restore writes and sets permissions through snapshot paths; `remove_any` can inspect a path and then act after a parent swap. Existing tests cover a symlink already present before validation, not replacement between validation and use. | Normal same-worker sequencing does not create this race: mutation runs before its process, and reset runs after termination/reap. Exploitation requires a different worker, a same-UID external actor, or a descendant that escaped containment to rename a checked parent and replace it with a symlink. A later read/write/remove/restore operation can then affect a path outside that worker root. This is a conditional workspace isolation/integrity failure, not privilege escalation or a security boundary. It remains separate from RUST-001/RUST-003, which own containment timing and termination-error reap. | Add a deterministic seam or Linux stress test with one of the stated actors swapping a parent between validation and `write`/`remove`/reset restore, and assert an outside sentinel's contents and permissions remain unchanged. Use root-directory-handle-relative operations that reject symlinks atomically (`openat2`/equivalent per platform), with a portable fail-closed contract. |

## Task 3 core lead disposition

Task 3 retained only `RUST-001` in the CLI isolation area. It produced no
`hoimin-core` lead requiring accepted or rejected classification in Task 4.
Task 5 does not duplicate that attach-time isolation lead. Its `RUST-003` path
starts after successful attachment, when cancellation or timeout encounters a
supervisor termination error and skips the explicit root wait/reap.

## Task 6 isolation lead disposition

Task 6 retains `RUST-004` as the sole new lead. It does not duplicate `RUST-001`:
the latter is the Windows portable spawn-to-Job-assignment interval, while
RUST-004 requires an already materialized worker plus a different worker, same-UID
external actor, or containment-escaped descendant that can race filesystem path
validation against use. The ordinary same-worker mutation/process/reset lifecycle
is sequential. It also does not duplicate `RUST-003`, whose failure path begins
after successful supervisor attachment when termination errors skip an explicit
root wait. RUST-004 is therefore a P2 workspace isolation/integrity design risk,
not a privilege-escalation boundary. The hard Windows backend uses suspended
startup, and its static attach path creates no second RUST-001 lead. Delegated Linux
cgroup v2 and Windows Job Object execution remain platform-limited evidence rather
than new findings.

## Task 5 shell decomposition assessment

`shell.rs` can be split along the requested four responsibilities without changing
the core state-machine protocol, but only as a staged extraction with
characterization tests:

1. Extract **workspace/session adapters** from `ShellContext`, preserving lazy
   session opening, active-candidate insert/remove timing, worker environment
   rewriting, and cleanup retry behavior. Characterize every non-process
   `RunEffect` as exactly one same-ID `RunEvent`.
2. Extract **process preparation and completion** around
   `worker_process_request`, dispatch gates, `spawn_process`, completion
   accounting, and `drain_processes`. Characterize cancellation at each
   prepare/gate/spawn boundary, the `jobs`/`jobs + 1` bounds, one completion per
   accepted process, and unconditional descendant termination plus root reap.
3. Move **effect dispatch** to a dispatcher depending only on those adapters and
   returning typed completions. Characterize output-write failure as
   `EffectFailed` and prove that no subsequently queued state-machine effect is
   executed before that failure is accepted.
4. Leave **run-loop termination and cleanup** as the outer owner of `RunState`,
   cancellation/deadline selection, task draining, handler close, metrics finish,
   and error combination. Characterize primary-error precedence with simultaneous
   drain, workspace-close, process-close, metrics-state, and metrics-write
   failures.

Dependency direction should therefore be `run loop -> dispatcher -> adapters`,
with process completion feeding the run loop through a typed bounded queue;
adapters must not call `transition`, and the dispatcher must not own cleanup
policy. This is a decomposition boundary, not an additional maintainability lead:
the current focused tests already cover successful ordering and boundedness, but
the failure-injection characterization named above is required before moving code.
