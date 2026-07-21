# Plan/Verify Skill Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align the executable bundled-skill contract with the shipped plan/verify workflow and restore every CI job without weakening coverage.

**Architecture:** `tests/test_skills.py` remains the single semantic contract for both mirrored skill packages. Replace only the retired workflow blocks with complete safety-critical blocks already present in the skill bodies; production code and skill prose remain unchanged.

**Tech Stack:** Python 3.14 `unittest`, Markdown skill packages, Cargo workspace CI

## Global Constraints

- Preserve byte-identical `.agents` and `.claude` skill mirrors.
- Preserve exact frontmatter name and description checks.
- Test user-visible workflow semantics, not Markdown hashes or line positions.
- Do not change production behavior to satisfy this CI repair.

---

### Task 1: Replace the retired skill workflow contract

**Files:**
- Modify: `tests/test_skills.py`
- Test: `tests/test_skills.py`

**Interfaces:**
- Consumes: Markdown bodies from both bundled `SKILL.md` mirrors.
- Produces: `SKILLS[name]["required_blocks"]`, the durable plan/verify safety contract used by `SkillContractTests`.

- [ ] **Step 1: Verify the existing regression failure**

Run:

```bash
UV_CACHE_DIR=/private/tmp/hoimin-uv-cache uv run --frozen python -m unittest tests/test_skills.py -v
```

Expected: one test method with twelve subtest failures because required `run`/`progress` prose is absent from the plan/verify skills.

- [ ] **Step 2: Replace old mutation-testing blocks**

Replace its `required_blocks` tuple with:

```python
(
    "1. Inspect changed production Python files, their tests, and the normal test command. Target\n"
    "   production code, never test modules. Prefer `--source <dir> --changed`; otherwise use\n"
    "   `--file <path>`, `--line`, or `--symbol`.",
    "2. Run the normal test command. Stop and repair or report a failure before planning.",
    "3. Create a temporary directory outside the repository. Keep the root, selector, profile,\n"
    "   operators, limits, test argv, and fingerprint inputs unchanged for every verify that consumes\n"
    "   its plan.",
    "hoimin plan --root . --source <dir> --changed --profile focused \\\n"
    "  --fingerprint-include pyproject.toml -- python -m pytest -q > \"$plan_path\"",
    "Read `candidates[].id` from `PLAN.json`; choose one ID and pass that exact ID to `verify`:",
    "Everything after `--` in `plan` remains the normal test command's native argv; do not turn it\n"
    "into a shell command string. `plan` runs no baseline, test command, worker copy, or session.\n"
    "Each `verify` runs a fresh baseline and never reuses a session.",
    "- `verify` exit 0 means the selected candidates completed with no survivor. Exit 1 means at least\n"
    "  one selected candidate survived. Exit 3 means the fresh baseline failed. Exit 4 is incomplete;\n"
    "  exit 130 is cancelled. For exits 2, 3, 4, or 130, stop and diagnose before changing tests.",
    "- A `plan.source.changed` or `plan.fingerprint_input.changed` rejection means the manifest is\n"
    "  stale. Regenerate it before any further verify. Also replan when selector, operators, profile,\n"
    "  limits, or test argv must change.",
)
```

- [ ] **Step 3: Replace old mutation-improvement blocks**

Replace its `required_blocks` tuple with:

```python
(
    "1. Keep `PLAN.json` and verify reports in a temporary directory outside the repository.",
    "2. Keep the plan's root, selector, profile, operators, limits, fingerprint inputs, and test argv\n"
    "   unchanged while improving tests. Test-only changes are expected: every verify still runs a\n"
    "   fresh baseline with those new tests.",
    "3. Discard and regenerate the plan before verify if production target source or a fingerprint\n"
    "   input changes. Also replan before changing selector, operators, profile, limits, or test argv.",
    "1. Read a candidate ID from `PLAN.json` and verify it. Preserve the report.",
    "2. If it survives, read its original expression, replacement, symbol, and line. Add or strengthen\n"
    "   the smallest behavioral test that observes the violated contract. Do not add implementation-\n"
    "   detail mocks, unrelated tests, or production-code changes made only to kill the mutant.",
    "3. Run the normal test command. If it fails, repair or report that failure before verifying again.",
    "4. Reverify the same candidate. A completed killed result closes that candidate; a survivor needs\n"
    "   another contract investigation; a baseline failure, incomplete run, cancellation, or error\n"
    "   stops the loop for diagnosis.",
    "Use `hoimin progress --format json` only when every supplied report covers the identical candidate-ID set.\n"
    "Do not use arbitrary one-candidate or changing-subset verify reports to infer\n"
    "improvement, stalls, or saturation. When a planned candidate remains unverified, say so. When\n"
    "`PLAN.json` was truncated, also say that candidates outside its retained partial set were never\n"
    "enumerated.",
    "State the production target, test argv, plan profile and fingerprint inputs, selected candidate\n"
    "IDs, each candidate's final status, tests added or strengthened, any replan reason, and unverified\n"
    "or truncated-away candidates with their rationale.",
)
```

- [ ] **Step 4: Verify the focused contract passes**

Run:

```bash
UV_CACHE_DIR=/private/tmp/hoimin-uv-cache uv run --frozen python -m unittest tests/test_skills.py -v
```

Expected: `Ran 1 test` and `OK`.

- [ ] **Step 5: Verify CI-equivalent suites**

Run:

```bash
UV_CACHE_DIR=/private/tmp/hoimin-uv-cache uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Expected: every command exits zero with no test failure, formatting difference, warning, or whitespace error.

- [ ] **Step 6: Commit the repair**

```bash
git add tests/test_skills.py docs/superpowers/specs/2026-07-21-plan-verify-skill-contract-design.md docs/superpowers/plans/2026-07-21-plan-verify-skill-contract.md
git commit -m "fix: align skill contract with plan workflow"
```
