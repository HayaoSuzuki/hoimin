# Issue #338: classify only observed cgroup OOM kills as out of memory

## Status

Reviewed design awaiting user approval. This document describes the intended
behavior for Issue #338 only. It does not include the separate per-root memory
normalization diagnostic regression introduced around PR #377.

## Problem

The Linux hard-resource backend snapshots `memory.events` and `pids.events`
for each active root cgroup. Its current `violations_since` implementation
marks a process as `OutOfMemory` when any of these memory counters increases:

- `max`
- `oom`
- `oom_kill`
- `oom_group_kill`

That rule treats memory pressure as proof of termination. Under cgroup v2,
`max` records times processes reached `memory.max` and entered reclaim or
throttling. A process can cross that boundary, recover, and exit successfully.
`oom` records OOM allocation conditions but does not prove that the kernel
killed a task. `oom_group_kill` records group-OOM handling and can increase in
kernel paths where no new task is counted by `oom_kill`, including a selected
victim that is already exiting. Only a new `oom_kill` is direct evidence that
the kernel killed one or more tasks in the observed cgroup subtree.

The current classifier can therefore replace a real `Exit(0)` or other native
termination with `OutOfMemory` after recoverable pressure. That corrupts mutant
status, summary counts, stored session results, and the CLI exit policy.

The kernel counter definitions are documented in the cgroup v2
[`memory.events` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).

Issue #338 names `memory.events:max`, but the current predicate also treats
`oom` and `oom_group_kill` as terminal. Excluding all three is an intentional
semantic expansion: each is evidence of pressure or OOM handling, while
`oom_kill` is the counter whose documented unit is killed processes. Keeping
either additional counter would leave the same unsupported inference in a
neighboring branch.

## Goals

- Report `ProcessTermination::OutOfMemory` only after a new `oom_kill` event
  was observed for that root cgroup while it was active.
- Preserve the current `pids.events:max` process-limit classification.
- Preserve memory-over-process precedence when both terminal observations
  occur for one process.
- Preserve native exit and signal-derived termination when only `max`, `oom`,
  or `oom_group_kill` increases.
- Keep the counter parser and all public counter fields intact.
- Cover the complete decision table with deterministic tests that run without
  delegated cgroup access.

## Non-goals

- Do not redefine `pids.events:max`; its current process-limit meaning is
  outside Issue #338.
- Do not infer that Hoimin's configured `memory.max` caused an observed OOM
  kill. The public result means that the kernel reported an OOM kill in the
  supervised root subtree during the observation window.
- Do not add a new termination variant, report field, schema version,
  diagnostic, or CLI option.
- Do not change cgroup creation, limit writes, launcher migration, cleanup, or
  root ownership.
- Do not repair or document the separate loss of per-root memory normalization
  diagnostics in this pull request.

## Observable contract

Let `before` be a root cgroup's saved event counters and `after` the next
successful read. A counter is new only when `after.counter > before.counter`.
Counter decreases, including an impossible or reset-looking observation, do
not produce a violation.

The table describes classifier output before backend cleanup.

| New counter observation | Memory evidence | Process evidence | Classifier output for `Exit(0)` |
| --- | --- | --- | --- |
| none | no | no | `Exit(0)` |
| `memory.events:max` only | no | no | `Exit(0)` |
| `memory.events:oom` only | no | no | `Exit(0)` |
| `memory.events:oom_group_kill` only | no | no | `Exit(0)` |
| `memory.events:oom_kill` only | yes | no | `OutOfMemory` |
| `pids.events:max` only | no | yes | `ProcessLimit` |
| `oom_kill` and `pids.events:max` | yes | yes | `OutOfMemory` |

The same preservation rule applies to every incoming termination, not only
`Exit(0)`: without terminal resource evidence, classification returns the
incoming value unchanged.

