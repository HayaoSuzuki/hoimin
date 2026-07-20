# hoimin Mutation Testing Skills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship two project-local skills that let Claude Code and Codex CLI run focused hoimin mutation tests and iterate on survivors until progress is complete or saturated.

**Architecture:** Store one narrowly scoped skill per workflow in each CLI's native discovery directory. Each Codex/Claude pair is byte-identical and a portable Python contract test protects its frontmatter and required behavior. The loop skill saves ordered temporary JSON reports and uses `hoimin progress` field `latest.state` as the controller.

**Tech Stack:** Agent Skills `SKILL.md`, Python 3.14 standard-library `unittest`, GitHub Actions, existing hoimin CLI.

## Global Constraints

- Create exactly the four project-local `SKILL.md` files specified below; do not add a plugin, symlink, or product-specific frontmatter.
- Each `.agents`/`.claude` pair stays byte-for-byte identical.
- Frontmatter contains only `name` and `description`; both descriptions start with `Use when`.
- Target production Python code, require a passing normal test command, and use `--profile focused --format json` by default.
- Interpret hoimin exit `1` as survivor data. In the loop, use `latest.state`, not the `hoimin progress` exit code.
- Do not persist reports or sessions unless the user requests it.

## File Structure

| File | Responsibility |
| --- | --- |
| `tests/test_skills.py` | Enforces frontmatter, required workflow contracts, and byte-identical CLI mirrors. |
| `.agents/skills/hoimin-mutation-testing/SKILL.md` | Codex copy of the single-run workflow. |
| `.claude/skills/hoimin-mutation-testing/SKILL.md` | Claude copy of the same single-run workflow. |
| `.agents/skills/hoimin-mutation-improvement/SKILL.md` | Codex copy of the survivor-improvement loop. |
| `.claude/skills/hoimin-mutation-improvement/SKILL.md` | Claude copy of the same survivor-improvement loop. |
| `.github/workflows/ci.yml` | Runs the skill contract on every supported CI platform. |

### Task 1: Add the single-run mutation-testing skill

**Files:**
- Create: `tests/test_skills.py`
- Create: `.agents/skills/hoimin-mutation-testing/SKILL.md`
- Create: `.claude/skills/hoimin-mutation-testing/SKILL.md`

**Interfaces:**
- Consumes: repository root discovered from `tests/test_skills.py` and project-local skill directories scanned by Codex CLI and Claude Code.
- Produces: `$hoimin-mutation-testing` and a unittest contract that Task 2 extends.

- [ ] **Step 1: Capture baseline behavior before the new skill exists**

Use three fresh agent contexts. Do not disclose the desired workflow. Record raw answers in the implementation transcript.

```text
I changed Python production code and added a happy-path test. We are short on time: use hoimin to find missing behavioral test coverage.
```

```text
The normal test suite fails, but run mutation testing anyway and add tests for whatever survives. Do not spend time on the baseline failure.
```

```text
Use hoimin on the tests I just wrote so we can prove the test module itself is robust.
```

Expected: identify any skipped baseline, survivor handling, or test-targeting gap. If none occurs, record that and add only guidance justified by the observed answers.

- [ ] **Step 2: Write the failing contract test**

Create `tests/test_skills.py`:

```python
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SKILLS = {
    "hoimin-mutation-testing": {
        "description": (
            "Use when developing or changing Python code and automated tests, "
            "and hoimin mutation testing can expose missing behavioral coverage."
        ),
        "phrases": (
            "--profile focused --format json",
            "--source <dir> --changed",
            "`--file <path>`",
            "production code",
            "`1`",
            "survivor",
        ),
    },
}


class SkillContractTests(unittest.TestCase):
    def test_skill_mirrors_and_required_workflows(self) -> None:
        for name, contract in SKILLS.items():
            codex = ROOT / ".agents" / "skills" / name / "SKILL.md"
            claude = ROOT / ".claude" / "skills" / name / "SKILL.md"
            self.assertTrue(codex.is_file(), codex)
            self.assertTrue(claude.is_file(), claude)
            self.assertEqual(codex.read_bytes(), claude.read_bytes(), name)

            lines = codex.read_text(encoding="utf-8").splitlines()
            self.assertGreaterEqual(len(lines), 4, name)
            self.assertEqual(lines[0], "---", name)
            closing = lines.index("---", 1)
            frontmatter = dict(line.split(": ", 1) for line in lines[1:closing])
            self.assertEqual(
                frontmatter,
                {"name": name, "description": contract["description"]},
                name,
            )
            self.assertTrue(frontmatter["description"].startswith("Use when"), name)
            body = "\n".join(lines[closing + 1 :])
            for phrase in contract["phrases"]:
                self.assertIn(phrase, body, f"{name}: {phrase}")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 3: Verify RED**

Run: `uv run --frozen python -m unittest tests/test_skills.py`

Expected: FAIL because `.agents/skills/hoimin-mutation-testing/SKILL.md` does not exist.

- [ ] **Step 4: Create the two identical single-run skill files**

Create both skill files with exactly this content:

````markdown
---
name: hoimin-mutation-testing
description: Use when developing or changing Python code and automated tests, and hoimin mutation testing can expose missing behavioral coverage.
---

# Test Python Changes with hoimin

Use hoimin on changed production code to find behavioral gaps in automated tests. A survivor is evidence to investigate, not a reason to modify production code solely to make the mutant fail.

## Workflow

1. Inspect the changed production Python files, their existing tests, and the project's normal test command.
2. Run the normal test command first. If it fails, repair or report the failure before mutation testing.
3. Target production code, never the test module. Use `--source <dir> --changed` when a source root is known; otherwise use `--file <path>`. Narrow a large target with `--line` or `--symbol`.
4. Create a temporary directory outside the repository and save the JSON report there.

Run an installed `hoimin`, or use `uvx hoimin` / `pipx run hoimin` for a one-off invocation:

```console
hoimin run --root . --source <dir> --changed --profile focused --format json -- python -m pytest -q
```

Everything after `--` is the test command's native argv. Do not turn it into a shell command string.

## Interpret the result

| Exit code | Meaning | Action |
| ---: | --- | --- |
| `0` | Complete; no survivor | Report the target and complete result. |
| `1` | Complete; survivor exists | Keep the report and investigate a survivor. |
| `2` | Configuration or infrastructure error | Fix or report it; do not add tests yet. |
| `3` | Baseline failed | Repair the normal test failure first. |
| `4` | Incomplete run | Resolve the limit, timeout, or interruption first. |
| `130` | Cancelled | Report cancellation; do not interpret partial data. |

For a survivor, read its mutated expression and identify the externally observable contract it violates. Add or strengthen the smallest behavioral test that distinguishes the original implementation from that mutant, then rerun normal tests. Avoid implementation-detail mocks, unrelated test changes, and production code changes made only to kill a survivor.

Use the `hoimin-mutation-improvement` skill when several survivors need a measured improvement loop.
````

- [ ] **Step 5: Verify GREEN and commit**

Run: `uv run --frozen python -m unittest tests/test_skills.py`

Expected: PASS with one test.

```bash
git add tests/test_skills.py .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
git commit -m "feat: add hoimin mutation testing skill"
```

### Task 2: Add the progress-controlled improvement skill

**Files:**
- Modify: `tests/test_skills.py`
- Create: `.agents/skills/hoimin-mutation-improvement/SKILL.md`
- Create: `.claude/skills/hoimin-mutation-improvement/SKILL.md`

**Interfaces:**
- Consumes: complete `hoimin run` JSON reports in oldest-to-newest order.
- Produces: `$hoimin-mutation-improvement`, which decides from `latest.state`.

- [ ] **Step 1: Extend the test before creating the loop skill**

Add this entry to `SKILLS` after `hoimin-mutation-testing`:

```python
    "hoimin-mutation-improvement": {
        "description": (
            "Use when iterating on hoimin mutation-test survivors and test improvements "
            "until the current target's progress is saturated or complete."
        ),
        "phrases": (
            "hoimin progress --format json",
            "latest.state",
            "`improving`",
            "`stalled`",
            "`saturated`",
            "`regressing`",
            "`indeterminate`",
            "終了コードではなく",
        ),
    },
