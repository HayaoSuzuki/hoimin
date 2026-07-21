# Agent Mutation Skills Plan-First Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update the repository-local mutation-testing skills so agents plan candidates once and verify selected, unchanged candidates while improving Python tests.

**Architecture:** Keep two focused skills: `hoimin-mutation-testing` owns normal-test preflight, plan generation, and first candidate verification; `hoimin-mutation-improvement` owns the survivor-to-test loop and controlled replanning. Each skill is edited in `.agents` first and copied byte-for-byte to its matching `.claude` path.

**Tech Stack:** Markdown skill files, `hoimin plan`, `hoimin verify`, `hoimin progress`, `cmp`, `rg`, and Git.

## Global Constraints

- Modify only the four mirrored skill files named below; do not change hoimin executable behavior, CI configuration, or report schemas.
- Keep every `.agents/skills/<name>/SKILL.md` byte-identical to `.claude/skills/<name>/SKILL.md`.
- Store `PLAN.json` and verify reports in a temporary directory outside the repository.
- Use `--fingerprint-include` for root-relative behavior-affecting configuration or fixtures not represented by selected production sources; use `--include` only when normal worker-copy policy would otherwise omit a required file.
- Treat `plan` exit 4 as a partial but usable manifest; treat `plan` exit 2 as no usable manifest.
- Replan before verify when target source, fingerprint input, selector, operator selection, profile, limits, or test argv changes.
- Use `hoimin progress` only for reports with the identical candidate-ID set; never infer saturation from arbitrary partial verify reports.
- Every skill-only commit message includes `[skip ci]`.

---

## File Structure

- Modify: `.agents/skills/hoimin-mutation-testing/SKILL.md` — canonical plan-first discovery and first verification workflow.
- Modify: `.claude/skills/hoimin-mutation-testing/SKILL.md` — byte-identical mirror of the canonical testing workflow.
- Modify: `.agents/skills/hoimin-mutation-improvement/SKILL.md` — canonical selected-survivor improvement, reverify, and stopping workflow.
- Modify: `.claude/skills/hoimin-mutation-improvement/SKILL.md` — byte-identical mirror of the canonical improvement workflow.

### Task 1: Make candidate discovery plan-first

**Files:**
- Modify: `.agents/skills/hoimin-mutation-testing/SKILL.md:1-40`
- Modify: `.claude/skills/hoimin-mutation-testing/SKILL.md:1-40`

**Interfaces:**
- Consumes: a changed production target, its normal test command, and any explicit fingerprint inputs.
- Produces: a temporary `PLAN.json`, an explicitly selected candidate ID, and a first `verify` report; hands surviving candidates to `hoimin-mutation-improvement`.

- [ ] **Step 1: Prove the current skill lacks the plan-first workflow**

Run:

```bash
rg -F "hoimin plan" .agents/skills/hoimin-mutation-testing/SKILL.md
```

Expected: exit 1 because the current skill only documents `hoimin run`.

- [ ] **Step 2: Replace the canonical testing skill with the exact plan-first guidance**

Replace `.agents/skills/hoimin-mutation-testing/SKILL.md` with front matter retaining
`name: hoimin-mutation-testing` and this operational content:

```markdown
# Test Python Changes with hoimin

Use a read-only plan to choose mutation candidates, then verify only the candidates whose
behavioral contracts need investigation. A survivor is evidence to investigate, not a reason to
modify production code solely to make the mutant fail.

## Plan candidates

1. Inspect changed production Python files, their tests, and the normal test command. Target
   production code, never test modules. Prefer `--source <dir> --changed`; otherwise use
   `--file <path>`, `--line`, or `--symbol`.
2. Run the normal test command. Stop and repair or report a failure before planning.
3. Create a temporary directory outside the repository. Keep the root, selector, profile,
   operators, limits, test argv, and fingerprint inputs unchanged for every verify that consumes
   its plan.

```console
temp_dir="$(mktemp -d)"
plan_path="$temp_dir/PLAN.json"
hoimin plan --root . --source <dir> --changed --profile focused \\
  --fingerprint-include pyproject.toml -- python -m pytest -q > "$plan_path"
