# Issue #339: remove Hoimin-installed portable Unix RLIMIT_CPU

## Status

Reviewed design awaiting user approval. This document covers the portable Unix
timeout defect in Issue #339. It does not redesign the Windows Job Object or
Linux cgroup-v2 backends.

## Problem

Hoimin already owns a monotonic wall-clock deadline for every test process.
The portable Unix backend additionally installs `RLIMIT_CPU` with both soft and
hard limits equal to the rounded-up wall timeout. Linux portable mode also
installs `RLIMIT_AS`; macOS installs no address-space limit.

`RLIMIT_CPU` measures CPU consumed by the process, not elapsed wall time. Threads
within one process share that process limit. A CPU-heavy multithreaded test can
therefore consume several CPU-seconds during a fraction of one wall-clock
second and be killed before Hoimin's deadline. This is not limited to Python:
any native or managed multithreaded test process can trigger it.

Setting the soft and hard limits to the same value also removes a useful soft
signal interval. The resulting termination can reach Hoimin as signal-derived
`Exit(128 + signal)` before the wall deadline, so the report records a native
exit instead of `Timeout`. On current macOS, an eight-thread CPU-heavy Python
process with a two-second `RLIMIT_CPU` was observed to exit as 152 in well under
two seconds of wall time.

The defect is the use of a CPU-time policy as a second implementation of a
wall-time contract. The fix is to stop installing that policy.

## Goals

- Remove every `RLIMIT_CPU` read or write performed by Hoimin's portable Unix
  backend on every Unix target, including Linux and macOS.
- Preserve the exact `RLIMIT_CPU` soft and hard values held by the spawning
  Hoimin process immediately before spawn.
- Keep Tokio's pre-spawn monotonic deadline as the sole Hoimin-owned timeout
  mechanism for portable Unix processes.
- Preserve cancellation-before-timeout-before-completion precedence.
- Preserve timeout and cancellation process-group cleanup and root reaping.
- Preserve native exit and signal-derived termination when the child completes
  before Hoimin's deadline.
- Correct user-facing resource-control diagnostics and documentation.

## Non-goals

- Do not add a replacement CPU quota or scale a CPU limit by core count,
  `--jobs`, thread count, or `available_parallelism`.
- Do not reinterpret `SIGXCPU`, `SIGKILL`, or exit code 152 as a Hoimin timeout.
- Do not change the timeout duration, automatic-timeout formula, Tokio select
  order, output-drain grace, or termination variants.
- Do not change Linux hard cgroup behavior, Windows Job Objects, process-count
  enforcement, memory enforcement, resource mode, or report schema.
- Do not promise cleanup of descendants after a root process naturally exits
  before the deadline. Portable Unix deliberately drops safe process-group
  ownership after root reap; that existing limitation is separate from Issue
  #339.
- Do not claim protection from a stalled Tokio runtime, host suspension, or a
  Hoimin crash. Deadline enforcement assumes Hoimin and the runtime continue to
  make progress.

## Observable contract

### Timeout ownership

`ProcessHandler::run` computes a Tokio deadline before spawning the child and
selects outcomes in this order:

1. cancellation;
2. wall-clock deadline;
3. child completion.

This existing biased selection remains unchanged. If the deadline wins, Hoimin
attempts to terminate the supervised process group and reap the root before
producing `ProcessTermination::Timeout`. Cancellation uses the same cleanup
before producing `Cancelled`. If child completion wins, the existing exit-status
conversion and resource classification run unchanged.

The precedence above governs selection, not successful final delivery. A
termination or reap failure returns `EffectFailed`. An ordinary output
read/write/join failure also returns `EffectFailed`. The existing mutant-only
output-close-timeout rule retains the selected termination with
`ProcessOutputState::CloseTimedOut`. None of these later rules change in this
issue.

### Inherited CPU policy

The portable Unix backend must leave both numeric members of the inherited
`RLIMIT_CPU` pair unchanged across `exec`:

| Parent soft limit | Parent hard limit | Child soft limit | Child hard limit |
| --- | --- | --- | --- |
| finite `S` | finite `H` | `S` | `H` |
| infinity | infinity | infinity | infinity |
| any valid unequal pair | any valid unequal pair | same soft value | same hard value |

