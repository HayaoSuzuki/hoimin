# P3 Release, Refactoring, Documentation, and Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Complete the seven actionable P3 issues while keeping #64 pending and merging each issue through an isolated pull request.

**Architecture:** Preserve the current core/CLI boundaries. Add the ordered limit guard in the core state machine, keep human rendering and shell completions in the CLI, and keep documentation-only changes limited to existing README/development/skill contracts. Execute tasks serially from updated main so overlapping documentation changes never share a worktree or conflict during review.

**Tech Stack:** Rust 2024 workspace, Tokio tests, clap/clap_complete, Markdown, Python unittest contracts, Git worktrees, GitHub Actions/PRs.

## Global Constraints

- Issue #64 remains pending; do not modify release workflows or add macOS wheels.
- Each issue uses a dedicated .worktrees/<branch> directory and its own PR.
- Documentation-only commits for #134 and #137 use [skip ci] in the commit subject.
- JSON, JSONL, schema versions, exit codes, and machine-readable event shapes remain unchanged.
- Run focused tests before package/workspace checks and request review before merge.
- Preserve unrelated user files (.idea/, .serena/, and tests/fixtures/projects/basic/uv.lock).

---

### Task 1: Issue #115 — Bound ordered scheduling

**Files:**
- Modify: crates/hoimin-core/src/machine.rs in RunState::schedule_read_or_finalize
- Test: crates/hoimin-core/tests/machine.rs

- [ ] Step 1: Create the worktree

    git worktree add .worktrees/issue-115-max-mutants -b fix/issue-115-max-mutants main

- [ ] Step 2: Add failing tests

Extend the existing ScheduleHarness with max_mutants = 1 and three ordered
candidate IDs. Assert one scheduled mutation, later NotRun results,
mutant_limit_reached, incomplete, and no second ApplyMutation. Add an
exact-bound case asserting one candidate is complete and not incomplete.

- [ ] Step 3: Verify the new tests fail

    cargo test -p hoimin-core --test machine ordered -- --nocapture

- [ ] Step 4: Implement the guard

In the ordered loop, replace unconditional scheduling with:

    if self.scheduled_mutants >= self.config.limits.max_mutants.get() as u64 {
        self.flags.scheduling.mutant_limit_reached = true;
        self.flags.outcome.incomplete = true;
        effects.extend(self.synthetic_started_output(worker, MutationStatus::NotRun)?);
    } else {
        self.scheduled_mutants += 1;
        effects.extend(self.candidate_effects(worker)?);
    }

- [ ] Step 5: Verify and commit

    cargo test -p hoimin-core --test machine ordered -- --nocapture
    cargo test -p hoimin-core
    git add crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs
    git commit -m "fix(core): enforce max_mutants in ordered scheduling"

Review, CI, merge, remove the worktree, and fast-forward main.

---

### Task 2: Issue #134 — Repair the Rust mutation workflow documentation

**Files:**
- Modify: docs/development.md around the analyzer mutation command
- Test: tests/test_focused_mutation_docs.py

- [ ] Step 1: Create the worktree

    git worktree add .worktrees/issue-134-rust-docs -b docs/issue-134-rust-docs main

- [ ] Step 2: Add the contract and verify it is red

Assert that the invalid uv run hoimin run --root . --file
crates/hoimin-cli/src/analyzer/mod.rs command is absent and that
tools/focused_mutation.py, --budget 30m, and cargo mutants --workspace remain.

    uv run --frozen python -m unittest tests.test_focused_mutation_docs -v

- [ ] Step 3: Replace the invalid block

Cross-reference the existing “Focused 30-minute Rust mutation workflow” and
the cargo mutants --workspace workflow; do not suggest passing Rust files to the
Python analyzer.

- [ ] Step 4: Verify and commit with CI suppression

    uv run --frozen python -m unittest tests.test_focused_mutation_docs -v
    git diff --check
    git add docs/development.md tests/test_focused_mutation_docs.py
    git commit -m "docs: replace broken Rust analyzer mutation example [skip ci]"

Review, merge, remove the worktree, and update main.

---

### Task 3: Issue #137 — Document macOS memory-policy behavior

