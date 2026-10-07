# Publishing hoimin wheels to PyPI

Use `.github/workflows/publish-pypi.yml` to publish an existing GitHub Release.
The workflow runs manually from `main` in `HayaoSuzuki/hoimin`. Select a stable
release tag and an index: `testpypi` (default) or `pypi`.

The workflow downloads the three native wheels and `SHA256SUMS`. It checks that
the tag belongs to `main`, that the release is published and not a prerelease,
and that the wheel filenames, SHA-256 digests, package metadata and license
texts match the expected release. It stages only the verified wheels. A separate
job uploads those same bytes using PyPI Trusted Publishing and creates publish
attestations. It does not build or execute downloaded code in the upload job.

License comparisons allow only the CRLF/LF line-ending difference introduced by
Windows checkouts. Other text and whitespace differences are rejected. Wheel
bytes and SHA-256 checks remain unchanged.

Supported distributions remain Windows x86-64, manylinux2014 x86-64 and macOS
11+ arm64, with Python `>=3.14,<3.15`. This workflow publishes wheels only.
Standalone executable archives and `SHA256SUMS` remain on GitHub Releases.

## One-time setup

1. Sign in to the PyPI account that will own `hoimin`, verify its email address,
   and enable two-factor authentication. Use a separate account on TestPyPI
   to test the publication process.
2. In [GitHub repository environments](https://github.com/HayaoSuzuki/hoimin/settings/environments),
   create `pypi` and `testpypi`. Set **Deployment branches and tags** to
   **Selected branches and tags** and add a **Branch** rule for `main` in both.
   Add a required reviewer if the repository visibility and GitHub plan support
   it. For private repositories on GitHub Pro or Team, **Required reviewers**
   and **Prevent self-review** are unavailable; use the branch restriction and
   manual workflow dispatch. In that configuration, dispatch starts publication
   after validation without a second approval prompt. Where reviewers are
   available, leave **Prevent self-review** disabled for a sole maintainer who
   approves their own runs. These protections require configuration on GitHub;
   the workflow file does not install them. See [GitHub's availability rules](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments#required-reviewers).
3. Register a pending Trusted Publisher using the following values on
   [PyPI account publishing](https://pypi.org/manage/account/publishing/) and
   [TestPyPI account publishing](https://test.pypi.org/manage/account/publishing/).

| Field | PyPI | TestPyPI |
| --- | --- | --- |
| PyPI project name | `hoimin` | `hoimin` |
| GitHub owner | `HayaoSuzuki` | `HayaoSuzuki` |
| Repository name | `hoimin` | `hoimin` |
| Workflow filename | `publish-pypi.yml` | `publish-pypi.yml` |
| Environment name | `pypi` | `testpypi` |

The workflow filename excludes `.github/workflows/`. Register only this workflow,
not `release.yml`. No PyPI API token or GitHub repository secret is needed.
If the project already exists under the publishing account, add the same values
under the project's **Publishing** settings instead of creating a pending publisher.

As of the preparation check on 2026-10-07, both project JSON endpoints returned
404. This does not guarantee that the name can be registered. A pending publisher
does not reserve the name; PyPI creates the project on the first successful
publication. See [creating a project with Trusted Publishing](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/).

## First publication

Merge the preparation branch into `main` and wait for the existing **Build and
release** workflow to finish creating a GitHub Release. Use a release containing
both the ELv2 change and the updated `HayaoSuzuki/hoimin` package URLs. The earlier
`v0.1.17` release predates ELv2 and fails the license checks. The workspace version
is a release-version floor; the existing release workflow assigns the next tag
and writes that version into the wheel metadata before building.

Inspect the release and replace `vX.Y.Z` in these commands with its actual tag.
First publish to TestPyPI:

```sh
gh release view vX.Y.Z --repo HayaoSuzuki/hoimin
gh workflow run publish-pypi.yml --repo HayaoSuzuki/hoimin --ref main \
  -f tag=vX.Y.Z -f index=testpypi
```

Review the selected release before dispatching. Open the run in GitHub Actions
and approve the `testpypi` environment if required reviewers are configured;
otherwise the upload proceeds after validation. On a supported OS and
architecture, check installation with an isolated Python 3.14 environment:

```sh
uv venv --python 3.14 /tmp/hoimin-testpypi
uv pip install --python /tmp/hoimin-testpypi/bin/python \
  --index-url https://test.pypi.org/simple/ --only-binary=:all: 'hoimin==X.Y.Z'
/tmp/hoimin-testpypi/bin/hoimin --version
/tmp/hoimin-testpypi/bin/hoimin --help
```

The commands above use Unix paths. On Windows, use the environment's
`Scripts/python.exe` and `Scripts/hoimin.exe`. hoimin's wheel has no runtime
Python dependencies, so this check does not need an additional package index.

Publish the same tag to PyPI after the TestPyPI check:

```sh
gh workflow run publish-pypi.yml --repo HayaoSuzuki/hoimin --ref main \
  -f tag=vX.Y.Z -f index=pypi
```

If required reviewers are configured, approve the `pypi` environment after
reviewing the run. Otherwise dispatch authorizes the upload. Then confirm the
project page, the three wheel files, license, repository links and installation:

```sh
uvx --python 3.14 --from 'hoimin==X.Y.Z' hoimin --version
```

## Failures and retries

The workflow fails before staging if a wheel is missing, corrupt, from another
version, or carries different license metadata or license texts. Compare the
selected tag and GitHub Release assets; do not bypass the checks or relabel an
older wheel. The checksums protect against accidental mismatches; they do not
prove the origin of assets if an administrator replaces both assets and checksums.

For OIDC errors, compare the repository owner, workflow filename and environment
with the registration on the selected index. PyPI and TestPyPI registrations are
independent. Runs started from a branch other than `main` skip publication.

If no files were uploaded, fix the configuration and retry the same tag. When
the workflow or validator code changes, merge the fix and start a new manual run
from `main`. GitHub's **Re-run** retains the original commit and will not pick up
the fix; see [re-running workflows](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/re-run-workflows-and-jobs).
Duplicate
uploads fail: `skip-existing` is intentionally disabled, as recommended by the
[publishing action](https://github.com/pypa/gh-action-pypi-publish#tolerating-release-package-file-duplicates).
If an upload stops after publishing some files, inspect the index's files and
digests before taking further action. The workflow does not automatically repair
a partial publication. Publish a new release version when a clean retry is needed;
do not delete an existing release expecting to reuse its filenames.

## References and verification scope

- [PyPI Trusted Publishing](https://docs.pypi.org/trusted-publishers/)
- [Adding a publisher and configuring environments](https://docs.pypi.org/trusted-publishers/adding-a-publisher/)
- [Publishing with OIDC and TestPyPI](https://docs.pypi.org/trusted-publishers/using-a-publisher/)
- [Validator](../tools/pypi_release.py) and [tests](../tests/test_pypi_release.py)

Local checks validate artifact selection, metadata, licenses, checksums and
workflow permissions. They do not exercise GitHub's environment approval or
PyPI's OIDC token exchange. The first TestPyPI run verifies those integrations.