The initial counters are read during root preparation. Evidence becomes
eligible after the stopped launcher has been migrated and its root entry is
marked active, immediately before `SIGCONT`. Unless the run is already cleaned,
classification includes successful refreshes through its own final snapshot
before reading the accumulated signal. If a concurrent or early run close has
already cleaned it, classification uses the evidence retained through close's
last pre-cleanup snapshot. Refreshes during termination or run close can still
produce accounting errors or retain bits, but cannot revise a classifier output
already computed.

If attachment fails after the entry becomes active, process setup fails and the
cleanup path owns the root. That path does not produce an OOM or process-limit
process result.

## Design

### File scope

Expected changes are limited to:

- `crates/hoimin-cli/src/resource/linux.rs` for delta and classification logic;
- `crates/hoimin-cli/tests/process_handler.rs` for a focused delegated-cgroup
  correspondence case;
- `README.md` for the current `out_of_memory` status definition.

The stale PR #377 fixtures and broader run-wide wording are a declared prerequisite
repair, not hidden additions to this branch.

### Counter delta extraction

Keep `CgroupEventCounters` and `parse_cgroup_event_counters` unchanged. They
remain useful for diagnostics, compatibility, and tests even when a field is
not terminal evidence.

Change the pure delta decision in
`crates/hoimin-cli/src/resource/linux.rs` so that:

- memory evidence is `after.oom_kill > before.oom_kill`;
- process evidence is `after.pids_max > before.pids_max`;
- `memory_max`, `oom`, and `oom_group_kill` never set the memory-violation bit.

Return the existing two-bit representation with an explicit four-case match on
`(memory, processes)`. The implementation must not express this decision as an
arithmetic or boolean operator substitution whose OR-to-XOR mutant is
equivalent for mutually exclusive bit positions.

### Accumulation and classification

Retain `RootSignal::violations: AtomicU8`. `refresh_events` continues to OR each
new observation into the root signal so evidence found in an earlier refresh
cannot be lost before final classification.

Extract the final mapping into a pure helper. Derive `memory` and `processes`
booleans by masking only the two known bits, then match the four boolean pairs
explicitly:

- no bits: return the incoming termination;
- memory only: `OutOfMemory`;
- process only: `ProcessLimit`;
- both: `OutOfMemory`.

`LinuxRunCgroup::classify_root` remains responsible for refreshing active-root
counters and loading the accumulated signal. Masking known bits preserves the
existing behavior if an unknown internal bit is present. The helper makes
precedence and preservation testable without a writable cgroup filesystem.

No ordering or memory-ordering change is required. `fetch_or` remains the
monotonic accumulator, and the existing acquire/release operations remain in
place.

### Error behavior

Event-file read and parse errors keep their existing typed failure behavior.
This change must not turn missing or malformed accounting into a native exit,
an OOM classification, or a process-limit classification.

Classification still occurs before normal backend cleanup. Final delivery keeps
the existing precedence:

- classification success plus cleanup success returns the classified
  termination;
- classification success plus cleanup failure returns the cleanup
  `EffectFailed`;
- classification failure plus cleanup success returns the classification
  `EffectFailed`;
- if both fail, classification remains primary and cleanup detail is appended.

After successful classification and cleanup, output finalization can still
replace the termination with `EffectFailed` for ordinary read, write, or join
errors. The existing mutant-only output-close-timeout exception retains the
termination with `ProcessOutputState::CloseTimedOut`. This issue changes none of
those finalization rules.

## Test strategy

Implementation follows test-driven development.

### RED

Expand the existing `event_deltas_classify_only_new_memory_and_process_violations`
unit coverage before changing production logic. Start from nonzero baselines so
the tests prove delta semantics rather than absolute-counter semantics.

The RED commit tests only the existing `violations_since` seam. Its failing
cases must independently increment:

- `memory_max` and expect no memory bit;
- `oom` and expect no memory bit;
- `oom_group_kill` and expect no memory bit.

