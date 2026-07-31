# Disable Implicit PyPI Publication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure an ordinary version tag builds retained wheel artifacts without obtaining PyPI publication credentials or invoking a package publisher.

**Architecture:** Keep the existing tag validation and Windows/Linux wheel build jobs unchanged. Remove the default publisher job, decode the release YAML with `yaml.safe_load`, enforce an exact artifact-only job/action/command graph over the decoded structures, and document the explicit controls required before a future public-distribution workflow is introduced.

**Tech Stack:** GitHub Actions YAML, Python 3.14 `unittest`, PyYAML 6.x, Markdown.

## Global Constraints

- An ordinary `v*` tag may validate metadata, build wheels, smoke-test them, and upload GitHub Actions artifacts.
- The default tag workflow job set must be exactly `validate-tag`,
  `windows-wheel`, and `linux-wheel`.
- The decoded default tag workflow must not contain an `id-token` key.
- The default tag workflow must not reference a GitHub environment or secret.
- Every `uses:` entry must match the exact pinned checkout, setup-python,
  setup-uv, Maturin, or upload-artifact action already required to build the
  retained wheels.
- Security decisions must use decoded YAML structures, not raw-text regexes,
  so alternate YAML key syntax and escape sequences cannot bypass the policy.
- Future public publication must use a separate explicit manual or controlled opt-in and a protected GitHub environment.
- Keep all action revisions pinned exactly as they are.
- Do not change wheel targets, Maturin arguments, tag validation, or smoke-test commands.

---

### Task 1: Make Tagged Releases Artifact-Only

**Files:**
- Modify: `tests/test_ci_workflow.py`
- Modify: `.github/workflows/release.yml`
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-07-31-disable-implicit-pypi-publication.md`
- Modify: `pyproject.toml`
- Modify: `uv.lock`

**Interfaces:**
- Consumes: `RELEASE_WORKFLOW`, `yaml.safe_load`, and the existing workflow contract suite.
- Produces: `ReleaseWorkflowContractTests`, an artifact-only `release.yml`, and the documented future-publication controls.

- [ ] **Step 0: Add the YAML parser test dependency**

```console
uv add --dev 'pyyaml>=6.0.2,<7'
```

- [ ] **Step 1: Add the failing release-policy contract**

Add a shared assertion and contract tests to `tests/test_ci_workflow.py`.
The assertion must require the exact three-job graph, both exact pinned wheel
uploads and their artifact names and paths, the exact action allowlist, and no
job-level environment, secret reference, or write permission. It must decode
all YAML representations with `yaml.safe_load`, require the exact current
decoded action and command multisets, and recursively inspect decoded keys and
values:

```python
UPLOAD_ARTIFACT_ACTION = (
    "actions/upload-artifact@"
    "ea165f8d65b6e75b540449e92b4886f43607fa02"
)
EXPECTED_RELEASE_ACTIONS = Counter(
    {
        "actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10": 3,
        "actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1": 3,
        "astral-sh/setup-uv@08807647e7069bb48b6ef5acd8ec9567f424441b": 2,
        "PyO3/maturin-action@e83996d129638aa358a18fbd1dfb82f0b0fb5d3b": 2,
        UPLOAD_ARTIFACT_ACTION: 2,
    }
)
EXPECTED_RELEASE_RUN_COMMANDS = Counter(
    {
        f"{TAG_VALIDATION_COMMAND}\n": 1,
        "uv run --frozen python tests/wheel_smoke.py": 2,
    }
)


def assert_artifact_only_release(test: unittest.TestCase, workflow: str) -> None:
    decoded = yaml.safe_load(workflow)
    test.assertEqual(decoded["permissions"], {"contents": "read"})
    jobs = decoded["jobs"]
    test.assertEqual(set(jobs), {"validate-tag", "windows-wheel", "linux-wheel"})

    actions = Counter()
    run_commands = Counter()
    for job in jobs.values():
        test.assertNotIn("environment", job)
        for permission in job.get("permissions", {}).values():
            test.assertNotEqual(str(permission).casefold(), "write")
        for step in job["steps"]:
            if "uses" in step:
                actions[step["uses"]] += 1
            if "run" in step:
                run_commands[step["run"]] += 1

    test.assertEqual(actions, EXPECTED_RELEASE_ACTIONS)
    test.assertEqual(run_commands, EXPECTED_RELEASE_RUN_COMMANDS)
    for job_name, artifact_name in (
        ("windows-wheel", "wheels-windows-x86_64"),
        ("linux-wheel", "wheels-linux-x86_64"),
    ):
        uploads = [
            step
            for step in jobs[job_name]["steps"]
            if step.get("uses") == UPLOAD_ARTIFACT_ACTION
        ]
        test.assertEqual(len(uploads), 1)
        test.assertEqual(
            uploads[0]["with"],
            {"name": artifact_name, "path": "target/wheels/*.whl"},
        )

    for value in decoded_strings(decoded):
        test.assertNotEqual(value.casefold(), "id-token")
        test.assertIsNone(re.search(r"\bsecrets\s*(?:\.|\[)", value, re.I))


class ReleaseWorkflowContractTests(unittest.TestCase):
    def test_version_tags_build_artifacts_without_publication_credentials(
        self,
    ) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        assert_artifact_only_release(self, workflow)
```

Add a hostile-fixture test that proves the assertion rejects an aliased
publisher job, an unexpected action, altered artifact names or paths, a
job-level environment, dot and bracket secret references, block and flow OIDC
permissions, added publisher commands, quoted structural keys, explicit
mapping keys, and escaped OIDC keys.

- [ ] **Step 2: Run the focused contract and verify RED**

Run:

```console
uv run --frozen python -m unittest \
  tests.test_ci_workflow.ReleaseWorkflowContractTests -v
```

Expected: the contract fails because the workflow contains a fourth job and
publication action, environment, and OIDC permission. The hostile-fixture test
must also be observed failing against the earlier weak string checks before the
shared assertion is strengthened.

- [ ] **Step 3: Remove the default publisher**

Delete the complete `publish` job from `.github/workflows/release.yml`,
starting at:

```yaml
  publish:
```

and ending with the pinned `pypa/gh-action-pypi-publish` step. Do not change
the `validate-tag`, `windows-wheel`, or `linux-wheel` jobs.

- [ ] **Step 4: Document the artifact-only release policy**

Replace the final release paragraph in `README.md` with:

```markdown
Releases are built only from matching `v*` tags. Version tags build and retain wheel artifacts without publishing them to PyPI. Public distribution is not enabled.

Before enabling public distribution, add a separate manually triggered workflow, protect its GitHub environment with required reviewers or equivalent rules, and register PyPI Trusted Publishing for only that workflow and environment. Grant `id-token: write` only to its publication job, and update the workflow contract tests in the same change.
```

- [ ] **Step 5: Run focused and full verification**

Run:

```console
uv run --frozen python -m unittest tests.test_ci_workflow -v
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check
```

Expected: every command passes. `release.yml` has exactly the three expected
jobs, contains both exact pinned wheel artifact uploads with the expected
artifact names and paths, uses only the exact expected pinned action multiset,
runs only the exact tag-validation and two wheel-smoke commands, and contains
no decoded environment, secret context, OIDC key, or write permission.

- [ ] **Step 6: Commit the implementation**

```console
git add tests/test_ci_workflow.py .github/workflows/release.yml README.md \
  pyproject.toml uv.lock \
  docs/superpowers/plans/2026-07-31-disable-implicit-pypi-publication.md
git commit -m "release: make tagged builds artifact-only"
```
