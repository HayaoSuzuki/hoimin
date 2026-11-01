# Focused mutation improvement workflow

## Purpose

Create a repository-local workflow that finds and verifies high-value Rust
mutation targets within a 30-minute budget. The workflow must leave an
explainable, ranked list of next candidates even when it finds no bug or test
gap during the current run.

This is an evidence-gathering step before adding another public hoimin feature.
The first implementation is a Python development tool plus documentation. After
several runs, only repeatedly useful and repository-independent behavior should
be considered for generalization into the Rust CLI.

Issue #29 is outside this scope. Its adjacent-stall policy was completed on
`main` by commit `a266bdd`.

## Success criteria

A run succeeds when it records:

- the inputs, Git state, commands, and elapsed time;
- the candidates considered and the reason for their ranking;
- which candidates were verified and their mutation outcomes;
- which candidates were not verified and why;
- a deterministic, ranked recommendation for the next run.

Finding a bug, survivor, or test gap is valuable but not required. A
budget-exhausted run is successful when it preserves the evidence above and
reserves enough time to write its report.

## Scope

The first version adds:

- `tools/focused_mutation.py`, a standard-library-first development tool;
- unit tests using fake Git, test, and cargo-mutants executables;
- developer documentation for the 30-minute workflow;
- machine-readable JSON and concise Markdown run reports.

The work is performed in the dedicated worktree
`.worktrees/focused-mutation-workflow` on branch
`feat/focused-mutation-workflow`.

The first version does not:

- change the public hoimin CLI or report schemas;
- mutate Python through hoimin itself;
- promise complete Rust mutation coverage;
- automatically label a survivor as a bug;
- add a Rust syntax-parser dependency;
- run a full workspace mutation inventory on every invocation;
- stash, reset, checkout, or modify the user's source tree.

## Chosen approach

A Python orchestrator is preferred over a shell script or Rust `xtask`.

Python makes time accounting, JSON handling, process recording, deterministic
ranking, and cross-platform testing practical without coupling an experimental
workflow to the product. A shell script would become fragile as result parsing
and interruption handling grow. An `xtask` would provide strong typing but
would require more implementation and would prematurely couple the experiment
to the Rust workspace.

The tool should use the Python standard library unless a dependency provides a
clear, demonstrated benefit.

## Architecture

The tool has four isolated responsibilities.

### Candidate discovery

Discovery reads repository state without modifying it. Inputs are:

- explicit `--file` and `--symbol` selectors;
- production Rust files changed from `--base`;
- production Rust files changed in the working tree;
- recent committed changes when explicit and current-change discovery yields
  fewer than ten candidates;
- candidate information exposed by the installed cargo-mutants version.

Tests, generated artifacts, `.worktrees`, IDE files, and non-Rust files are not
production candidates. Explicit targets always remain eligible.

When cargo-mutants cannot provide a usable function-level inventory, discovery
may use conservative text extraction. Ambiguous results are retained as
unverified candidates and are not executed automatically.

Recent-history discovery walks at most twenty first-parent commits from `HEAD`
and stops as soon as the queue contains ten unique candidates. It does not
traverse unrelated merged branch history.

### Ranking

Ranking is deterministic and explainable. Each candidate stores both a numeric
score and an ordered list of reasons. Initial ranking signals are:

1. explicit user selection;
2. changes since `--base` or in the current worktree;
3. recent committed changes;
4. error handling, state transitions, cancellation, timeouts, resource limits,
   filesystem boundaries, session/resume behavior, and report completion;
5. observable conditional and error paths.

Large file size alone does not increase priority. Stable path and symbol
ordering breaks score ties. The report records the ranking rule version so a
later rule change does not obscure how an earlier decision was made.

### Runner

The runner invokes commands with native argument arrays and never constructs a
shell command string. For each selected target it:

1. runs the focused existing test command;
2. proceeds only when that baseline succeeds;
3. invokes cargo-mutants with narrow `--file` and `--re` selectors;
4. records stdout, stderr, exit status, start time, end time, and timeout;
5. updates the partial run record after every command.

The exact cargo-mutants arguments must be verified against the pinned supported
version during implementation. Iterative runs may reuse caught and unviable
outcomes through cargo-mutants' supported iteration mechanism. A required final
inventory, when one is explicitly requested, must not depend on reused outcomes.

### Reporter

The reporter writes one machine-readable source of truth and one human summary.
It distinguishes tool outcomes from human interpretation. In particular,
`survived`, `timeout`, `unviable`, tool failure, and not-run are separate
states. Manual classification may later identify a test gap, equivalent mutant,
design boundary, potential bug, or low-value result.

## Command interface

The initial interface is:

```console
uv run python tools/focused_mutation.py \
  --budget 30m \
  --base origin/main \
  --output /tmp/hoimin-focused-run
```

`--budget` defaults to 30 minutes. `--base` defaults to `origin/main`.
`--output` is required and must not resolve to the repository's existing
`mutants.out`. Repeated `--file` and `--symbol` options provide explicit
priority targets.