The child values in this table are the numeric `rlim_cur` and `rlim_max`
observed immediately after `exec`, before target code modifies them. An
inherited finite limit is external policy. If that policy kills a test before
Hoimin's deadline, Hoimin preserves the platform's existing native termination
representation. Hoimin neither raises inherited limits nor labels their
signals as its own timeout.

### Platform resource setup after the change

| Backend | Hoimin setup before `exec` |
| --- | --- |
| Linux portable Unix | `setpgid(0, 0)` and `RLIMIT_AS` from `max_memory_bytes` |
| macOS portable Unix | `setpgid(0, 0)` only |
| other Unix | unchanged `setpgid(0, 0)` and `RLIMIT_AS` setup |
| Windows portable | unchanged suspended-process Job Object setup |
| non-Unix, non-Windows targets | unchanged no-op setup |

Portable Linux and macOS remain `ResourceMode::BestEffort`. macOS still requires
`--allow-best-effort-memory` because `max-memory` is not enforced there.

## Design

### File scope

Expected changes are limited to:

- `crates/hoimin-cli/src/resource/portable.rs` for Unix setup and diagnostics;
- `crates/hoimin-cli/src/resource/mod.rs` for pinned macOS diagnostic tests;
- `crates/hoimin-cli/tests/process_handler.rs` for inheritance, native-exit,
  and diagnostic coverage;
- `README.md` for both current CPU-limit claims.

Do not edit process lifecycle code unless RED evidence disproves this design.

### Unix command configuration

In `crates/hoimin-cli/src/resource/portable.rs`:

- remove timeout-to-CPU-seconds conversion from both Unix configuration
  functions;
- remove both `setrlimit(RLIMIT_CPU, ...)` calls and their structures;
- retain `setpgid(0, 0)` on Linux and macOS;
- retain `RLIMIT_AS` setup on non-macOS Unix;
- mark the macOS `ProcessLimits` argument unused without changing the shared
  function signature.

The change adds no pre-exec call: it removes the CPU-limit call and leaves the
existing `setpgid` and `RLIMIT_AS` behavior and risk unchanged. Error
propagation for those retained calls remains unchanged. A broader audit of
portable pre-exec safety is outside Issue #339.

No code is added to query `RLIMIT_CPU`. Production code must be independent of
that inherited value so it cannot accidentally normalize, clamp, or reject an
external policy.

### Process lifecycle

Do not modify `ProcessHandler::run`, `select_process_result`,
`terminate_and_reap`, or output collection. They already implement the desired
wall-clock deadline and cleanup path. Keeping those functions unchanged makes
the regression a narrow policy deletion and avoids introducing a second race
near child completion.

The existing precedence at simultaneous readiness is intentional. Because the
select is biased, cancellation wins over the deadline and the deadline wins
over child completion. Removing `RLIMIT_CPU` must not weaken tests that pin this
order.

### Diagnostics and documentation

Update current statements that claim portable Unix uses `RLIMIT_CPU`:

- Linux `BestEffortNotAllowed` error when opt-in is absent: say that portable
  Linux uses per-process `RLIMIT_AS` and process groups.
- macOS best-effort diagnostic: say that macOS uses process groups and does not
  enforce `max-memory`.
- both current README resource-limit passages: describe Tokio wall-clock
  deadlines and process-group cleanup without claiming a CPU-time limit on
  macOS (the plan/verify paragraph near the top and the limits paragraph).
- Tests that pin the exact diagnostic strings: update them with production.

Do not edit archival design documents or implementation plans. They describe
the behavior at the time they were written.

## Test strategy

Implementation follows test-driven development.

### RED: inherited finite unequal limit

Add a Unix-only regression beside `configure_command` in
`crates/hoimin-cli/src/resource/portable.rs`. It must not mutate the shared Rust
test process's hard resource limit.

Use the repository's ignored-fixture pattern with one child, not nested helper
and probe processes:

1. the parent test reads its inherited hard ceiling and chooses a fixed
   high-headroom unequal pair such as 30/31 seconds when permitted, otherwise an
   equally generous lower pair or an infrastructure precondition failure;
2. it constructs a command for the current test executable with
   `--ignored --exact`, selecting only a limit-probe fixture;
