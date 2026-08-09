# Lean workspace lifecycle audit report

## Outcome

No production-reachable workspace ownership bug was confirmed in the audited
scope. Lean-generated expectations matched all three public
`WorkspaceHandler` schedules and both same-premise crate-private task
schedules.

The audit did expose one real defense gap at a different premise:
`WorkspaceHandler::handle_cleanup` accepts cleanup while a separately owned
`WorkspaceTask` still holds a worker generation. That direct combination can
release the reservation while the generation remains task-owned. The
production shell does not permit this schedule: after a stop or failure it
drains every owned blocking completion before appending the state machine's
new finalization and cleanup effects. The case is retained in the Lean corpus
as `model-only`, not counted as a Rust mismatch, and documented below as a
hardening concern.

No production fix or follow-up bug issue was opened from this audit. If another
caller of the crate-private task API is introduced, task ownership must either
move into `WorkspaceHandler` or cleanup must receive an explicit task-empty
capability.

## Audited contract

The model owns one central invariant: each logical worker generation is in at
most one of active handler state, task/completion ownership, pending cleanup,
or absence. It additionally checks:

- prepare success transfers ownership out of active/pending state;
- rejected prepare operations are transactional;
- apply/reset completions return the same generation or explicitly make it
  absent/pending;
- duplicate registration cannot overwrite an existing generation;
- cleanup failure preserves retryable ownership;
- cleanup success requires no task-owned generation, clears handler ownership,
  and advances a lifecycle epoch;
- an old-epoch completion cannot alter active or pending ownership;
- released cleanup state has no modeled owned generation.

Filesystem syscall atomicity, allocator failure, poisoned locks, crashes
inside one filesystem call, and deliberately corrupt private values were not
modeled. Shutdown-grace expiry while a `spawn_blocking` operation continues is
owned by the separate shutdown audit and was not re-proved here.

## Correspondence results

| Case | Mode | Result | Implementation premise |
| --- | --- | --- | --- |
| `public_create_cleanup` | `strict` | match | Public preflight, grant, create, and cleanup |
| `public_duplicate_create_is_atomic` | `strict` | match | Duplicate public create preserves the registered root |
| `public_apply_reset_round_trip` | `strict` | match | Public synchronous apply/reset preserve generation identity |
| `owned_apply_returns_generation` | `internal-fixture` | match | Crate-private prepare/execute/accept with an affine apply task |
| `owned_reset_returns_generation` | `internal-fixture` | match | Crate-private prepare/execute/accept with an affine reset task |
| `cleanup_rejects_inflight_task` | `model-only` | different premise, not compared | Direct handler cleanup while a task is externally owned |

The JSONL parser accepts only the four audit classifications `strict`,
`internal-fixture`, `model-only`, and `infrastructure-error`; rejects unknown
fields, events, modes, workers, generations, verdicts, duplicate case IDs, and
schedule/observation disagreement; and supports focused replay through
`HOIMIN_WORKSPACE_ORACLE_CASE`.

Infrastructure/setup/parser failures are reported separately from semantic
mismatches. Rust adapters translate generation roles to observed worker-root
identity and never derive the expected postcondition.

## Different-premise counterexample

The shortest retained direct-handler schedule is:

1. preflight and reserve workspace copy;
2. create worker `w0` generation `g0`;
3. `prepare_apply_task`, which removes `g0` from `workers` and transfers it to
   the affine task;
4. call `handle_cleanup` before executing/accepting that task.

Lean expects step 4 to reject with `workspace.worker.busy`, keep `g0`
task-owned, and keep the reservation unreleased. Direct Rust observation was:

```text
verdict=accepted
active=[]
pending=[]
task_owned=[w0:g0]
released=true
```

This is not a model defect: it correctly detects that cleanup would cross an
ownership boundary. It is not a production correspondence failure either,
because the model event omits the shell's required drain premise.

The relevant implementation path is:

- `prepare_apply_task` removes the active worker and embeds it in
  `WorkspaceTask` (`workspace/mod.rs`);
- `handle_cleanup` can inspect only `workers` and `pending_cleanup`, not tasks
  held by its caller;
- on an external stop/failure, the shell calls `drain_processes` before
  `effects.extend(produced)`;
- drained `WorkspaceTaskCompletion` values are accepted back into the handler
  before the produced finalize/cleanup effects can be prepared.

Core's `retire_pending` retires logical effect IDs, but ownership acceptance is
intentionally performed before the retired event is offered back to the state
machine. Thus the worker returns to the handler even though the logical result
is discarded during stopping.

