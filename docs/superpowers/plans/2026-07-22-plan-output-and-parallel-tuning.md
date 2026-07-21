# Plan Output and Parallel-Run Tuning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align the mutation-testing skill with `hoimin plan`'s JSON-only output contract and document a measured procedure for tuning parallel mutation runs.

**Architecture:** Keep the CLI and runtime unchanged. Strengthen the mirrored skill contract in the existing Python test, then add one user-facing README section whose fenced `hoimin run` command is exercised by the existing Rust documentation contract.

**Tech Stack:** Markdown, Python `unittest`, Rust integration tests, Cargo, Git

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/issues-14-16-docs` on branch `docs/issues-14-16`.
- `hoimin plan` continues to reject `--format` and always writes one JSON plan manifest to standard output; `verify --format json` remains valid.
- Keep `.agents/skills/hoimin-mutation-testing/SKILL.md` and `.claude/skills/hoimin-mutation-testing/SKILL.md` byte-identical.
- Do not change CLI behavior, resource defaults, automatic timeout calculation, public JSON schemas, or `docs/development.md`.
- The README command must use native argv after `--` and work on Windows and Unix without shell-specific syntax.
- Treat the 14-second/4-worker settings as an illustrative measured example, not a universal sizing formula.

## File Map

- `tests/test_skills.py` — defines required bundled-skill prose and enforces mirror equality.
- `.agents/skills/hoimin-mutation-testing/SKILL.md` — Codex-facing mutation planning workflow.
- `.claude/skills/hoimin-mutation-testing/SKILL.md` — Claude-facing byte-identical mirror.
- `README.md` — public resource-limit reference and parallel-run tuning procedure.
- `crates/hoimin-cli/tests/cli_config.rs` — existing proof that `plan` is JSON-only and rejects `--format`; verify without modification.
- `crates/hoimin-cli/tests/report_handler.rs` — existing executor for every fenced README `hoimin run` command; verify without modification.

---

### Task 1: Make the plan output contract explicit in both skills

**Files:**
- Modify: `tests/test_skills.py:15-28`
- Modify: `.agents/skills/hoimin-mutation-testing/SKILL.md:20-31`
- Modify: `.claude/skills/hoimin-mutation-testing/SKILL.md:20-31`
- Test: `tests/test_skills.py`
- Test: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**
- Consumes: the existing `SKILLS["hoimin-mutation-testing"]["required_blocks"]` substring contract and `PlanArgs::into_run_config()` JSON default.
- Produces: an explicit prose contract stating that `plan` always emits one JSON document on stdout and must not receive `--format`.

- [ ] **Step 1: Add a failing required-block assertion**

In `tests/test_skills.py`, insert this string immediately after the existing plan-command block in `required_blocks`:

```python
            "`plan` always writes one JSON document to standard output and diagnostics to standard error;\n"
            "do not pass `--format` to `plan`.",