3. a test-owned pre-exec closure sets the chosen pair in that child only;
4. production `PortableBackend::prepare` appends the real Unix setup with a
   deliberately different five-second wall timeout;
5. after spawn, the parent attaches the returned supervisor; if attachment
   fails, it kills and reaps the directly owned child before returning;
6. the fixture reads its numeric pair immediately and asserts the expected
   values supplied in private environment variables;
7. on normal completion, the parent reaps the child and then calls
   `supervisor.terminate(false)` so Drop cannot signal a recycled process-group
   ID;
8. on timeout or wait error, the parent calls `supervisor.terminate(true)` while
   the root is still owned, then reaps the child before failing.

The ignored fixture does not change limits itself. All changed limits exist only
in the directly supervised child. There is no escaped inner group and no role
selector to inherit recursively. The parent requires fixture success and
includes captured output on failure.

This test fails against the current implementation because Hoimin overwrites
the pair with equal rounded wall-time values. It passes only when the portable
backend leaves the pair alone. A finite unequal pair detects both accidental
normalization and changes to only one member.

The test-owned pre-exec setter must not raise the inherited hard limit. With an
infinite ceiling it uses the fixed generous pair. If a finite ceiling cannot
provide safe headroom, the test reports an infrastructure precondition failure
rather than mutating the parent or silently passing.

### Characterization and regression coverage

Add Unix-only characterization cases with a generous Hoimin deadline:

- an explicit `exit(152)` remains `ProcessTermination::Exit(152)`;
- self-delivered `SIGXCPU` remains the existing
  `ProcessTermination::Exit(128 + SIGXCPU)` representation.

Before self-delivery, the SIGXCPU probe must set the disposition to `SIG_DFL`
and unblock the signal. This makes the expected native termination independent
of a parent that ignored or blocked SIGXCPU before launching the test suite.

These cases prevent a later implementation from hiding the bug by
reclassifying ambiguous native exits as `Timeout`.

Retain and run the existing process lifecycle tests, especially:

- `honors_requested_timeout`;
- `timeout_terminates_descendants`;
- `cancellation_terminates_descendants`;
- `cancellation_and_timeout_precede_a_simultaneous_exit`;
- timeout/cancellation cleanup tests with injected termination failures.

Do not add a duplicate descendant-cleanup fixture solely for this issue. The
existing tests exercise the unchanged owner and cleanup paths directly.

Update diagnostic expectation tests for Linux and macOS. Add a documentation
contract assertion if needed so `RLIMIT_CPU` cannot reappear in the current
README resource description without review.

## Mutation-test contract

Use `cargo-mutants 27.1.0` after focused and workspace tests pass. Mutation work
has two gates.

The critical change deletes production code, so cargo-mutants cannot mutate the
removed `setrlimit` calls. The mandatory RED/GREEN inheritance test is the
primary evidence for that deletion: it must fail on the parent commit and pass
on the branch.

First, for retained production Rust code affected by the edit, use a raw
cargo-mutants filter. On each native target and final branch SHA, run the
following without `--iterate`:

```console
output_dir="$(mktemp -d /tmp/hoimin-issue-339-mutants.XXXXXX)"
filter='(replace configure_command ->| in configure_command$)'
cargo mutants \
  --file crates/hoimin-cli/src/resource/portable.rs \
  --re "$filter" --list --json > "$output_dir/inventory.json"
cargo mutants \
  --file crates/hoimin-cli/src/resource/portable.rs \
  --re "$filter" --output "$output_dir"
```

1. record the branch commit SHA, exact commands, `inventory.json`, and the
   generated `mutants.out` report directory;
2. compare executed mutant names with `inventory.json`; any listed mutant
   without a terminal result makes the focused run incomplete;
3. require zero missed, timeout, and error results among mutants applicable on
   that native target;
4. inspect and explain every unviable result;
5. treat survivors as test gaps unless an independently reviewed equivalence
   argument is recorded.

