# hoimin through the C4 model

hoimin is a local mutation-testing tool for Python projects. A human or an
external coding agent selects code, inspects mutation candidates, runs a test
command against modified copies, and uses the results to improve tests. The
shipped application is one Rust executable; its core library and I/O handlers
run inside that executable.

This source-based description was checked on 2026-10-19 against
[`b65f62834b35a9ff85795b041e9687b21e5c51ba`](https://github.com/HayaoSuzuki/hoimin/tree/b65f62834b35a9ff85795b041e9687b21e5c51ba).
It describes the implemented architecture. The language-specific CLI separation
proposed in #743 is not implemented here. Links to source files identify the
implementation to reread when updating these views. No runtime or platform test
result is implied.

## Reading and editing the views

Read levels 1 and 2 for the system boundary, level 3 for execution responsibilities,
and level 4 for the core transition API. The final sequence diagram explains how
those responsibilities cooperate during a successful run.

C4 distinguishes software systems, runtime/data containers, components inside a
container, and code. A **container** here means an application or data store;
Rust crates and Tokio tasks do not each become containers. See the official
[C4 container definition](https://c4model.com/abstractions/container) and
[diagram levels](https://c4model.com/diagrams).

The diagrams use ordinary Mermaid flowcharts, a class diagram and a sequence
diagram to express C4 views. C4 is [notation independent](https://c4model.com/diagrams/notation).
The [tooling guide](https://c4model.com/tooling) distinguishes drawing separate
views from generating views from a shared model: this document takes the former
approach, so repeated names and boundaries must be reviewed together. The
Mermaid blocks are the editable source; there are no separately maintained image
copies or a Structurizr model.

Open this Markdown file in a Mermaid-capable preview, or copy a fenced block
into a Mermaid renderer. The existing documentation site does not publish this
page directly; its development-guide link opens the repository source. Viewing
or editing the text does not require adding a runtime dependency to hoimin.

## Level 1 — System context

Scope: hoimin as one software system. Repository files and persistence are
expanded at level 2. The external agent is a caller, not an embedded LLM.

```mermaid
---
title: hoimin - C4 level 1 system context
---
flowchart LR
    H["Human maintainer<br/>[Person]<br/>Chooses intent and reviews test changes"]
    A["External coding agent<br/>[External system]<br/>Edits tests and invokes local tools"]
    S["hoimin<br/>[Software system]<br/>Plans and executes bounded Python mutation tests"]
    T["Project test toolchain<br/>[External system]<br/>Runs the user-selected test command"]
    G["Git<br/>[External system]<br/>Supplies changed-file and line information"]
    H -->|Directs and reviews| A
    H -->|Invokes CLI and reads results| S
    A -->|Invokes CLI and reads structured results| S
    S -->|Runs baseline and mutant test commands| T
    S -->|Queries changes when requested| G
```

Legend: each box states its C4 type and responsibility. Arrows are directed,
labelled interactions, not a chronological sequence. External means outside
hoimin's implementation, even when the software runs on the same computer.

The project toolchain may be Python and pytest, but the command is supplied by
the caller. The Rust analyzer uses Ruff's parser in-process; it does not invoke
a Python/LibCST analyzer service. Git is needed for Git-based selection, not for
every run. Package registries, GitHub CI and the documentation website belong to
distribution and development workflows and are outside this runtime view.

Sources: [product direction](../product-principles.md),
[CLI dispatch](../../crates/hoimin-cli/src/lib.rs),
[Git selection](../../crates/hoimin-cli/src/target/git.rs),
[analyzer](../../crates/hoimin-cli/src/analyzer/mod.rs).

## Level 2 — Containers and local data

Scope: the CLI runtime and data it owns or consumes. Each store denotes a
logical portion of the local filesystem, not a server or a compulsory file for
every command. Session and review databases have distinct schemas and purposes.

```mermaid
---
title: hoimin - C4 level 2 containers and local data
---
flowchart LR
    U["Human or external agent<br/>[Caller]<br/>Controls commands and output redirection"]
    P[("Project checkout<br/>[External data store]<br/>Filesystem: original sources and tests")]
    T["Project test toolchain<br/>[External application]<br/>Caller-selected test executable"]
    G["Git<br/>[External application]<br/>Git executable: change selection"]
    subgraph SYS["hoimin — software system boundary"]
        C["hoimin CLI<br/>[Application container]<br/>Rust / Tokio: analysis and execution"]
        W[("Execution storage<br/>[Data container]<br/>Managed directories: snapshot, workers and spools")]
        D[("Delivery storage<br/>[Data container]<br/>Managed directories: JSON spool")]
        F[("Exchange artifacts<br/>[Data container]<br/>Local files: plans, reports and metrics")]
        DB[("Session database<br/>[Data container]<br/>Optional SQLite: results and resume")]
        R[("Review database<br/>[Data container]<br/>Optional SQLite: assessment history")]
    end
    U -->|CLI arguments and environment| C
    C -->|stdout, stderr and exit status| U
    C -->|Read selected sources and protected inputs: filesystem I/O| P
    C -->|Query changes: subprocess arguments and pipes| G
    G -->|Read repository metadata and working tree: filesystem I/O| P
    C -->|Create, patch, restore and reclaim: filesystem I/O| W
    C -->|Spool and release report records: filesystem I/O| D
    C -->|Read plans and saved reports; write optional metrics: filesystem I/O| F
    U -->|Save plan/report stdout with shell redirection| F
    C -->|Persist or resume runs: embedded SQLite API| DB
    C -->|Read or record assessments: embedded SQLite API| R
    C -->|Spawn, wait and stop: OS process APIs; collect output via pipes| T
    T -->|Run with a worker copy as cwd: filesystem I/O| W
```

Legend: rectangles are applications/callers, cylinders are data stores, and the
large boundary groups hoimin's application and logical stores. Arrows describe
operations initiated by their source; reads return data implicitly. They do not
assert that every command touches every store. `cwd` is a process's working
directory; JSONL is one JSON record per line; SQLite is embedded, not a database
server.

`hoimin-cli` depends on `hoimin-core` and `hoimin-python`; `hoimin-python` also
depends on `hoimin-core`. All three crates are linked into the CLI container.
The CLI, analyzer corpus tests and Python fuzz target use the same analysis
crate API. Tokio tasks, blocking worker threads, the disk monitor and session
dispatcher share the CLI process. The test command creates external process
trees. Linux resource setup can invoke a private launcher mode of the same
executable; it is not an additional long-running service.

Execution and delivery storage have separate ownership and cleanup conditions.
A worker is a reusable copy/slot, not an independently deployed worker service.
Workspace preflight builds a shared disk snapshot from which workers are created
and restored. Reports normally go to stdout; the caller may save that stream as
a file. Metrics use a separately configured destination. Session persistence is
opt-in; human review storage does not alter mutation outcomes or scores.

Plan and run-report readers accept their current schemas. Session storage
initializes fresh databases at its current schema and rejects unsupported
versions instead of migrating their tables. Reusing results from a compatible
incomplete run is a separate session policy, not an old-schema adapter.

Sources: [binary entry point](../../crates/hoimin-cli/src/main.rs),
[workspace members](../../Cargo.toml),
[analysis crate API](../../crates/hoimin-python/src/lib.rs),
[fuzz dependencies](../../fuzz/Cargo.toml),
[shell roots and setup](../../crates/hoimin-cli/src/shell.rs),
[owned workspace storage](../../crates/hoimin-cli/src/workspace/owned.rs),
[report delivery](../../crates/hoimin-cli/src/report/delivery.rs),
[session](../../crates/hoimin-cli/src/session/mod.rs),
[session schema](../../crates/hoimin-cli/src/session/schema.rs),
[review](../../crates/hoimin-cli/src/review/mod.rs),
[metrics destination](../../crates/hoimin-cli/src/metrics_destination.rs).

## Level 3 — Components of the CLI execution path

Scope: the `run` path and execution after `verify` preparation, inside the CLI
container. Boxes group cohesive Rust modules by responsibility. This is a
runtime collaboration view, not the complete Rust import graph. Internal
relationships use Rust calls, futures and typed values, not network protocols.

```mermaid
---
title: hoimin CLI - C4 level 3 execution components
---
flowchart LR
    subgraph CLI["hoimin CLI — container: Rust executable / Tokio"]
        ENTRY["Command dispatch and preparation<br/>[Component: Rust]<br/>lib / cli / plan: settings and checked plans"]
        SH["Shell coordinator<br/>[Component: Rust / Tokio]<br/>shell: effects, completions and ownership"]
        CORE["Execution policy<br/>[Component: Rust]<br/>hoimin-core: transitions, budgets and admission"]
        TARGET["Target discovery<br/>[Component: Rust]<br/>target: filesystem and Git selectors"]
        ANA["Candidate acquisition<br/>[Component: Rust]<br/>CLI analyzer: source validation and candidate spool"]
        PY["Pure Python analysis<br/>[Component: Rust / Ruff]<br/>hoimin-python: typed candidates and exception index"]
        WS["Workspace ownership<br/>[Component: Rust / filesystem]<br/>workspace: copies, mutation and cleanup"]
        PROC["Process execution<br/>[Component: Rust / Tokio]<br/>process: commands, output and drain"]
        RES["Platform resource control<br/>[Component: Rust / OS APIs]<br/>resource: backend selection and control"]
        REP["Report delivery<br/>[Component: Rust / serde]<br/>report: encoding and writer ownership"]
        SES["Session persistence<br/>[Component: Rust / rusqlite]<br/>session: persistence and resume ownership"]
    end
    ENTRY -->|Pass prepared config or checked plan| SH
    SH -->|Apply accepted run events and disk lifecycle observations| CORE
    CORE -->|Return state, requested effects and admission decisions| SH
    SH -->|Resolve target requests| TARGET
    SH -->|Analyze source and replay candidate requests| ANA
    ANA -->|Pass source and share candidate payload budget| PY
    SH -->|Perform workspace effects and owned-root cleanup| WS
    SH -->|Dispatch baseline/mutant requests and await drain| PROC
    SH -->|Initialize and close the selected backend| RES
    PROC -->|Spawn and control process trees| RES
    SH -->|Deliver output events and await flush| REP
    SH -->|Dispatch persistence and resume operations| SES
```

Legend: all boxes are in-process components. Arrows identify the main request or
result direction. Handler completions return to the shell; those return edges
are omitted except at the core boundary. Stores and external programs are shown
at level 2. The entry box groups command-front-end responsibilities for this
view; `plan` is not used by ordinary `run`.

The core decides what should happen next. It cannot start a process, write a
report or remove a directory. The shell performs those effects and decides when
a completion can be accepted. A finished task and an accepted core event are
separate facts. `DiskLifecycle` governs safe admission; `CleanupProgress` tracks
local cleanup attempts and cannot authorize removal.

The analyzer, target discovery and workspace use shared path/source policies
without moving filesystem I/O into core. Low-level readers and artifact
validation are omitted from this diagram to keep the execution boundary visible.

The CLI analyzer fixes its root, operators, profile and candidate limit when
constructing `AnalyzerHandler`. File requests contribute to one accumulated
candidate spool until finalization. The `hoimin-python` API returns typed
candidates and diagnostics directly; JSONL is a spool format, not a transport
between analysis components. Candidate payload leases follow allocations through
generation and conversion, including work whose async waiter has detached.
This logical payload budget does not cover all source, AST or index memory.

`WorkspaceHandler` keeps registered workers in one map: a `WorkerSlot` is either
active or pending cleanup. Owned create/reset tasks carry workspace ownership
while detached from the handler. Shared operation code applies the same creation
and restoration policy to synchronous calls and owned tasks. Copy admission
checks precede snapshot writes and continue while bytes are copied.

Sources: [pure transition](../../crates/hoimin-core/src/machine.rs),
[core dependencies](../../crates/hoimin-core/Cargo.toml),
[disk lifecycle](../../crates/hoimin-core/src/disk.rs),
[shell module contract](../../crates/hoimin-cli/src/shell.rs),
[target](../../crates/hoimin-cli/src/target/mod.rs),
[CLI analyzer acquisition](../../crates/hoimin-cli/src/analyzer/mod.rs),
[pure Python API](../../crates/hoimin-python/src/lib.rs),
[candidate payload ownership](../../crates/hoimin-python/src/analyzer/payload.rs),
[workspace](../../crates/hoimin-cli/src/workspace/mod.rs),
[workspace operations](../../crates/hoimin-cli/src/workspace/operation.rs),
[copy admission](../../crates/hoimin-cli/src/workspace/copy.rs),
[process](../../crates/hoimin-cli/src/process/mod.rs),
[resource](../../crates/hoimin-cli/src/resource/mod.rs),
[report](../../crates/hoimin-cli/src/report/mod.rs),
[session dispatcher](../../crates/hoimin-cli/src/session/mod.rs).

### Configuration and checked-plan handoff

Run and plan arguments share mutation options and normalization. CLI parsing
produces `RunConfig<EnvironmentRequest>`; preparation captures the requested
inherited environment and resolves fingerprint inputs to produce
`RunConfig<EnvironmentSnapshot>`. The root is stored in `selection`. Environment
typing guarantees the capture state; common configuration semantics still have
an explicit `validate()` API used by normalization and persisted-plan admission.

`DiscoveryConfig` owns the plan/verify analyzer timeout. A saved `PlanConfig`
combines discovery settings with the execution template; discovery-only settings
are absent from `RunLimits` and the execution resume fingerprint. Ordinary `run`
uses its total execution deadline for analysis.

Verification checks the manifest and current inputs, then constructs a
`VerifiedPlan` containing configuration, protected paths and a `RunSelection`.
The selection binds execution membership/order to immutable verification
provenance. `VerifiedPlan::execute` consumes that handoff and owned output
writers; the shell does not accept a separately attached selection description.

Sources: [CLI normalization](../../crates/hoimin-cli/src/cli.rs),
[configuration types](../../crates/hoimin-core/src/config.rs),
[environment states](../../crates/hoimin-core/src/fingerprint_env.rs),
[preparation and execution](../../crates/hoimin-cli/src/shell.rs),
[checked plan](../../crates/hoimin-cli/src/plan.rs),
[selection and provenance](../../crates/hoimin-core/src/machine/selection.rs),
[resume fingerprint](../../crates/hoimin-core/src/resume.rs).

### Process request identity

The core fixes required identity when it creates an effect:
`RunBaseline(RunProcess<BaselineContext>)` requires worker and run ID;
`RunMutant(RunProcess<MutantContext>)` also requires mutant ID. A standalone
`RunProcess<ProbeContext>` carries no workspace identity. Purpose is not stored
as another independently editable flag inside these requests.

The shell derives worker dispatch and the worker environment from that typed
identity. The process runner lowers it to one private context enum for tracing,
completion worker tags and mutant output policy. Cancellation gates, process
ownership and metrics admission remain separate responsibilities. See
[process effects](../../crates/hoimin-core/src/effect.rs),
[process runner](../../crates/hoimin-cli/src/process/mod.rs) and
[typed process requests](../development.md#typed-process-execution-requests).

### Command paths outside the execution loop

These are branches of the same CLI application, not additional containers.

| Command | Main path | Test processes / persistent effects |
| --- | --- | --- |
| `run` | CLI config → shell → core/effect handlers | Runs baseline and mutants; optional session and metrics |
| `plan` | `plan::create` → targets and source fingerprints → CLI analyzer / `hoimin-python` → ranking | Writes a plan to stdout; does not run baseline/mutant tests |
| `verify` | `prepare_verify_selection` → `VerifiedPlan::execute` → shell | Rechecks plan/current inputs, then executes the selected candidates |
| `verify --dry-run` | Checked plan → preview | Writes a preview without baseline/mutant execution |
| `progress` | Validated saved reports → comparison → rendering | Reads reports; no mutation test execution |
| `review list/show/record` | Plan, optional report and current source identity → review store | Reads assessments or records revisioned assessments separately from run results |
| `completions` | CLI definitions → completion script writer | Writes a shell completion script |

Shared [artifact readers](../../crates/hoimin-cli/src/artifacts/mod.rs) open
plan/report inputs as regular files and provide bounded reads where required.
[Run-report decoding](../../crates/hoimin-cli/src/artifacts/run_report.rs) checks
event structure, summary consistency and candidate identity for progress and
review. Commands retain their own format, size and usability rules; the review
input size limit is not a universal limit on progress reports or source reads.
See [dispatch](../../crates/hoimin-cli/src/lib.rs),
[plan and VerifiedPlan](../../crates/hoimin-cli/src/plan.rs),
[progress](../../crates/hoimin-cli/src/progress/mod.rs), and
[review](../../crates/hoimin-cli/src/review/mod.rs).

## Level 4 — Selected code inside execution policy

Scope: a small code view of `hoimin-core::transition`, rather than an exhaustive
class diagram. Rust structs and enums are represented as UML-style classes.
`Transition` represents a free function, not a class in the implementation.
Only selected fields and enum variants are shown; variant payloads are omitted.
Composition edges show where selection and worker state are owned.

```mermaid
---
title: hoimin-core - C4 level 4 transition types
---
classDiagram
    direction LR
    class RunState {
        <<Rust struct>>
        -RunConfig config
        -RunPhase phase
        -RunSelection selection
    }
    class RunSelection {
        <<Rust struct>>
        -SelectionState state
    }
    class WorkerState {
        <<Rust enum>>
        Idle
        Ready
        Running
        Persisting
        Finishing
        Resetting
    }
    class RunEvent {
        <<Rust enum>>
        StartRequested
        BaselineFinished
        MutantFinished
        WorkerReset
        OutputEmitted
        EffectFailed
        CancellationRequested
        DeadlineReached
    }
    class RunEffect {
        <<Rust enum>>
        ResolveTargets
        CreateWorker
        RunBaseline
        AnalyzeFile
        ApplyMutation
        RunMutant
        ResetWorker
        EmitOutput
        Cleanup
    }
    class EffectId {
        <<Rust newtype>>
        +u64 value
    }
    class Transition {
        <<Rust free function>>
        transition(state, event)
    }
    class MachineError {
        <<Rust enum>>
    }
    RunState "1" *-- "1" RunSelection : selection and provenance
    RunState "1" *-- "*" WorkerState : keyed by worker ID
    Transition ..> RunState : consumes and returns on success
    Transition ..> RunEvent : consumes an event
    Transition ..> RunEffect : returns zero or more effects
    Transition ..> MachineError : returns on rejected transition
    RunEffect ..> EffectId : request payload has an ID
    RunEvent ..> EffectId : completion payload has an ID
```

Legend: solid diamond edges denote ownership; dashed arrows are code dependencies,
not temporal messages. `EffectId` is a tuple newtype; `value` is a display label
for its `u64` field. Completion events identify an effect, while stop/start
events do not all carry effect IDs.
The actual signature is
`transition(RunState, RunEvent) -> Result<(RunState, Vec<RunEffect>), MachineError>`.

`RunState` keeps execution state private. Effects describe operations; handlers
perform them. The transition checks whether a completion belongs to a pending
effect and is valid for the current state. This code boundary supports tests
without requiring every state transition to start an OS process.

`RunSelection` distinguishes all-candidate, explicit-ID and ordered execution.
Ordered execution separates candidate collection from dispatch. `WorkerState`
owns the candidate or result needed by its phase, rather than tracking phase and
payload in independent fields. `MutationOutcome` distinguishes executed, reused
and not-run results; status, elapsed time and output availability are derived
from that evidence. Diagnostics do not determine the outcome.

The pending-effect registry owns expected completions. For output effects,
`PendingCompletion::Output` also owns the continuation to take after
`OutputEmitted`. Delivery acknowledgement therefore controls release of the
worker's result and dispatch of its reset. This is separate from the shell's
process, task and filesystem ownership.

Sources: [RunState, transition and MachineError](../../crates/hoimin-core/src/machine.rs),
[RunEffect and EffectId](../../crates/hoimin-core/src/effect.rs),
[RunEvent](../../crates/hoimin-core/src/event.rs),
[RunSelection](../../crates/hoimin-core/src/machine/selection.rs),
[verification metadata](../../crates/hoimin-core/src/report/selection.rs),
[MutationOutcome](../../crates/hoimin-core/src/resume.rs).

## Dynamic view — Successful run and final delivery

Scope: a successful ordinary run with a new mutant execution. This is an
abbreviated collaboration sequence; it omits report-start events, session
operations, candidate ranking, budget checks and repeated integrity checks.
Completions between the grouped steps are accepted by the shell and applied to
core. Mutant work may overlap across workers within the configured limits.

```mermaid
---
title: hoimin - Successful run and final delivery
---
sequenceDiagram
    autonumber
    actor Caller as Human or external agent
    participant Shell as Shell coordinator
    participant Core as Core transition
    participant Inputs as Target and analyzer components
    participant Workspace as Workspace ownership
    participant Process as Process execution
    participant Tests as External test command
    participant Report as Report delivery
    Caller->>Shell: Invoke run with selectors, limits and test argv
    Shell->>Core: StartRequested with current RunState
    Core-->>Shell: Next state and requested effects
    Shell->>Inputs: Resolve targets
    Inputs-->>Shell: TargetsResolved
    Shell->>Workspace: Preflight shared snapshot and materialize workers
    Workspace-->>Shell: Workspace completions
    Shell->>Process: Run baseline in worker copy
    Process->>Tests: Spawn with worker cwd and environment
    Tests-->>Process: Exit status and captured output
    Process-->>Shell: Baseline completion and accounting
    Shell->>Core: Accept successful BaselineFinished
    Core-->>Shell: Baseline output and ResetWorker(0)
    Shell->>Workspace: Restore preflight snapshot
    Workspace-->>Shell: WorkerReset
    Shell->>Core: Accept matching restoration completion
    Core-->>Shell: Continue discovery
    Shell->>Inputs: Analyze selected Python sources
    Inputs-->>Shell: Candidate spool and diagnostics
    loop Selected mutants with available budget
        Shell->>Workspace: Apply candidate to worker copy
        Shell->>Process: Run mutant test command
        Process->>Tests: Spawn with worker cwd and environment
        Tests-->>Process: Exit status and captured output
        Process-->>Shell: Mutant completion and accounting
        Shell->>Core: Apply accepted completion
        Core-->>Shell: Outcome, next state and output effects
        Shell->>Report: Deliver MutantFinished
        Report-->>Shell: OutputEmitted
        Shell->>Core: Accept mutant output acknowledgement
        Core-->>Shell: ResetWorker
        Shell->>Workspace: Restore worker for reuse
    end
    Shell->>Process: Establish process and output-drain quiescence
    Shell->>Workspace: Complete workspace cleanup and execution-root removal
    Note over Shell,Workspace: Disk monitor/final measurement must also be quiescent
    Shell->>Report: Deliver final report and flush
    Report-->>Shell: Write/flush result and delivery spool release
    Shell->>Workspace: Complete delivery-root cleanup
    Shell->>Core: Accept final OutputEmitted after lifecycle admission
    Shell-->>Caller: Exit status after report delivery
```

Legend: solid arrows are requests, dashed arrows are results, and the loop groups
repeated work rather than requiring serial execution of all workers. Workspace
ownership includes managed-root cleanup invoked by the shell. This sequence
does not substitute for the state machine or enumerate every event.

Baseline output acknowledgement is independent of restoration. Only the accepted
worker reset permits discovery or the budget check for ordered verification.
Explicit candidate-ID verification skips this budget observation. Cancellation
or failure during restoration prevents mutant dispatch.

On cancellation, timeout or fatal failure, the first shutdown cause and absolute
deadline stay fixed through draining and outer finalization. Recovering a queued
ownership payload after expiry does not certify successful core completion.
Already-running blocking work can outlive its async wait. Run and verify report
work requires owned writers and retains its writer and delivery-root ownership.
Unsafe removal can be deferred for later managed-root reclamation, and failures
preserve incomplete-cleanup evidence.

Sources: [shell orchestration and finalization](../../crates/hoimin-cli/src/shell.rs),
[owned report delivery](../../crates/hoimin-cli/src/report/delivery.rs),
[shutdown deadline contract](../development.md#shutdown-deadline-invariants).

## Deployment and evidence limits

The runtime shown above is local to one host. OS backends differ: Linux can use
cgroup v2 hard limits when available; Windows uses Job Objects; portable
best-effort operation requires the corresponding opt-in. macOS uses the portable
path. The selected backend and reported mode matter more than the operating
system name alone. See [resource implementation](../../crates/hoimin-cli/src/resource/mod.rs)
and [limits and defaults](../usage.md#limits-and-defaults).

Worker copies isolate hoimin's mutation writes from the original source. They
are not a security sandbox for arbitrary test commands. Descendant-process
memory limits do not cover the in-process Rust analyzer, and cooperative disk
monitoring is not a universal hard quota. Persistent outputs and test-command
side effects remain separate concerns. See [product constraints](../product-principles.md)
and [disk-safe execution](../usage.md#disk-safe-execution).

Lean models and their Rust adapters are development evidence, not services or
containers in this runtime. Model proofs and selected implementation comparisons
have different scopes; see [Lean evidence limits](../knowledge/audits/lean-evidence.md).

## Keeping this description current

Update the affected views together when changing CLI dispatch, process/storage
boundaries, the effect/event protocol, plan execution ownership, resource
backends, or cleanup/report ordering. Recheck the command table when adding a
subcommand. Do not promote a proposal to an implemented component before its
code exists.

The [architecture concept](../knowledge/design/architecture.md) links this view
to historical design evidence. Use the [development guide](../development.md)
for validation procedures and the [compatibility policy](../compatibility.md)
for changes to CLI and saved artifacts. The C4 diagrams describe responsibilities
and relationships; they do not prove termination, isolation or platform support.
