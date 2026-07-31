# Disable Implicit PyPI Publication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure an ordinary version tag builds retained wheel artifacts without obtaining PyPI publication credentials or invoking a package publisher.

**Architecture:** Keep the existing tag validation and Windows/Linux wheel build jobs unchanged. Remove the default publisher job, protect the artifact-only release graph with a standard-library Python contract test, and document the explicit controls required before a future public-distribution workflow is introduced.

**Tech Stack:** GitHub Actions YAML, Python 3.14 `unittest`, Markdown.

## Global Constraints

- An ordinary `v*` tag may validate metadata, build wheels, smoke-test them, and upload GitHub Actions artifacts.
- The default tag workflow must not request `id-token: write`.
- The default tag workflow must not invoke `pypa/gh-action-pypi-publish` or another package publisher.
- Future public publication must use a separate explicit manual or controlled opt-in and a protected GitHub environment.
- Keep all action revisions pinned exactly as they are.
- Do not change wheel targets, Maturin arguments, tag validation, or smoke-test commands.

---

### Task 1: Make Tagged Releases Artifact-Only

**Files:**
- Modify: `tests/test_ci_workflow.py`
- Modify: `.github/workflows/release.yml`
- Modify: `README.md`

**Interfaces:**
- Consumes: `RELEASE_WORKFLOW`, `job_block`, and the existing standard-library workflow contract suite.
- Produces: `ReleaseWorkflowContractTests`, an artifact-only `release.yml`, and the documented future-publication controls.

- [ ] **Step 1: Add the failing release-policy contract**

Append this class to `tests/test_ci_workflow.py`:

```python
class ReleaseWorkflowContractTests(unittest.TestCase):
    def test_version_tags_build_artifacts_without_publication_credentials(
        self,
    ) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("actions/upload-artifact@", workflow)
        self.assertNotIn("\n  publish:\n", workflow)
        self.assertNotIn("id-token: write", workflow)
        self.assertNotIn("pypa/gh-action-pypi-publish@", workflow)
```

- [ ] **Step 2: Run the focused contract and verify RED**

Run:

```console
uv run --frozen python -m unittest \
  tests.test_ci_workflow.ReleaseWorkflowContractTests -v
```

Expected: the test fails because the workflow contains the current `publish`
job and its OIDC publication permission.

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

Expected: every command passes. `release.yml` still contains both wheel
artifact uploads and contains no publisher job or OIDC publication permission.

- [ ] **Step 6: Commit the implementation**

```console
git add tests/test_ci_workflow.py .github/workflows/release.yml README.md
git commit -m "release: make tagged builds artifact-only"
```