**Files:**
- Modify: README.md
- Modify: .agents/skills/hoimin-mutation-testing/SKILL.md
- Modify: .claude/skills/hoimin-mutation-testing/SKILL.md
- Test: tests/test_skills.py

- [ ] Step 1: Create the worktree

    git worktree add .worktrees/issue-137-macos-memory-docs -b docs/issue-137-macos-memory-docs main

- [ ] Step 2: Add failing documentation assertions

Require README to contain --max-memory and not enforced; require both skill
mirrors to contain --allow-best-effort-memory, while retaining byte identity.

    uv run --frozen python -m unittest tests.test_skills -v

- [ ] Step 3: Update both documents

State that macOS accepts --max-memory for plan compatibility but does not
enforce it; CPU-time and process-group cleanup remain available. Add the
allow-best-effort flag to the skill's macOS plan example in both mirrors.

- [ ] Step 4: Verify and commit with CI suppression

    uv run --frozen python -m unittest tests.test_skills -v
    cmp .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
    git diff --check
    git add README.md .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md tests/test_skills.py
    git commit -m "docs: clarify macOS memory enforcement [skip ci]"

Review, merge, remove the worktree, and update main.

---

### Task 4: Issue #136 — Make operator IDs discoverable

**Files:**
- Modify: README.md
- Modify: crates/hoimin-core/src/config.rs
- Test: crates/hoimin-core/tests/operator_selection.rs
- Test: crates/hoimin-cli/tests/cli_config.rs

- [ ] Step 1: Create the worktree

    git worktree add .worktrees/issue-136-operator-ids -b feat/issue-136-operator-ids main

- [ ] Step 2: Add failing assertions

Assert that an unknown operator error contains valid operators/selectors,
compare_eq_ne, type_nullable, and type_dict_mapping. Add a README contract
covering all runtime IDs, type IDs, and selector families.

    cargo test -p hoimin-core --test operator_selection -- --nocapture

- [ ] Step 3: Implement one canonical list and document it

Add MutationOperatorSelection::valid_names() returning the sorted 20 operator
IDs plus type_collections, type_iterables, and type_nullable. Use it in
UnknownMutationOperator display and add the same IDs/selectors to README;
retain type_mapping as an accepted alias but document type_dict_mapping.

- [ ] Step 4: Verify and commit

    cargo test -p hoimin-core --test operator_selection
    cargo test -p hoimin-cli --test cli_config
    cargo test -p hoimin-core
    git diff --check
    git add README.md crates/hoimin-core/src/config.rs crates/hoimin-core/tests/operator_selection.rs crates/hoimin-cli/tests/cli_config.rs
    git commit -m "feat: make mutation operator IDs discoverable"

Review, CI, merge, remove the worktree, and update main.

---

### Task 5: Issue #138 — Improve limit-validation diagnostics

**Files:**
- Modify: crates/hoimin-core/src/config.rs
- Modify: crates/hoimin-cli/src/cli.rs
- Test: crates/hoimin-core/tests/plan_config.rs
- Test: crates/hoimin-core/tests/target_policy.rs
- Test: crates/hoimin-cli/tests/cli_config.rs

- [ ] Step 1: Create the worktree

    git worktree add .worktrees/issue-138-validation-diagnostics -b feat/issue-138-validation-diagnostics main

- [ ] Step 2: Add failing message cases

Require zero/overflow errors to show --max-memory, --jobs, and
--baseline-timeout; require --max-memory 2gb to list exact-case byte suffixes
and --total-timeout nonsense to show a duration example.

    cargo test -p hoimin-core --test plan_config --test target_policy -- --nocapture
    cargo test -p hoimin-cli --test cli_config -- --nocapture

- [ ] Step 3: Implement diagnostics without changing validation semantics

Use flag spellings in InvalidLimit. Append format guidance only for byte flags
(B, KB, MB, GB, KiB, MiB, GiB; case-sensitive) and duration flags
(expected a duration such as 90s or 5m); keep other InvalidValue messages
unchanged.

- [ ] Step 4: Verify and commit

    cargo test -p hoimin-core --test plan_config --test target_policy
    cargo test -p hoimin-cli --test cli_config
    cargo test -p hoimin-core
    cargo test -p hoimin-cli --lib
    git diff --check
    git add crates/hoimin-core/src/config.rs crates/hoimin-cli/src/cli.rs crates/hoimin-core/tests/plan_config.rs crates/hoimin-core/tests/target_policy.rs crates/hoimin-cli/tests/cli_config.rs
    git commit -m "feat: clarify limit validation diagnostics"