Severity is currently informational/latent: no production call path bypassing
the drain was found. The risk becomes material if a second scheduler or direct
crate caller combines task preparation and cleanup. The recommended future
repair boundary would be generation/epoch tracking inside `WorkspaceHandler`
or an explicit cleanup token proving there are no outstanding workspace tasks,
not a one-off platform condition.

## Lean evidence

Pinned tools:

```text
Lean 4.32.2 (f3b06c7)
Lake 5.0.0
rustc 1.97.1
cargo 1.97.1
```

The imported proof module contains no `sorry`, `admit`, or custom axioms. It
proves structural consequences of `safe`, exact cleanup success/failure
behavior, transactional busy cleanup, stale-completion preservation of active
and pending maps, completed-absent acceptance, and a generic arbitrary-trace
lifting theorem from a local preservation premise.

The concrete transition's full preservation claim is checked by bounded BFS,
not presented as an unbounded theorem. This distinction is intentional: the
list-based ownership representation makes uniqueness substantive, while a
short proof obtained by encoding ownership in a uniqueness-by-construction map
would test less. Exploration checks every outgoing event before state
deduplication.

Bounded exploration result:

```text
depth=8 alphabet=12 states=221 transitions=2040
```

No unsafe state was reachable under the correct transition within that bound.
The search uses stable event order and breadth-first layers, so the first
counterexample for a safety property is shortest by transition count among the
enumerated alphabet.

## Sensitivity ledger

| Fault family | Retained witness | Detection |
| --- | --- | --- |
| Atomicity/transactionality | Duplicate task ID after another worker has already transferred to `t0`; broken prepare removes `w0` before rejecting | Rejection-state equality fails; length 5 |
| Uniqueness/idempotency | Preflight, install `w0:g0`, broken second install of `w0:g1` | Worker-owner `Nodup` fails; length 3 |
| Boundary/precedence | Prepare `w0:g0`, broken cleanup advances epoch with task outstanding, completion returns after cleanup | Released-with-owner fails at cleanup and old generation resurrection remains observable; full witness length 6 |

Every corpus generation, check, or stats invocation first verifies all three
fixed sensitivity witnesses. A missing detector exits nonzero before corpus
output.

## Corpus and cost

The Lean-owned corpus contains 6 cases and is 4,493 bytes. Generation is
deterministic; `--check` compares exact bytes. Cached local measurements on an
Apple arm64 host were:

| Command | Wall time | Result |
| --- | ---: | --- |
| `lake build` | 2.45 s | pass, 18 jobs |
| `lake exe generate_workspace -- --stats` | 0.19 s | pass |
| `lake exe generate_workspace -- --check corpus/workspace-lifecycle.jsonl` | 0.20 s | fresh |
| strict Rust adapter | 3.41 s | 2 tests pass |
| internal Rust adapter | 3.19 s | 1 test pass |

The executable BFS and sensitivity gate are not imported by
`HoiminOracle.lean`; ordinary library builds import only model, proof, and case
definitions.

## Limitations and independent evidence

The corpus adapter does not forge a reset failure plus discard-cleanup failure
to manufacture pending ownership. Existing workspace recovery tests remain the
implementation evidence for that filesystem-dependent branch. Likewise, the
adapter does not construct duplicate or cloned completions: Rust completion
values are affine, and the existing duplicate-slot unit test independently
checks that a synthetic competing active worker is preserved.

There is no controlled public hook that pauses apply/reset after ownership
transfer while exercising the complete CLI stop path. Shell ordering was
audited from the concrete event loop and its existing owned-workspace shutdown
drain test. This is source and internal-fixture evidence, not a new strict CLI
corpus replay. A future pause hook at the workspace task boundary would allow
that row to move from reviewed implementation correspondence to strict
executable correspondence without changing the formal model.

## Verification commands

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/WorkspaceModel.lean
lake env lean HoiminOracle/WorkspaceProofs.lean
lake build
lake exe generate_workspace -- --stats
lake exe generate_workspace -- --check corpus/workspace-lifecycle.jsonl

cd ../..
cargo test -p hoimin-cli --test lean_workspace_oracle
cargo test -p hoimin-cli lean_workspace_internal_oracle_exposes_task_ownership --lib
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
uv run --frozen python -m unittest tests/test_skills.py
git diff --check
```

The final full-workspace and CI results are recorded in the PR. The audit's
claim remains limited to the premises and bounds above; Lean does not prove the
Rust implementation.