```

`--fingerprint-include` records root-relative configuration or fixtures that affect test behavior
but are not selected production sources. It does not copy files into workers. Add `--include`
only when normal worker-copy policy would otherwise omit a required file.

## Select and verify candidates

Read `candidates[].id` from `PLAN.json`; choose one ID and pass that exact ID to `verify`:

```console
hoimin verify "$plan_path" --candidate '<ID_FROM_PLAN_JSON>' --format json \\
  > "$temp_dir/verify-001.json"
```

Everything after `--` in `plan` remains the normal test command's native argv; do not turn it
into a shell command string. `plan` runs no baseline, test command, worker copy, or session.
Each `verify` runs a fresh baseline and never reuses a session.

## Interpret plan and verify results

- `plan` exit 0 produces a complete manifest. Exit 4 produces a partial manifest; IDs present in
  it are still valid verification choices. Exit 2 writes no usable manifest: repair the input and
  plan again.
- `verify` exit 0 means the selected candidates completed with no survivor. Exit 1 means at least
  one selected candidate survived. Exit 3 means the fresh baseline failed. Exit 4 is incomplete;
  exit 130 is cancelled. For exits 2, 3, 4, or 130, stop and diagnose before changing tests.
- A `plan.source.changed` or `plan.fingerprint_input.changed` rejection means the manifest is
  stale. Regenerate it before any further verify. Also replan when selector, operators, profile,
  limits, or test argv must change.

For a survivor, identify the externally observable contract its mutated expression violates and
use `hoimin-mutation-improvement` for the test-improvement loop.
```

- [ ] **Step 3: Mirror the canonical testing skill exactly**

Copy the final `.agents` file to `.claude/skills/hoimin-mutation-testing/SKILL.md`, then run:

```bash
cmp -s .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
rg -F "hoimin plan" .agents/skills/hoimin-mutation-testing/SKILL.md
rg -F "plan.source.changed" .agents/skills/hoimin-mutation-testing/SKILL.md
git diff --check
```

Expected: every command exits 0.

- [ ] **Step 4: Commit the plan-first testing skill**

```bash
git add .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
git commit -m "docs: adopt plan-first mutation testing skill [skip ci]"
```

### Task 2: Rework survivor improvement around selected verification

**Files:**
- Modify: `.agents/skills/hoimin-mutation-improvement/SKILL.md:1-41`
- Modify: `.claude/skills/hoimin-mutation-improvement/SKILL.md:1-41`

**Interfaces:**
- Consumes: a valid `PLAN.json`, one or more selected candidate IDs, a passing normal test command, and verify reports.
- Produces: reverified behavioral tests, an explicit replan decision when configuration drifts, and a report that distinguishes verified from unverified candidates.

- [ ] **Step 1: Prove the current improvement skill still requires full run reports**

Run:

```bash
rg -F "hoimin verify" .agents/skills/hoimin-mutation-improvement/SKILL.md
```

Expected: exit 1 because the current skill records only `hoimin run --format json` reports.

- [ ] **Step 2: Replace the canonical improvement skill with the exact selected-candidate loop**

Replace `.agents/skills/hoimin-mutation-improvement/SKILL.md` with front matter retaining
`name: hoimin-mutation-improvement` and this operational content:

```markdown
# Improve Python Tests with hoimin

Improve tests against explicitly selected planned candidates. A surviving candidate identifies a
behavioral contract to test; it does not justify changing production code only to make a mutant
fail.

## Keep one plan valid

1. Keep `PLAN.json` and verify reports in a temporary directory outside the repository.
2. Keep the plan's root, selector, profile, operators, limits, fingerprint inputs, and test argv
   unchanged while improving tests. Test-only changes are expected: every verify still runs a
   fresh baseline with those new tests.
3. Discard and regenerate the plan before verify if production target source or a fingerprint
   input changes. Also replan before changing selector, operators, profile, limits, or test argv.

## Improve one selected candidate

1. Read a candidate ID from `PLAN.json` and verify it. Preserve the report.

```console
hoimin verify "$plan_path" --candidate '<ID_FROM_PLAN_JSON>' --format json \\
  > "$temp_dir/verify-001.json"
