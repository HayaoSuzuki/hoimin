# P3 Release, Refactoring, Documentation, and Features Design

## Scope

Issue #64 remains pending by decision: hoimin is not currently distributed to
people outside the project, so no macOS wheel policy or release workflow change
will be made in this cycle.

The implementation scope is the seven remaining actionable P3 issues:

- #115: enforce `max_mutants` in ordered scheduling;
- #134: replace the invalid Rust-file mutation example;
- #135: make human report output actionable;
- #136: expose runtime operator IDs and selector names;
- #137: document macOS memory-policy behavior and repair the mutation-testing skill;
- #138: improve limit-validation diagnostics;
- #139: add shell completion generation.

Each issue is an independent pull request from its own worktree. PRs are
merged serially because #136 and #137 both touch README documentation and the
documentation-only commits must be marked `[skip ci]`.

## Design decisions

### #115 — ordered mutant bound

The ordered branch of `RunState::schedule_read_or_finalize` will use the same
guard as the unordered branch. Once `scheduled_mutants` reaches
`config.limits.max_mutants`, the next selected candidate is emitted as
`MutationStatus::NotRun`, `mutant_limit_reached` and `outcome.incomplete` are
set, and no process is scheduled. Existing finalization and worker-idle rules
remain unchanged. Machine tests cover both an exact bound and a candidate set
larger than the bound.

### #134 — development documentation

The obsolete command that passes a Rust source file to the Python-only hoimin
analyzer will be removed. The section will point contributors to the existing
bounded `tools/focused_mutation.py` workflow and to `cargo mutants --workspace`.
The documentation contract will assert that the invalid command is absent and
the two supported Rust workflows are present.

### #135 — human output

Only `--format human` changes; JSON, JSONL, and all schemas remain unchanged.
Human mutant events will include the candidate location, operator, original
text, replacement text, and stable status name on one line. Source fragments
will use a debug-style quoted representation so embedded whitespace and quotes
remain unambiguous. Baseline termination will use stable lowercase names rather
than Rust `Debug` output. `RunFinished` will render a short summary block with
all status counts, score (or `none`), completeness, and exit code.

### #136 — operator discoverability

The README operator table will list all 13 runtime IDs, all seven type IDs, and
the three type selector families. `UnknownMutationOperator` will include the
sorted valid ID/selector list in its error text. No listing subcommand is added
here because #139 owns the new CLI discoverability surface.

### #137 — macOS resource documentation

README will explicitly say that macOS accepts `--max-memory` for plan
compatibility but does not enforce it; CPU-time and process-group cleanup remain
the available controls. The `.agents` and `.claude` mutation-testing skill
mirrors will both show `--allow-best-effort-memory` in the macOS plan example,
and the existing byte-identical mirror contract remains required.

### #138 — validation diagnostics

Core `InvalidLimit` values will use CLI flag spellings (`--jobs`,
`--max-memory`, and so on), while preserving the existing error variant and
exit behavior. CLI byte errors will explain the accepted exact-case suffixes
`B`, `KB`, `MB`, `GB`, `KiB`, `MiB`, and `GiB`; duration errors will give a
valid example such as `90s` or `5m`. Non-limit parser errors retain their
current concise form.

### #139 — shell completions

The CLI will add `clap_complete` and a `completions` subcommand accepting
`bash`, `zsh`, `fish`, or `powershell`. It writes the generated script to
stdout and performs no run configuration or filesystem work. Parser tests
cover all four shells and the executable path is exercised through
`run_with_io`.

## Verification and integration

Every implementation PR follows the existing Rust/Python quality gates. Rust
behavior changes use a focused red-green test first; documentation PRs use the
relevant Python contract tests and `[skip ci]` in the commit subject. Each PR
receives an independent code review before CI and merge. After each merge, the
next issue worktree is created from the updated `main`.