```

- [ ] **Step 2: Run the focused contract test and confirm the new requirement is absent**

Run:

```bash
uv run --frozen python tests/test_skills.py -v
```

Expected: FAIL in `test_skill_mirrors_and_required_workflows`, with the new JSON/stdout block reported as missing from `hoimin-mutation-testing`.

- [ ] **Step 3: Add the minimal identical skill wording**

In `.agents/skills/hoimin-mutation-testing/SKILL.md`, add this paragraph immediately after the `hoimin plan` code fence:

```markdown
`plan` always writes one JSON document to standard output and diagnostics to standard error;
do not pass `--format` to `plan`.
```

Apply the exact same paragraph at the same location in `.claude/skills/hoimin-mutation-testing/SKILL.md`. Do not change the later `hoimin verify ... --format json` command.

- [ ] **Step 4: Run the skill and CLI contract tests**

Run:

```bash
uv run --frozen python tests/test_skills.py -v
cargo test -p hoimin-cli --test cli_config plan_accepts_run_selection_but_rejects_report_and_session_options -- --exact
cmp .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
```

Expected: the Python test passes, the Rust test passes and continues to reject `plan --format`, and `cmp` exits 0 with no output.

- [ ] **Step 5: Commit the Issue #14 fix**

```bash
git add tests/test_skills.py .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
git commit -m "docs: clarify plan JSON output contract"
```

### Task 2: Add practical parallel-run tuning guidance

**Files:**
- Modify: `README.md:61-86`
- Test: `crates/hoimin-cli/tests/report_handler.rs`

**Interfaces:**
- Consumes: documented defaults `--jobs 1`, `--max-memory 1GiB`, and `--mutant-timeout auto`, plus the existing `documentation_contract()` fenced-command executor.
- Produces: one `Tuning parallel runs` section with a measurement procedure, failure remedies, and a cross-platform executable command example.

- [ ] **Step 1: Record the missing documentation contract**

Run:

```bash
rg -F '## Tuning parallel runs' README.md
rg -F -- '--jobs 4 --max-memory 4GiB --mutant-timeout 2m' README.md
rg -F 'about 14 seconds' README.md
```

Expected: all three commands exit 1 because the section, tested settings, and measured baseline example are not yet documented.

- [ ] **Step 2: Add the tuning section and complete command example**

In `README.md`, insert the following text after the paragraph ending with “Reports identify `hard` or `best_effort` resource mode.” and before the worker-copy paragraph:

````markdown
### Tuning parallel runs

Start with `--jobs 1` and a focused test command. Record the baseline elapsed time and
estimate one test worker's memory use before increasing concurrency gradually. A small
target can often keep `--jobs 1 --max-memory 1GiB --mutant-timeout auto`.

`--max-memory` is one run-wide limit shared by the analyzer, baseline, and all concurrent
workers; it is not multiplied by `--jobs`. When increasing `--jobs`, set an explicit
`--mutant-timeout` with headroom for contention instead of assuming that the `auto` value
derived from a single baseline will remain sufficient. For example, a focused baseline
that takes about 14 seconds can be tried with the following measured settings:

```console
hoimin run --root . --source src --profile focused --jobs 4 --max-memory 4GiB --mutant-timeout 2m -- python -m pytest -q
```

Measure your own suite rather than treating these values as a sizing formula. If results
contain `out_of_memory`, lower `--jobs` or raise `--max-memory`. If they contain `timeout`,
lower `--jobs` or raise `--mutant-timeout`.
````

- [ ] **Step 3: Verify the prose contract and executable README command**

Run:

```bash
rg -F '## Tuning parallel runs' README.md
rg -F -- '--jobs 1 --max-memory 1GiB --mutant-timeout auto' README.md
rg -F -- '--jobs 4 --max-memory 4GiB --mutant-timeout 2m' README.md
rg -F 'about 14 seconds' README.md
cargo test -p hoimin-cli --test report_handler documentation_contract -- --exact
```

Expected: every `rg` prints its matching line and exits 0. `documentation_contract` passes after parsing and executing the new fenced command and validating its JSON report.

- [ ] **Step 4: Review the README diff for scope and portability**

Run:

```bash
git diff -- README.md
rg -n -- '\$\(|`[^`]*\||&&|; ' README.md
```

Expected: the diff changes only the resource-limit area. The portability search shows no shell substitution, pipe, command chaining, or separator added by the new example.

- [ ] **Step 5: Commit the Issue #16 fix**

```bash
git add README.md
git commit -m "docs: explain parallel mutation tuning"
```

### Task 3: Run final repository verification

**Files:**
- Verify only: all files changed in Tasks 1 and 2

**Interfaces:**
- Consumes: the committed skill contract and README tuning guide.
- Produces: evidence that both issue fixes satisfy focused and repository-wide checks without changing runtime behavior.

- [ ] **Step 1: Run focused Python and Rust checks**

Run:

```bash
uv run --frozen python tests/test_skills.py -v
cargo test -p hoimin-cli --test cli_config plan_accepts_run_selection_but_rejects_report_and_session_options -- --exact
cargo test -p hoimin-cli --test report_handler documentation_contract -- --exact
cmp .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
```

Expected: all tests pass and `cmp` exits 0 with no output.

- [ ] **Step 2: Run the full Rust test suite**

Run:

```bash
cargo test --workspace
```

Expected: every unit, integration, property, and documentation test passes with zero failures.

- [ ] **Step 3: Check formatting, whitespace, and final scope**

Run:

```bash
cargo fmt --check
git diff --check HEAD~2..HEAD
git status --short
git diff --stat HEAD~2..HEAD
```

Expected: formatting and whitespace checks exit 0; status is clean; the two implementation commits modify only `README.md`, `tests/test_skills.py`, and the two mirrored skill files.