The tool validates the repository root, output destination, duration, explicit
paths, and required commands before starting mutation work. Validation errors
do not modify the worktree.

## Time budget

One monotonic deadline governs the whole run. The default allocation is:

- up to 10 minutes for discovery, ranking, and focused baseline tests;
- up to 15 minutes for focused mutation execution;
- at least 5 minutes reserved for collection and reporting.

These are stage ceilings under one deadline, not three independent budgets.
Unused discovery time may increase mutation time, but the five-minute reporting
reserve is never spent by starting another mutation command.

Before each command, the runner computes a timeout from the remaining stage and
global budgets. It does not start a mutation command after the reporting reserve
begins. A command timeout triggers child-process termination and bounded cleanup,
then reporting. User interruption follows the same cleanup path and produces a
partial report.

The tool does not predict that a command will finish within the budget. It
controls cost through explicit timeouts and by refusing new work after the
mutation stage closes.

## Data flow

1. Create the output directory and write initial run metadata.
2. Record the current commit, branch, dirty state, base revision, tool versions,
   command-line inputs, and monotonic budget.
3. Discover candidates and write the complete ranked candidate list.
4. Select the highest-ranked candidate that fits the remaining scheduling
   rules.
5. Run its focused baseline.
6. If the baseline succeeds, run its focused mutation command.
7. Persist the command record and candidate outcome atomically.
8. Repeat while the mutation stage remains open.
9. Mark every remaining candidate with a not-run reason.
10. Write the final JSON state and derive the Markdown summary from it.

The Markdown report never becomes an independent state store. If Markdown
generation fails, the JSON records the reporting failure when it can still be
safely updated.

## Output contract

Each run directory contains:

- `run.json`: versioned machine-readable run state;
- `report.md`: human and agent summary derived from `run.json`;
- `commands/`: one stdout file, stderr file, and metadata record per command.

`run.json` includes:

- schema and ranking-rule versions;
- repository and tool provenance;
- configured and observed timing;
- overall state;
- all ranked candidates and ranking reasons;
- command records;
- candidate outcomes and manual classifications;
- unverified reasons and next-run ranking.

`report.md` contains:

1. run summary and observed duration;
2. verified candidates;
3. survivors and other investigation results;
4. unverified candidates and reasons;
5. next recommended order;
6. manual classification fields;
7. comparison with a prior full inventory, when supplied.

The workflow never runs a full inventory solely to calculate a comparison.
Without a supplied comparable result, the report says that the reduction ratio
was not measured.

## Run states and error handling

The overall states are:

- `completed`: all scheduled focused work completed;
- `budget_exhausted`: the deadline or stage budget stopped further scheduling;
- `baseline_failed`: a focused baseline failed and its mutation was not run;
- `tool_unavailable`: a required executable was unavailable;
- `command_failed`: a tool failed outside a recognized mutation outcome;
- `interrupted`: the user interrupted the run;
- `report_failed`: the final report could not be completely persisted.

`budget_exhausted`, mutation survivors, and unviable mutants are ordinary
observable outcomes, not infrastructure failures.

Run data is checkpointed after initial metadata, discovery, and every command.
Updates use a same-directory temporary file followed by an atomic replacement.
An interruption or timeout must preserve the latest completed checkpoint.

The runner records initial Git status and does not reject a dirty worktree. It
does not alter tracked or untracked source files, use Git cleanup commands, or
overwrite an existing `mutants.out`. All cargo-mutants artifacts are directed
into the run output area using supported cargo-mutants options or an isolated
working/output location validated during implementation.

## Testing

The Python code separates ranking, time calculation, command execution, output
parsing, and report rendering so each can be tested independently.

Unit and integration-style tests use fake executables and an injected monotonic
clock. They cover:

- deterministic ranking and stable tie ordering;
- explicit and changed target priority;
- stage and global deadline calculations;
- refusal to start mutation during the report reserve;
- baseline failure preventing mutation;
- missing cargo-mutants;
- command timeout and user interruption;
- partial checkpoint retention;
- stdout, stderr, timestamps, and exit-status capture;
- dirty-worktree preservation;
- incomplete or malformed cargo-mutants output;
- consistent JSON and Markdown candidate states.

Tests do not wait for real-time budgets and do not run real mutations.

After those tests pass, one 30-minute evidence run targets the hoimin
repository. That run evaluates the ranking rules and report usability; it is not
a release gate and does not promise complete mutation coverage. Normal Rust and
Python quality gates remain unchanged.

## Generalization gate

No behavior moves into the public hoimin CLI based on a single run. A later
design may generalize a behavior only when multiple evidence runs show that it:

- repeatedly saves material execution time;
- preserves explainable selection and stable identifiers;
- is not specific to hoimin's Rust module names or cargo-mutants output;
- has a clear failure and compatibility contract;
- improves decisions rather than only increasing a mutation score.

Potential generalizations include affected-candidate verification, stable
time-budgeted batching, ranked survivor output, and distinct exploration versus
completeness profiles. Each requires its own approved design.
