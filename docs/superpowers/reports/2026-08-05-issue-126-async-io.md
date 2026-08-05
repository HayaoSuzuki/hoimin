# Issue #126: Async Filesystem Effect Dispatch

## Outcome

`CreateWorker`, `ReadCandidate`, `ApplyMutation`, `ResetWorker`, and `VerifyOriginals` no longer execute blocking filesystem calls on the async run-loop task. Ready operations are sent to Tokio's blocking pool, independent workers can overlap, and their typed events return through the existing bounded completion channel.

The run state machine, workspace maps, active-candidate map, transition ordering, and process metrics remain owned by the main async task.

The public direct-effect helper retains synchronous borrowed-context execution. This avoids transferring a worker across an await point where cancellation could detach the blocking task and lose the returned state; production run-loop dispatch uses the owned concurrent path.

## Design

Two simpler designs were rejected:

- awaiting each `spawn_blocking` call would free a Tokio worker thread but still prevent the run loop from dispatching the next ready worker;
- scheduling behind one global workspace mutex would keep the loop responsive but serialize all worker I/O.

The implemented design transfers the affected `WorkerWorkspace` out of `WorkspaceHandler` when an effect is dispatched. The owned `WorkspaceTask` runs on the blocking pool and returns both its `RunEvent` and the workspace state that the main task must restore. Different workers therefore share no mutable workspace object during I/O. The preflight plan is an `Arc<WorkspacePlan>` whose existing mutex and atomics protect reservation and copy accounting during concurrent worker creation.

Candidate replay is naturally owned by its spool reference and offset. Its result updates `active_candidates` only after the main task receives the completion.

## Correctness Boundary

- A worker removed for an in-flight task cannot be dispatched a second workspace operation; valid machine ordering prevents this, and an invalid duplicate observes `WorkerMissing` instead of aliasing the worker.
- A malformed duplicate completion is rejected without replacing the worker already registered for that slot.
- Apply success or failure returns the worker to the active map.
- Reset success returns the restored worker; reset failure retains the existing discard behavior, including pending cleanup when discard cleanup also fails.
- Create-worker rollback remains in `WorkspacePlan::create_worker`.
- Initial reservation binding and allowance publication occur under the same plan-state lock, preventing a concurrent creator from charging against an unpublished allowance.
- A cleaned pending worker is dropped before replacement materialization so its plan worker slot is released first.
- Blocking-task join failures retain the original effect ID under `shell.blocking_io`.
- Process tasks and I/O tasks use separate `JoinSet`s, so process-concurrency telemetry is unchanged.
- Cancellation and fatal failures drain both task sets and accept returned workspace state before final workspace cleanup.
- `spawn_blocking` cannot interrupt an OS call that has already started. Shutdown therefore waits for that call; it no longer stalls Tokio's async worker or the run loop before cancellation is selected.

Completion payloads are boxed because they can own a complete worker. This preserves the existing 14 KiB upper bound on the CLI entrypoint future; the unboxed intermediate revision grew the future to 14,752 bytes and was rejected by the existing heap regression.

## Deterministic Regression Coverage

The overlap test schedules two controlled blocking effects. Each reports that it started and then waits for its own release signal. The test requires both start reports before releasing either operation, so a serial scheduler deadlocks until the one-second guard fails while the concurrent scheduler completes immediately.

A separate test holds a blocking operation while a Tokio timer and yield complete, proving that filesystem work is not running on the async executor thread. A controlled blocking panic verifies typed join-failure identity.

Shutdown coverage holds a real owned apply task at the blocking boundary, confirms drain remains pending, then releases it and verifies that the worker returns with its mutation while process metrics remain empty.

Existing E2E coverage also exercises jobs=4 worker isolation, the cross-process barrier, bounded completion queues, Ctrl+C, total timeout, sessions, candidate replay, mutation/reset, metrics, and final cleanup through the new dispatch path.

## Benchmark

Command, run five times in release mode:

```console
cargo test --release -p hoimin-cli benchmark_blocking_io_dispatch -- --ignored --nocapture
```

Each run compares four fixed 50 ms blocking operations awaited serially with the same four operations scheduled through the production blocking-I/O task and completion-channel path. Timing is supporting scheduler evidence; the channel-controlled overlap test is the regression gate.

| Run | Serial | Concurrent | Observed maximum I/O in flight |
|---:|---:|---:|---:|
| 1 | 215 ms | 55 ms | — |
| 2 | 216 ms | 55 ms | — |
| 3 | 213 ms | 55 ms | — |
| 4 | 219 ms | 54 ms | — |
| 5 | 213 ms | 55 ms | — |
| Post-review validation | 214 ms | 55 ms | 4 |
| Median of the five recorded runs | 215 ms | 55 ms | — |

The median scheduler workload fell by 74.4%, a 3.91x speedup. The first five timing runs predated the live-counter correction, so their maximum-I/O cells are deliberately left blank. The post-review validation measures the maximum from the benchmark's live I/O counter rather than inferring it from the scheduled operation count. The small excess over the theoretical 200/50 ms reflects timer and task scheduling overhead.

## Review Corrections

Independent review found and the final implementation corrects three ownership races: initial allowance publication after unlocking plan state, delayed drop of a cleaned pending worker before replacement, and direct-effect cancellation after worker extraction. Deterministic regressions cover the first two; direct calls now have no await boundary after worker access begins. Classification covers all five filesystem variants, and the shutdown test exercises real workspace completion acceptance.

## Scope Boundary

Preflight and final cleanup remain serial because they establish global workspace lifetime boundaries. This change does not alter candidate spool format (#127), worker materialization strategy (#128), or line/column indexing (#129).

## Verification

Executed on 2026-08-05:

```console
cargo fmt --check
cargo test --workspace
cargo test --workspace --all-features -- --test-threads=1
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

Focused shell, run E2E, workspace handler, workspace recovery, blocking-panic, timer-responsiveness, deterministic-overlap, and CLI-future-size tests also passed. The worktree used a local symlink to the repository's controlled `.venv`; it was not committed.
