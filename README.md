# hoimin

hoimin is a mutation-testing CLI for Python. It makes small changes to copies of
your source and runs your tests to find behavior they do not check.

## Install

Install from [PyPI](https://pypi.org/project/hoimin/) with
[uv](https://docs.astral.sh/uv/getting-started/installation/):

```console
uv tool install --python 3.14 hoimin
```

Wheels require Python 3.14 and support macOS Apple Silicon, Linux x86_64
(glibc), and Windows x86_64. Run `uv tool update-shell` if `hoimin` is not on
`PATH`, then open a new shell.

## Quick start

From your project directory, use a Python environment with your test dependencies
installed. Replace `src/calc.py` with the file you want to check:

```console
hoimin run --root . --file src/calc.py --allow-best-effort-memory --format human -- python -m pytest -q
```

Everything after `--` is your project's test command. hoimin runs it once as a
baseline, then for each selected mutation. Inspect surviving mutations to find
missing assertions or test cases. `--allow-best-effort-memory` permits execution
on macOS and Linux without delegated cgroup limits; see the
[resource limits](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md#limits-and-defaults).

See the [usage reference](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md)
for selectors, plan/verify workflows, reports, and configuration, or run `hoimin --help`.
For contributing, see [development](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/development.md)
and [building and releases](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/releases.md).

Licensed under [Elastic License 2.0](https://github.com/HayaoSuzuki/hoimin/blob/main/LICENSE).
See [licensing details](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/licensing.md).