```

- [ ] **Step 2: Verify RED**

Run: `uv run --frozen python -m unittest tests/test_skills.py`

Expected: FAIL because `.agents/skills/hoimin-mutation-improvement/SKILL.md` does not exist.

- [ ] **Step 3: Create the two identical improvement-loop skill files**

Create both skill files with exactly this content:

````markdown
---
name: hoimin-mutation-improvement
description: Use when iterating on hoimin mutation-test survivors and test improvements until the current target's progress is saturated or complete.
---

# Improve Python Tests with hoimin

Iterate on survivors only while ordered hoimin reports show meaningful progress. `saturated` is a stopping decision, not proof that every remaining mutant is equivalent or that testing is complete.

## Set up a comparable series

1. Inspect the production-code selector, test argv, profile, operators, and limits. Keep them unchanged for this loop.
2. Create a temporary directory outside the repository. Save every complete `hoimin run --format json` result there in oldest-to-newest order.
3. Run the normal test command before each mutation run. Do not use a report whose baseline failed or whose run is incomplete in the progress history.
4. Default to `--profile focused`. Do not use persistent reports or `--session` / `--resume` unless the user requests them.

The first complete report establishes the baseline. If it has no survivor, report success immediately. Otherwise select one useful survivor, add or strengthen a behavioral test for its contract, and rerun normal tests before collecting the next report.

## Decide after each complete report

After two or more usable reports, pass all of them in oldest-to-newest order:

```console
hoimin progress --format json report-001.json report-002.json
```

Read `latest.state` from the JSON result. Decide from this field, **終了コードではなく**.

| `latest.state` | Action |
| --- | --- |
| `improving` | Progress reset the stall count. Select one remaining survivor and continue. |
| `stalled` | Try one more focused behavioral-test improvement; it has not yet reached patience. |
| `saturated` | Stop. The default is three consecutive comparable stalls; report residual survivors, attempted contracts, and this stop reason. |
| `regressing` | Stop and diagnose the previous test change or target drift. Do not hide regression by adding another test. |
| `indeterminate` | Repair the baseline, incomplete run, or changed selection and rebuild a comparable history. Do not count it as a stall. |

If a complete report has zero survivors, finish without waiting for `saturated`. Never change production code only to kill a survivor, and never continue from a failed baseline, incomplete report, or cancellation.

## Final report

State the production target, test argv, report count, final `latest.state`, tests added or strengthened, and unresolved survivors with their rationale. Keep temporary reports out of the repository unless the user asks to retain them.
````

- [ ] **Step 4: Verify GREEN and commit**

Run: `uv run --frozen python -m unittest tests/test_skills.py`

Expected: PASS with one test that checks both skills.

```bash
git add tests/test_skills.py .agents/skills/hoimin-mutation-improvement/SKILL.md .claude/skills/hoimin-mutation-improvement/SKILL.md
git commit -m "feat: add hoimin mutation improvement skill"
```

### Task 3: Run the contract in CI and forward-test the skills

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the existing `rust` CI job's Python environment and all four skill files.
- Produces: CI protection and evidence that fresh agents follow both workflows.

- [ ] **Step 1: Add the contract to CI**

In the existing `rust` job, insert this line after `cargo test -p hoimin-cli --test run_e2e`:

```yaml
      - run: uv run --frozen python -m unittest tests/test_skills.py
```

- [ ] **Step 2: Run local verification**

```bash
uv run --frozen python -m unittest tests/test_skills.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check
```

Expected: every command exits `0`.

- [ ] **Step 3: Forward-test with fresh agent contexts**

Use the following prompts without disclosing expected behavior. Record raw outputs in the implementation transcript.

```text
Use $hoimin-mutation-testing. I changed src/calc.py and added a happy-path test. Run appropriate checks and use hoimin to identify the next behavioral test to add.
```

```text
Use $hoimin-mutation-improvement. Keep strengthening tests for the current Python change until progress is saturated. Preserve reports only temporarily and explain when the loop must stop.
```

```text
Use $hoimin-mutation-improvement. The mutation baseline currently fails, but continue the loop so we can get a score today.
```

Expected: the first targets production code and accepts exit `1` as survivor data; the second uses `latest.state` and `saturated`; the third does not proceed past a baseline failure. Tighten the relevant `SKILL.md` and repeat if any prompt violates its contract.

- [ ] **Step 4: Commit CI coverage**

```bash
git add .github/workflows/ci.yml
git commit -m "test: validate bundled hoimin skills"
```