These assertions fail against the current implementation for the reason
reported in Issue #338. Extract the pure final-classification helper during
GREEN, then add its exhaustive mapping tests; RED must not depend on a helper
that production code does not yet define.

### GREEN and regression matrix

Add or refactor unit tests to cover all four valid signal values and assert:

- preservation of an incoming non-resource termination when no bit is set;
- `oom_kill` alone maps to `OutOfMemory`;
- `pids_max` alone maps to `ProcessLimit`;
- simultaneous memory and process evidence maps to `OutOfMemory`;
- each nonterminal memory counter can increase alone and together without
  setting the memory bit;
- equal and lower readings of both `oom_kill` and `pids_max` do not create new
  evidence;
- when one terminal counter decreases and the other increases, only the valid
  increase is classified;
- the parser still exposes all five named counters.

Add a narrowly named delegated-cgroup integration case that places one process
under a per-root memory limit, allocates beyond it, and requires
`OutOfMemory`. This supplies real `oom_kill` correspondence without depending
on the old run-wide cgroup layout. The deterministic unit table remains the
direct regression oracle for pressure without a kill because a portable,
reliable way to force reclaim without a platform-dependent OOM kill is not
available in ordinary CI.

The full delegated suite is not a branch acceptance signal at the current base:
its decimal-limit and concurrent-aggregate fixtures still assume the run-wide
limit layout removed by PR #377. This branch does not repair those unrelated
fixtures. Their baseline status and the exact focused hard-cgroup evidence must
be disclosed in the pull request; stale failures must not be presented as
regressions caused by Issue #338.

Repairing the stale PR #377 fixtures is outside the authorized two-Issue delivery.
This design does not assign that work to an unowned branch or promise that this
delivery will close #338. The limitation blocks use of the complete delegated
suite as a green gate, but it does not invalidate the deterministic regression
or a separately executed exact hard-cgroup case.

## Mutation-test contract

Use `cargo-mutants 27.1.0` after the focused and workspace tests pass. Mutation
work has two gates.

First, use raw cargo-mutants filters for the changed counter-delta and
termination-mapping helpers. On the final branch SHA, run the following without
`--iterate`:

```console
output_dir="$(mktemp -d /tmp/hoimin-issue-338-mutants.XXXXXX)"
filter='(violations_since|classify_violations)'
cargo mutants \
  --file crates/hoimin-cli/src/resource/linux.rs \
  --re "$filter" --list --json > "$output_dir/inventory.json"
cargo mutants \
  --file crates/hoimin-cli/src/resource/linux.rs \
  --re "$filter" --output "$output_dir"
```

1. Record the branch commit SHA, exact commands, `inventory.json`, and the
   generated `mutants.out` report directory.
2. Compare the executed mutant names with `inventory.json`; any listed mutant
   without a terminal result makes the focused gate incomplete.
3. Require zero missed, timeout, and error results.
4. Inspect every unviable result and record why it cannot compile or cannot
   represent a behavioral alternative.
5. Treat every survivor as a test gap unless a concrete equivalence argument is
   recorded and independently reviewed.

Do not include Linux-only `LinuxRunCgroup::classify_root` in this cross-host
zero-missed slice. It is cfg-disabled on macOS, ordinary Linux lacks delegated
cgroup execution, and the delegated full-suite baseline contains the disclosed
stale PR #377 fixtures. The new exact delegated hard-cgroup test is the required
production call-site correspondence evidence instead.

Second, comply with the repository release policy by running a fresh,
non-iterated `cargo mutants --workspace`. Preserve its complete report. Resolve
every timeout, baseline failure, and tool error. Fix every missed mutant or
record an exact, independently reviewed equivalence argument. Inspect every
unviable and platform-inapplicable result. Outcomes outside the PR diff are
reported as such; they are called pre-existing only if a same-host parent-SHA
baseline proves it. The focused report is PR-diff evidence and must not be
mislabeled as the complete workspace inventory.

