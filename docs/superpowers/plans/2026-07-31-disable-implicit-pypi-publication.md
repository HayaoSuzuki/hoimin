# Disable Implicit PyPI Publication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure an ordinary version tag builds retained wheel artifacts without obtaining PyPI publication credentials or invoking a package publisher.

**Architecture:** Keep the existing tag validation and Windows/Linux wheel build jobs unchanged. Remove the default publisher job, protect the artifact-only release graph with a standard-library Python contract test, and document the explicit controls required before a future public-distribution workflow is introduced.

**Tech Stack:** GitHub Actions YAML, Python 3.14 `unittest`, Markdown.

## Global Constraints

- An ordinary `v*` tag may validate metadata, build wheels, smoke-test them, and upload GitHub Actions artifacts.
- The default tag workflow job set must be exactly `validate-tag`,
  `windows-wheel`, and `linux-wheel`.
- The default tag workflow must not request `id-token: write`.
- The default tag workflow must not reference a GitHub environment or secret.
- Every `uses:` entry must match the exact pinned checkout, setup-python,
  setup-uv, Maturin, or upload-artifact action already required to build the
  retained wheels.
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

**Interfaces:**
- Consumes: `RELEASE_WORKFLOW`, `job_block`, and the existing standard-library workflow contract suite.
- Produces: `ReleaseWorkflowContractTests`, an artifact-only `release.yml`, and the documented future-publication controls.

- [ ] **Step 1: Add the failing release-policy contract**

Add a shared assertion and contract tests to `tests/test_ci_workflow.py`.
The assertion must require the exact three-job graph, both exact pinned wheel
uploads and their artifact names and paths, the exact action allowlist, and no
job-level environment, secret reference, or OIDC write permission:

```python
UPLOAD_ARTIFACT_ACTION = (
    "actions/upload-artifact@"
    "ea165f8d65b6e75b540449e92b4886f43607fa02"
)
ALLOWED_RELEASE_ACTIONS = {
    "actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10",
    "actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1",
    "astral-sh/setup-uv@08807647e7069bb48b6ef5acd8ec9567f424441b",
    "PyO3/maturin-action@e83996d129638aa358a18fbd1dfb82f0b0fb5d3b",
    UPLOAD_ARTIFACT_ACTION,
}


def assert_artifact_only_release(test: unittest.TestCase, workflow: str) -> None:
    jobs = workflow[workflow.index("jobs:\n") + len("jobs:\n") :]
    test.assertEqual(
        set(
            re.findall(
                r"^  ([A-Za-z_][A-Za-z0-9_-]*):[ \t]*(?:#.*)?$",
                jobs,
                re.MULTILINE,
            )
        ),
        {"validate-tag", "windows-wheel", "linux-wheel"},
    )

    for job_name, artifact_name in (
        ("windows-wheel", "wheels-windows-x86_64"),
        ("linux-wheel", "wheels-linux-x86_64"),
    ):
        wheel = job_block(workflow, job_name)
        test.assertEqual(wheel.count(f"- uses: {UPLOAD_ARTIFACT_ACTION}"), 1)
        test.assertRegex(
            wheel,
            rf"(?m)^      - uses: {re.escape(UPLOAD_ARTIFACT_ACTION)}"
            rf"(?:[ \t]+#.*)?\n"
            rf"        with:\n"
            rf"          name: {re.escape(artifact_name)}\n"
            rf"          path: target/wheels/\*\.whl$",
        )

    actions = set(
        re.findall(
            r"^[ \t]+- uses[ \t]*:[ \t]*([^ \t#\r\n]+)",
            workflow,
            re.MULTILINE,
        )
    )
    test.assertEqual(actions, ALLOWED_RELEASE_ACTIONS)
    test.assertNotRegex(workflow, r"(?m)^    environment[ \t]*:")
    test.assertNotIn("${{ secrets.", workflow)
    test.assertNotRegex(
        workflow,
        r"(?mi)^[ \t]*id-token[ \t]*:[ \t]*['\"]?write['\"]?"
        r"[ \t]*(?:#.*)?$",
    )


class ReleaseWorkflowContractTests(unittest.TestCase):
    def test_version_tags_build_artifacts_without_publication_credentials(
        self,
    ) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        assert_artifact_only_release(self, workflow)
```

Add a hostile-fixture test that proves the assertion rejects an aliased
publisher job, an unexpected action, altered artifact names or paths, a
job-level environment, a secret reference, and an OIDC write permission.

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
artifact names and paths, uses only the exact allowed pinned actions, and
contains no job-level environment, secret reference, or OIDC write permission.

- [ ] **Step 6: Commit the implementation**

```console
git add tests/test_ci_workflow.py .github/workflows/release.yml README.md
git commit -m "release: make tagged builds artifact-only"
```
