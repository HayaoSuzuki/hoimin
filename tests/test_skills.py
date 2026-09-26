from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SKILLS = {
    "hoimin-mutation-testing": {
        "description": (
            "Use when developing or changing Python code and automated "
            "tests, and hoimin mutation testing can expose missing "
            "behavioral coverage."
        ),
        "required_blocks": (
            (
                "1. Inspect changed production Python files, their tests, and "
                "the normal test command. Target\n"
                "   production code, never test modules. Prefer `--source <dir>"
                " --changed`; otherwise use\n"
                "   `--file <path>`, `--line`, or `--symbol`."
            ),
            (
                "2. Run the normal test command. Stop and repair or report a "
                "failure before planning."
            ),
            (
                "3. Create a temporary directory outside the repository. Keep "
                "the root, selector, profile,\n"
                "   operators, limits, test argv, and fingerprint inputs "
                "unchanged for every verify that consumes\n"
                "   its plan."
            ),
            (
                "hoimin plan --root . --source <dir> --changed --profile "
                "focused \\\n"
                "  --jobs 1 --max-workspace-size 8GiB --min-free-space 10GiB \\\n"
                "  --fingerprint-include pyproject.toml -- python -m pytest -q "
                '> "$plan_path"'
            ),
            (
                "`plan` always writes one JSON document to standard output and "
                "diagnostics to standard error;\n"
                "do not pass `--format` to `plan`."
            ),
            (
                "Read `candidates[].id` from `PLAN.json`; choose one ID and "
                "pass that exact ID to `verify`:"
            ),
            (
                "Everything after `--` in `plan` remains the normal test "
                "command's native argv; do not turn it\n"
                "into a shell command string. `plan` runs no baseline, test "
                "command, worker copy, or session.\n"
                "Each `verify` runs a fresh baseline and never reuses a "
                "session."
            ),
            (
                "- `verify` exit 0 means the selected candidates completed with"
                " no survivor. Exit 1 means at least\n"
                "  one selected candidate survived. Exit 3 means the fresh "
                "baseline failed. Exit 4 is incomplete;\n"
                "  exit 130 is cancelled. For exits 2, 3, 4, or 130, stop and "
                "diagnose before changing tests."
            ),
            (
                "- A `plan.source.changed` or `plan.fingerprint_input.changed` "
                "rejection means the manifest is\n"
                "  stale. Regenerate it before any further verify. Also replan "
                "when selector, operators, profile,\n"
                "  limits, or test argv must change."
            ),
        ),
    },
    "hoimin-mutation-improvement": {
        "description": (
            "Use when iterating on hoimin mutation-test survivors and test "
            "improvements until the current target's progress is saturated "
            "or complete."
        ),
        "required_blocks": (
            (
                "1. Keep `PLAN.json` and verify reports in a temporary "
                "directory outside the repository."
            ),
            (
                "2. Keep the plan's root, selector, profile, operators, limits,"
                " fingerprint inputs, and test argv\n"
                "   unchanged while improving tests. Test-only changes are "
                "expected: every verify still runs a\n"
                "   fresh baseline with those new tests."
            ),
            (
                "3. Discard and regenerate the plan before verify if production"
                " target source or a fingerprint\n"
                "   input changes. Also replan before changing selector, "
                "operators, profile, limits, or test argv."
            ),
            (
                "1. Read a candidate ID from `PLAN.json` and verify it. "
                "Preserve the report."
            ),
            (
                "2. If it survives, read its original expression, replacement, "
                "symbol, and line. Add or strengthen\n"
                "   the smallest behavioral test that observes the violated "
                "contract. Do not add implementation-\n"
                "   detail mocks, unrelated tests, or production-code changes "
                "made only to kill the mutant."
            ),
            (
                "3. Run the normal test command. If it fails, repair or report "
                "that failure before verifying again."
            ),
            (
                "4. Reverify the same candidate. A completed killed result "
                "closes that candidate; a survivor needs\n"
                "   another contract investigation; a baseline failure, "
                "incomplete run, cancellation, or error\n"
                "   stops the loop for diagnosis."
            ),
            (
                "Use `hoimin progress --format json` only when every supplied "
                "report covers the identical candidate-ID set.\n"
                "Do not use arbitrary one-candidate or changing-subset verify "
                "reports to infer\n"
                "improvement, stalls, or saturation. When a planned candidate "
                "remains unverified, say so. When\n"
                "`PLAN.json` was truncated, also say that candidates outside "
                "its retained partial set were never\n"
                "enumerated."
            ),
            (
                "State the production target, test argv, plan profile and "
                "fingerprint inputs, selected candidate\n"
                "IDs, each candidate's final status, tests added or "
                "strengthened, any replan reason, and unverified\n"
                "or truncated-away candidates with their rationale."
            ),
        ),
    },
}


REQUIRED_DISK_SAFETY_PHRASES = {
    "hoimin-mutation-testing": (
        "--jobs 1",
        "--max-workspace-size 8GiB",
        "--min-free-space 10GiB",
        "Stop before `plan` or `verify`",
        "Remove only that exact temporary directory",
        "trap cleanup_temp_dir EXIT",
        "trap 'exit 130' INT",
    ),
    "hoimin-mutation-improvement": (
        "--jobs 1",
        "--max-workspace-size 8GiB",
        "--min-free-space 10GiB",
        "Stop before `verify`",
        "Remove only that exact temporary directory",
    ),
}


@pytest.mark.parametrize("root", [".agents", ".claude"])
@pytest.mark.parametrize(
    ("name", "phrase"),
    [
        (name, phrase)
        for name, phrases in REQUIRED_DISK_SAFETY_PHRASES.items()
        for phrase in phrases
    ],
)
def test_mutation_skills_require_disk_safe_execution(
    root: str, name: str, phrase: str
) -> None:
    skill = ROOT / root / "skills" / name / "SKILL.md"
    assert phrase in skill.read_text(encoding="utf-8")


def test_macos_memory_policy_is_documented() -> None:
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    assert "--max-memory" in readme
    assert "not enforced" in readme


@pytest.mark.parametrize("root", [".agents", ".claude"])
def test_macos_memory_policy_is_documented_in_skills(root: str) -> None:
    skill = ROOT / root / "skills" / "hoimin-mutation-testing" / "SKILL.md"
    assert "--allow-best-effort-memory" in skill.read_text(encoding="utf-8")


@pytest.mark.parametrize("name", SKILLS)
def test_skill_mirrors_and_frontmatter(name: str) -> None:
    contract = SKILLS[name]
    codex = ROOT / ".agents" / "skills" / name / "SKILL.md"
    claude = ROOT / ".claude" / "skills" / name / "SKILL.md"
    assert codex.is_file(), codex
    assert claude.is_file(), claude
    assert codex.read_bytes() == claude.read_bytes()

    lines = codex.read_text(encoding="utf-8").splitlines()
    assert len(lines) >= 4
    assert lines[0] == "---"
    closing = lines.index("---", 1)
    frontmatter = dict(line.split(": ", 1) for line in lines[1:closing])
    assert frontmatter == {"name": name, "description": contract["description"]}
    assert frontmatter["description"].startswith("Use when")


@pytest.mark.parametrize(
    ("name", "block"),
    [
        (name, block)
        for name, contract in SKILLS.items()
        for block in contract["required_blocks"]
    ],
)
def test_skill_required_workflows(name: str, block: str) -> None:
    skill = ROOT / ".agents" / "skills" / name / "SKILL.md"
    lines = skill.read_text(encoding="utf-8").splitlines()
    closing = lines.index("---", 1)
    body = "\n".join(lines[closing + 1 :])
    assert block in body
