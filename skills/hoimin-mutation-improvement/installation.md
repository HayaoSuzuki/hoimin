# Install hoimin from PyPI

Use [uv](https://docs.astral.sh/uv/guides/tools/) to install the published
[hoimin package](https://pypi.org/project/hoimin/):

```console
uv tool install --python 3.14 hoimin
hoimin --version
```

Wheels require Python 3.14 and support macOS Apple Silicon (arm64), Linux x86_64
with glibc, and Windows x86_64. Use a native arm64 shell on Apple Silicon.
If `hoimin` is not on `PATH`, run `uv tool update-shell` and open a new shell.
The project under test still needs its own Python environment and test dependencies.

## Keep one version for plan and verify

Record `hoimin --version` before planning. Keep that installation unchanged
throughout the improvement loop. To select a specific published PyPI version,
replace `X.Y.Z` below with that version:

```console
uv tool install --reinstall --python 3.14 'hoimin==X.Y.Z'
hoimin --version
```

Use this command when replacing an existing installation from a local wheel too.
Check that the reported version matches the requested version. If you change
versions, regenerate the plan before verifying.

## Run with uvx instead

For an invocation without a persistent tool installation, pin the package version:

```console
uvx --python 3.14 --from 'hoimin==X.Y.Z' hoimin --help
```

Replace each `hoimin` command in the workflow with this same pinned prefix,
using one published version for the plan and every verify.

If package resolution or platform compatibility fails, report the cause before
planning. When testing unreleased hoimin changes, build the working tree so
that the executable contains those changes.
