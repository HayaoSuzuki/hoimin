# Disable Implicit PyPI Publication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure an ordinary version tag builds retained wheel artifacts without obtaining PyPI publication credentials or invoking a package publisher.

**Architecture:** Keep the existing tag validation and Windows/Linux wheel build jobs unchanged. Remove the default publisher job, decode the release YAML with `yaml.safe_load`, enforce exact equality between the complete decoded workflow document and a hand-authored expected mapping, and document the explicit controls required before a future public-distribution workflow is introduced.

**Tech Stack:** GitHub Actions YAML, Python 3.14 `unittest`, PyYAML 6.x, Markdown.

## Global Constraints

- An ordinary `v*` tag may validate metadata, build wheels, smoke-test them, and upload GitHub Actions artifacts.
- The complete decoded workflow document must equal the hand-authored expected
  mapping for the exact name, version-tag trigger, read-only permissions, and
  jobs. No additional top-level key is permitted.
- The expected `jobs` mapping must contain exactly `validate-tag`,
  `windows-wheel`, and `linux-wheel`, including each job's dependencies,
  runner, and full ordered step mappings.
- Every expected step field is part of that equality: action SHAs, `with`
  options, names, environment values, and run commands. Extra fields such as
  `shell`, credentials, or alternate Maturin commands must fail the contract.
- Top-level permissions must equal `{"contents": "read"}`.
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
Decode all YAML representations with `yaml.safe_load`. Hand-author
`EXPECTED_RELEASE_JOBS` as the complete nested mapping of the three current
jobs and every ordered step. Include every `needs`, `runs-on`, action SHA,
`with` mapping, step `name`, `env`, and `run` field, including both exact
artifact names and paths. Then hand-author `EXPECTED_RELEASE_WORKFLOW` with the
exact workflow name, trigger, permissions, and jobs. PyYAML applies YAML 1.1
boolean resolution, so the unquoted top-level `on` key must be represented by
the `True` key in this decoded expectation. Require exact equality with the
entire decoded workflow document:

```python
EXPECTED_RELEASE_JOBS = {
    "validate-tag": {
        "runs-on": "ubuntu-latest",
        "steps": [
            # Complete expected step mappings, with no omitted fields.
        ],
    },
    "windows-wheel": {
        "needs": "validate-tag",
        "runs-on": "windows-latest",
        "steps": [
            # Complete expected step mappings, with no omitted fields.
        ],
    },
    "linux-wheel": {
        "needs": "validate-tag",
        "runs-on": "ubuntu-latest",
        "steps": [
            # Complete expected step mappings, with no omitted fields.
        ],
    },
}
EXPECTED_RELEASE_WORKFLOW = {
    "name": "Release wheels",
    # PyYAML's YAML 1.1 resolver decodes the unquoted `on` key as `True`.
    True: {"push": {"tags": ["v*"]}},
    "permissions": {"contents": "read"},
    "jobs": EXPECTED_RELEASE_JOBS,
}


def assert_artifact_only_release(test: unittest.TestCase, workflow: str) -> None:
    decoded = yaml.safe_load(workflow)
    test.assertEqual(decoded, EXPECTED_RELEASE_WORKFLOW)


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
mapping keys, escaped OIDC keys, a custom publishing shell on an expected smoke
command, a literal publication token environment, Maturin `command: publish`,
and a combined top-level credential environment plus
`defaults.run.shell` publisher.

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

Expected: every command passes. The complete decoded `release.yml` document
equals the exact expected mapping for `name`, the version-tag trigger
(`True` after PyYAML YAML 1.1 decoding of the unquoted `on` key), read-only
permissions, and the three expected jobs. The jobs include runner/dependency
fields, all pinned action SHAs, every action `with` mapping, the tag-validation
name/environment/command, the two wheel-smoke commands, and exact wheel
artifact names and paths. Any added or changed top-level, job, or step field
fails.

- [ ] **Step 6: Commit the implementation**

```console
git add tests/test_ci_workflow.py .github/workflows/release.yml README.md \
  pyproject.toml uv.lock \
  docs/superpowers/plans/2026-07-31-disable-implicit-pypi-publication.md
git commit -m "release: make tagged builds artifact-only"
```