Review, CI, merge, remove the worktree, and update main.

---

### Task 6: Issue #135 — Make human output actionable

**Files:**
- Modify: crates/hoimin-cli/src/report/human.rs
- Test: crates/hoimin-cli/tests/report_handler.rs

- [ ] Step 1: Create the worktree

    git worktree add .worktrees/issue-135-human-report -b feat/issue-135-human-report main

- [ ] Step 2: Add failing rendering assertions

Use existing fixtures with src/calc.py, line 12, column 8, binary_add_sub,
a + b, and a - b. Assert location/diff/status, readable baseline termination,
all status counts, score, complete, and exit are present.

    cargo test -p hoimin-cli --test report_handler human_format -- --nocapture

- [ ] Step 3: Implement stable human helpers

Add private termination_name(ProcessTermination) -> String and
write_summary(&mut impl Write, &RunSummary) -> io::Result<()>. Render
path:line:column operator "original" -> "replacement" status, all seven
counts, score as two decimals or none, complete, and exit under run summary:.
Keep JSON/JSONL, flushing, and diagnostic routing unchanged.

- [ ] Step 4: Verify and commit

    cargo test -p hoimin-cli --test report_handler human_format
    cargo test -p hoimin-cli --test run_e2e human
    cargo test -p hoimin-cli
    git diff --check
    git add crates/hoimin-cli/src/report/human.rs crates/hoimin-cli/tests/report_handler.rs
    git commit -m "feat: make human mutation reports actionable"

Review, CI, merge, remove the worktree, and update main.

---

### Task 7: Issue #139 — Add shell completions

**Files:**
- Modify: crates/hoimin-cli/Cargo.toml
- Modify: Cargo.lock
- Modify: crates/hoimin-cli/src/cli.rs
- Modify: crates/hoimin-cli/src/lib.rs
- Test: crates/hoimin-cli/tests/cli_config.rs
- Test: crates/hoimin-cli/tests/run_e2e.rs

- [ ] Step 1: Create the worktree

    git worktree add .worktrees/issue-139-completions -b feat/issue-139-completions main

- [ ] Step 2: Add failing parser and execution tests

Parse hoimin completions bash, zsh, fish, and powershell into a new
ParsedCommand::Completions variant. Exercise run_with_io for bash and assert
stdout contains the hoimin completion function, stderr is empty, and exit is
zero. Assert an unsupported shell is rejected by clap.

    cargo test -p hoimin-cli --test cli_config completions -- --nocapture
    cargo test -p hoimin-cli --test run_e2e completions -- --nocapture

- [ ] Step 3: Add the dependency and subcommand

Add locked clap_complete; define CompletionsArgs with
shell: clap_complete::Shell, add Command::Completions, map it to
ParsedCommand::Completions, and implement cli::write_completions with
clap_complete::generate(shell, &mut RootCli::command(), "hoimin", writer).

- [ ] Step 4: Verify and commit

    cargo fmt --all -- --check
    cargo test -p hoimin-cli --test cli_config completions
    cargo test -p hoimin-cli --test run_e2e completions
    cargo test -p hoimin-cli
    cargo test --workspace --locked
    git diff --check
    git add Cargo.lock crates/hoimin-cli/Cargo.toml crates/hoimin-cli/src/cli.rs crates/hoimin-cli/src/lib.rs crates/hoimin-cli/tests/cli_config.rs crates/hoimin-cli/tests/run_e2e.rs
    git commit -m "feat: add shell completion generation"

Review, CI, merge, remove the worktree, and update main.

---

## Final integration checklist

- [ ] Verify main contains merged PRs for #115, #134, #137, #136, #138, #135, and #139.
- [ ] Confirm #64 remains open/pending and no release workflow changed.
- [ ] Run cargo fmt --all -- --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace, and the Python unittest discovery command on final main.
- [ ] Confirm only the user's pre-existing .idea/, .serena/, and fixture uv.lock remain uncommitted.