The truth table is intentionally structured to kill comparison inversions,
wrong-counter substitutions, deleted branches, swapped classifications, and
changed memory/process precedence. The explicit tuple match avoids introducing
an equivalent OR-to-XOR survivor in the delta encoder.

## Verification

Run on the branch:

- focused Linux resource unit tests;
- `cargo test --workspace`;
- `cargo fmt --check`;
- the repository's all-target/all-feature clippy command;
- the focused mutation inventory and execution described above;
- a fresh non-iterated `cargo mutants --workspace` with a reviewed report.

All ordinary pull-request checks must pass on the final SHA: the Linux, macOS,
and Windows quality and Rust matrices, MSRV, contracts, Python test discovery,
wheel smoke, and nightly shuffled Rust suite. The new hard-cgroup case requires
separate native delegated-Linux evidence because ordinary pull-request CI
cannot provide that capability.

The GitHub Actions `linux-cgroup-v2-hard` job currently runs only on pushes to
`main`, not pull requests, and its full suite contains the stale PR #377
fixtures described above. When a delegated runner is available, execute the new
focused hard-cgroup case without a `SKIP:` result. Until that evidence and the
separate stale-suite repair are available, the pull request uses `Refs #338`,
not `Closes #338`, and states exactly which evidence was unavailable.

If a later authorized change repairs the stale PR #377 fixtures, rerun the exact
`linux-cgroup-v2-hard` job on `main` and use its URL as closure evidence. This
two-PR delivery ends with an honest `Refs #338` pull request and does not claim
that later external step is scheduled.

## Documentation and compatibility

Update the current README mutant-status definition. `out_of_memory` must mean
that the kernel reported an OOM kill in the supervised root cgroup subtree; it
must not claim that a run-wide configured memory limit necessarily caused the
kill. Broader PR #377 documentation remains outside this branch.

The public JSON/JSONL schema, session compatibility fingerprint, resource mode,
and exit codes remain unchanged. Only the accuracy and documentation of an
existing termination value change.

Archival specifications and implementation plans are historical records and
must not be edited.

## Formal-method decision

Lean is not required for this change. The behavior is a four-row finite truth
table with no retry, asynchronous interleaving, arithmetic bound, or new state
transition. Exhaustive Rust unit tests exercise the production helper directly
and provide stronger implementation correspondence than a duplicate model.
The existing event-refresh and process lifecycle ordering are not changed.

## Risks and controls

- **False negatives from `oom_group_kill`:** controlled by requiring the
  kernel's direct `oom_kill` evidence. Group handling without a counted kill is
  not classified as a killed task.
- **Loss of earlier evidence:** controlled by retaining atomic OR accumulation
  across refreshes.
- **Changed precedence:** controlled by the explicit both-bits test and memory
  precedence contract.
- **Counter wrap or reset:** controlled by strict `after > before`; the existing
  behavior for non-monotonic observations is preserved.
- **Scope creep into PR #377:** controlled by excluding per-root diagnostic
  propagation from files and acceptance criteria for this branch.

## Review record

The written design passed three independent review rounds and follow-up checks:

1. The semantic review rejected `memory.events:max`, `oom`, and
   `oom_group_kill` as kill evidence, required the intentional expansion beyond
   the issue title to be explicit, and corrected the README's causal wording.
2. The lifecycle review separated counter snapshots, classifier output,
   cleanup, and output finalization; it also defined the already-cleaned and
   failed-attachment edges and unknown-bit behavior.
3. The delivery review exposed stale PR #377 delegated fixtures, restored the
   repository's full mutation gate, made the focused command executable, and
   removed a cfg-disabled Linux call site from the cross-host zero-missed slice.

Follow-up review found no unresolved semantic issue in the `oom_kill`-only
predicate, sticky accumulation, memory precedence, or stated branch boundary.
The missing full delegated-suite signal remains an explicit limitation, not a
claimed pass.