```

2. If it survives, read its original expression, replacement, symbol, and line. Add or strengthen
   the smallest behavioral test that observes the violated contract. Do not add implementation-
   detail mocks, unrelated tests, or production-code changes made only to kill the mutant.
3. Run the normal test command. If it fails, repair or report that failure before verifying again.
4. Reverify the same candidate. A completed killed result closes that candidate; a survivor needs
   another contract investigation; a baseline failure, incomplete run, cancellation, or error
   stops the loop for diagnosis.

## Compare and stop honestly

Use `hoimin progress --format json` only when every supplied report covers the identical
candidate-ID set. Do not use arbitrary one-candidate or changing-subset verify reports to infer
improvement, stalls, or saturation. When a planned candidate remains unverified, say so. When
`PLAN.json` was truncated, also say that candidates outside its retained partial set were never
enumerated.

## Final report

State the production target, test argv, plan profile and fingerprint inputs, selected candidate
IDs, each candidate's final status, tests added or strengthened, any replan reason, and unverified
or truncated-away candidates with their rationale. Keep temporary manifests and reports out of the
repository unless the user asks to retain them.
```

- [ ] **Step 3: Mirror the canonical improvement skill exactly**

Copy the final `.agents` file to `.claude/skills/hoimin-mutation-improvement/SKILL.md`, then run:

```bash
cmp -s .agents/skills/hoimin-mutation-improvement/SKILL.md .claude/skills/hoimin-mutation-improvement/SKILL.md
rg -F "hoimin verify" .agents/skills/hoimin-mutation-improvement/SKILL.md
rg -F "identical candidate-ID set" .agents/skills/hoimin-mutation-improvement/SKILL.md
git diff --check
```

Expected: every command exits 0.

- [ ] **Step 4: Commit the selected-candidate improvement skill**

```bash
git add .agents/skills/hoimin-mutation-improvement/SKILL.md .claude/skills/hoimin-mutation-improvement/SKILL.md
git commit -m "docs: guide selected mutation verification [skip ci]"
```

### Task 3: Validate the mirrored skill set and skip-CI history

**Files:**
- Verify: `.agents/skills/hoimin-mutation-testing/SKILL.md`
- Verify: `.claude/skills/hoimin-mutation-testing/SKILL.md`
- Verify: `.agents/skills/hoimin-mutation-improvement/SKILL.md`
- Verify: `.claude/skills/hoimin-mutation-improvement/SKILL.md`

**Interfaces:**
- Consumes: the two commits from Tasks 1 and 2.
- Produces: proof that each mirror matches and each skill-update commit suppresses CI.

- [ ] **Step 1: Validate both mirrors and required workflow terms**

Run:

```bash
cmp -s .agents/skills/hoimin-mutation-testing/SKILL.md .claude/skills/hoimin-mutation-testing/SKILL.md
cmp -s .agents/skills/hoimin-mutation-improvement/SKILL.md .claude/skills/hoimin-mutation-improvement/SKILL.md
rg -F "hoimin plan" .agents/skills/hoimin-mutation-testing/SKILL.md
rg -F "hoimin verify" .agents/skills/hoimin-mutation-improvement/SKILL.md
rg -F "--fingerprint-include" .agents/skills/hoimin-mutation-testing/SKILL.md
rg -F "identical candidate-ID set" .agents/skills/hoimin-mutation-improvement/SKILL.md
git diff --check HEAD~2..HEAD
```

Expected: every command exits 0.

- [ ] **Step 2: Verify both skill commits carry the CI-skip marker**

Run:

```bash
git log -2 --format=%s | rg '^docs: .+ \[skip ci\]$'
```

Expected: exactly two matching commit subjects.

- [ ] **Step 3: Report validation without another commit**

Report that both mirror comparisons and all required-term checks passed, and list the two
`[skip ci]` skill-update commits. Do not create an empty validation commit.

## Plan Self-Review

- Spec coverage: Task 1 implements plan generation, fingerprint-input guidance, plan/verify exit handling, and replan conditions. Task 2 implements selected-survivor improvement, comparable-progress constraints, truncated-plan reporting, and final reporting. Task 3 covers mirroring and `[skip ci]` validation.
- Placeholder scan: all file paths, commands, expected results, commit messages, and replacement text are explicit; user-provided project selectors and candidate IDs intentionally remain documented placeholders inside the skills.
- Consistency: Tasks 1 and 2 make `.agents` canonical and copy to `.claude`; Task 3 compares the same two pairs and checks the exact terms introduced by the prior tasks.
