# Issue #126 Async Filesystem Dispatch Design

## Goal

Move worker workspace and candidate-spool filesystem operations off the Tokio dispatch task, while preserving the run state machine as the sole owner of transition ordering and allowing independent workers to perform I/O concurrently.

## Current Problem

The run loop starts process effects in tasks, but directly awaits `CreateWorker`, `ReadCandidate`, `ApplyMutation`, `ResetWorker`, and `VerifyOriginals`. These handlers use blocking filesystem APIs. One slow operation therefore prevents the loop from dispatching another ready worker, accepting a completed process, or selecting its deadline and interrupt branches until that operation returns.

## Considered Approaches

### Await each `spawn_blocking` call

This keeps the runtime thread available, but the run-loop future still waits for each operation before dispatching the next effect. It improves scheduler health without fixing cross-worker throughput or completion handling. Rejected.

### Dispatch tasks behind one `WorkspaceHandler` mutex

This lets the main loop remain responsive, but every workspace operation still serializes behind a global lock. Candidate replay can overlap, but worker apply/reset/create cannot. It does not satisfy the issue's disjoint-worker concurrency premise. Rejected.

### Transfer worker ownership to independent tasks

At dispatch, the main task removes the affected `WorkerWorkspace` from `WorkspaceHandler` and moves it into an owned blocking operation. The completion carries both the typed `RunEvent` and the workspace state to restore. Different workers have no shared mutable workspace object, while `WorkspacePlan` is shared through `Arc` and already synchronizes its reservation bookkeeping. Selected.

## Architecture

### Workspace task boundary

`WorkspaceHandler` stores its preflight plan as `Arc<WorkspacePlan>` and exposes shell-internal preparation and completion APIs. Preparation validates the effect against main-task-owned maps, removes any affected worker or pending-cleanup state, and produces an owned `WorkspaceTask`. `WorkspaceTask::execute` performs only synchronous work and returns a `WorkspaceTaskCompletion` containing the event and one of these updates:

- insert a newly created worker;
- return an applied or successfully reset worker to the active map;
- retain a reset worker in pending cleanup when discard cleanup fails;
- no worker for verification or a reset whose discard cleanup succeeded.

Preparation failures remain immediate typed `EffectFailed` events. Every prepared task must yield exactly one completion, even if the blocking task panics; join failures become `shell.blocking_io` failures with the original effect ID.

### Candidate replay boundary

`ReadCandidate` becomes an owned blocking task because it only needs its immutable spool reference and offset. Its `CandidateLoaded` result is accepted on the main task, which updates `active_candidates` before calling `transition`.

### Run-loop scheduling

The run loop recognizes the five blocking-I/O effects and schedules them through `tokio::task::spawn_blocking` without awaiting them inline. An I/O `JoinSet` is separate from process tasks so process-concurrency metrics continue to count only subprocesses. Both task classes send `ShellCompletion` values through the existing bounded channel.

The loop tracks process and I/O completions separately, but total completion in-flight accounting still includes both. It accepts returned workspace state before applying the corresponding machine transition. Cancellation and fatal failure drain both task sets; because a running `spawn_blocking` closure cannot be aborted safely, shutdown waits for it and restores or cleans up its returned state before closing the handler.

### Direct effect API

`execute_effect` remains behaviorally compatible for existing tests and callers by executing its five filesystem effects synchronously against the borrowed context. It does not transfer a worker across an await point: dropping the direct-effect future therefore cannot detach a blocking task and permanently remove that worker from the context. The production run loop alone uses the owned, non-awaiting scheduler path to gain concurrency and runtime responsiveness.

## Correctness and Failure Handling

- Only the main async task mutates `WorkspaceHandler` maps, `active_candidates`, and `RunState`.
- A worker can have at most one owned workspace task because removing it makes a duplicate preparation fail as `WorkerMissing`; machine ordering should prevent that failure in valid runs.
- The initial reservation and copy allowance become visible under the same plan-state lock, so concurrent first worker creation cannot charge against an unpublished zero allowance.
- Different workers may execute concurrently; operations for the same worker cannot.
- A successfully cleaned pending worker is dropped before its replacement is materialized, releasing its plan worker slot first.
- Reset preserves its current discard behavior and combined restore/cleanup error text.
- Create-worker rollback remains owned by `WorkspacePlan::create_worker` and its synchronized `PlanState`.
- Candidate order and offset validation remain in `CandidateStore::replay_one`.
- Deadline, cancellation, interrupt, typed effect failure, and metrics ordering remain on the dispatch task.
- Preflight and final cleanup stay serial because they define global workspace lifetime boundaries and are not listed in issue #126.

## Verification

Tests will prove:

- the five intended variants are classified as blocking I/O and no other effect is;
- two owned blocking operations can start before either is released;
- a Tokio timer fires while a blocking operation is held;
- worker state returns after successful apply/reset and reset failure preserves existing discard semantics;
- candidate results update the active-candidate map only when accepted by the main task;
- blocking-task join failure keeps the original effect ID and stable failure code;
- cancellation/failure drains both task classes without leaking worker reservations;
- shutdown drain waits for a held workspace task, restores its returned worker state, and leaves process metrics unchanged;
- existing shell, workspace, state-machine, randomized-order, contracts, and platform suites remain green.

An ignored benchmark will compare four fixed-duration blocking operations run serially and through the production scheduler. The deterministic concurrency test, rather than elapsed time, is the regression gate; timing is supporting evidence only.

## Scope

This change does not optimize candidate replay format (#127), worker materialization strategy (#128), line indexing (#129), or make blocking filesystem operations cancellable after the OS call has begun.
