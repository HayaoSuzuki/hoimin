# Building and releasing hoimin

## Build and verify a wheel

The package is a native binary wheel, not a Python extension module. Build and smoke-test the wheel locally with:

```console
uv run maturin build --release
uv sync --frozen --no-install-project
uv run --frozen --no-sync python tests/wheel_smoke.py
```

The smoke test installs the wheel into a new environment and runs the Rust-only CLI outside this checkout. For development verification, see [the development guide](development.md). Start design and audit work with [the OKF catalog](knowledge/index.md), and follow [the OKF workflow](okf-workflow.md) to keep it current with each relevant change.

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin build --release
uv sync --frozen --no-install-project
uv run --frozen --no-sync python tests/wheel_smoke.py
```

Windows Job Object tests run on Windows and Linux hard-limit tests require a delegated cgroup v2 runner. The ordinary Linux CI job verifies the explicit best-effort path separately.

## GitHub Releases

Merging a pull request into `main` starts `.github/workflows/release.yml`.
It reserves a `vMAJOR.MINOR.PATCH` tag on the merged commit, builds and checks
all three platforms, then publishes a [GitHub Release](https://github.com/HayaoSuzuki/hoimin/releases)
with generated release notes and `SHA256SUMS`:

| Platform | Standalone executable archive | Python wheel |
| --- | --- | --- |
| Windows x86_64 | `hoimin-v<VERSION>-windows-x86_64.zip` | `win_amd64` |
| Linux x86_64 | `hoimin-v<VERSION>-linux-x86_64.tar.gz` | manylinux2014 |
| macOS Apple Silicon | `hoimin-v<VERSION>-macos-aarch64.tar.gz` | macOS arm64 |

For normal use, install from [PyPI](https://pypi.org/project/hoimin/):

```console
uv tool install --python 3.14 hoimin
```

GitHub Releases retain standalone archives and wheels as release artifacts.
The standalone Linux executable is built on Ubuntu 22.04; the wheel uses
manylinux2014 for broader glibc compatibility. Wheels require Python 3.14.

The first release uses the workspace version (currently `0.1.0`). Later
merges increment the highest stable tag's patch version. Raising the workspace
version can establish a higher minimum version for the next release; keep
`Cargo.toml`, `pyproject.toml`, `Cargo.lock`, and `uv.lock` consistent when
changing it. CI embeds the reserved version into those four files in its
build checkout, without committing version changes back to `main`.

Before reserving a new tag, CI checks that the commit descends from the
highest stable tag's commit, fetching complete history when needed. A delayed
run for an older or divergent commit is skipped without a tag, build, or
release. Each reservation atomically creates the version tag and advances the
`hoimin-release-state` branch to that commit, conditional on the previously
observed branch SHA. A competing reservation rejects the entire push and forces
a fresh history check, even if the runs chose different version numbers.
Existing tags remain reusable, so retries of an older, already tagged release
still work.

The workflow creates `hoimin-release-state` on the first new reservation. Reserve
that branch for automation: do not delete, rewind, or push application changes
to it. Repository rules must allow the workflow to create/update this branch
and create version tags. Version tags are never overwritten; if either update
is rejected, the atomic push writes neither ref.
All concurrent automated reservations must use this protocol; manually created
tags or runs of an older workflow do not participate in its shared lease.

PRs and manual runs build preview packages (`-dev.<RUN_ID>`) and retain them
as Actions artifacts. They do not create tags or releases. For a manual check:

```console
gh workflow run release.yml --ref <BRANCH>
```

If a merged-PR run fails, rerun that Actions run. It reuses the tag for the
same commit, resumes a draft release, and leaves an already published release
unchanged. All three builds and wheel smoke tests must succeed before
publication. Manually pushing a tag does not trigger this workflow.
Publication explicitly uses GitHub's `make_latest=legacy` selection by version
and date, so a delayed older release does not become `Latest` merely by finishing
last. There is no separate client-side read/compare/update of `Latest` that could
race with another run. The GitHub API owns that selection; local tests check the
outgoing request, while hosted execution remains the integration check.

PyPI publication uses the separate, manually triggered `publish-pypi.yml`
workflow. Select a published ELv2 release tag and either TestPyPI (the default)
or PyPI. It verifies all three wheels against `SHA256SUMS`, checks their package
metadata and bundled licenses, and uploads the same wheel bytes through Trusted
Publishing. Only the upload job has `id-token: write`; its GitHub environment
restricts deployment to `main` and can require approval where the GitHub plan
supports reviewers. Without reviewers, manual dispatch proceeds to upload after
validation. See [PyPI publishing](pypi-publishing.md) for the initial
account and environment setup, first publication, and retry procedure.