Second, comply with the repository release policy by running a fresh,
non-iterated `cargo mutants --workspace`. Preserve its complete report. Resolve
every timeout, baseline failure, and tool error. Fix every missed mutant or
record an exact, independently reviewed equivalence argument. Inspect every
unviable and platform-inapplicable result. Outcomes outside the PR diff are
reported as such; they are called pre-existing only if a same-host parent-SHA
baseline proves it. The focused result is PR-diff evidence, not a complete
workspace audit.

Because one host's source inventory can list cfg-disabled `configure_command`
definitions, aggregate evidence by exact mutant name and native target. Require
zero adverse outcomes among the mutants applicable on that target. Run the
macOS slice locally and obtain a separate manual Linux slice; current GitHub
Actions does not install cargo-mutants, so ordinary CI is not mutation evidence.

## Verification

Run on macOS:

- the new inherited-limit RED against the parent commit and GREEN on the branch;
- focused portable process-handler tests;
- `cargo test --workspace`;
- `cargo fmt --check`;
- the repository's all-target/all-feature clippy command;
- the applicable focused mutation inventory;
- a fresh non-iterated `cargo mutants --workspace` with a reviewed report.

Obtain ordinary Linux CI evidence for the same inherited-limit test, portable
process-handler suite, workspace suite, and clippy. Run the Linux-applicable
mutation slice separately on a native Linux host. This fix may use
`Closes #339` once both platform paths are represented by passing native tests
and the required CI checks.

All ordinary pull-request checks must pass on the final SHA: the Linux, macOS,
and Windows quality and Rust matrices, MSRV, contracts, Python test discovery,
wheel smoke, and nightly shuffled Rust suite. The Windows matrix confirms that
the cfg-separated Job Object path remains unchanged; it is not part of the Unix
inheritance assertion.

Because both Issue branches update `README.md`, merge #339 first, then rebase
#338 onto the new `main`, repeat the complete verification and review on the
rebased SHA, and only then merge #338.

## Compatibility

There is no CLI, plan, report, session, or serialization change. A plan created
before this fix keeps the same timeout values and compatibility fingerprint.
Only the portable Unix implementation stops imposing an undocumented additional
CPU policy.

Users who deliberately launch Hoimin under a finite `RLIMIT_CPU` retain that
external policy. Removing Hoimin's override can therefore make the child inherit
a stricter or looser limit than the old rounded timeout; exact inheritance is
the compatibility rule because silently rewriting the caller's process policy
is the defect.

## Formal-method decision

Lean is not required. The fix removes a mismatched arithmetic policy and adds no
new timeout calculation or state transition. The existing cancellation,
deadline, and completion precedence already has direct Rust coverage and is not
modified. A new model of unchanged lifecycle code would not improve
implementation correspondence for the deleted `setrlimit` calls; the isolated
inheritance executable test does.

## Risks and controls

- **Runaway CPU after losing the fuse:** controlled by the existing monotonic
  wall deadline and process-group termination, under the stated runtime-progress
  assumption.
- **Inherited external CPU limits remain active:** controlled by documenting and
  testing exact inheritance rather than claiming Hoimin owns those limits.
- **Signal misclassification:** controlled by explicit exit-152 and SIGXCPU
  characterization tests.
- **Process leaks on timeout:** controlled by retaining and rerunning existing
  descendant cleanup and injected-failure tests.
- **Linux memory regression:** controlled by leaving `RLIMIT_AS` code in place
  and covering Linux portable setup in native CI.
- **Misleading best-effort reporting:** controlled by updating production
  diagnostics, their exact tests, and current README text together.

## Review record

The written design passed three independent review rounds and follow-up checks:

1. The semantic review corrected the inheritance boundary to the spawning
   Hoimin process, covered every Unix cfg branch, and removed an unsupported
   async-signal-safety claim.
2. The lifecycle review separated selection from cleanup and output errors,
   required deterministic SIGXCPU disposition/masking, and redesigned RED as a
   single directly supervised child with explicit attach, success, timeout,
   wait-error, and reap ordering.
3. The delivery review required both live README corrections, every ordinary PR
   matrix, native Linux/macOS evidence, executable raw cargo-mutants filters,
   and the repository's full non-iterated workspace mutation gate.

Follow-up review found no unresolved semantic, supervisor-ownership,
cross-platform, mutation, or delivery flaw. The documented portable Unix
natural-root-exit and runtime-progress limitations remain unchanged.
