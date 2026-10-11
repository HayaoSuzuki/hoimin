# hoimin

hoimin is a mutation-testing CLI for Python, designed for AI agents to improve
tests locally within explicit resource budgets. It makes small changes to copies
of your source and runs your tests to find behavior they do not check.

An agent investigates selected survivors, adds tests grounded in the intended
behavior, and verifies the improvement. Humans provide specification decisions
and review changes. Mutation scores are diagnostic observations; equivalent
mutants and specification-permitted changes make them unsuitable as a standalone
quality target. CI use is optional. See the [product principles](docs/product-principles.md).

Browse the [searchable main documentation](https://hayaosuzuki.github.io/hoimin/)
after its initial Pages activation. The existing references below remain available
on GitHub. Maintainers can follow the [publication guide](docs/documentation.md).

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
baseline, then for each selected mutation. Have your agent investigate a small
batch of survivors for missing assertions or test cases; a survivor alone does
not establish a test defect. `--allow-best-effort-memory` permits execution
on macOS and Linux without delegated cgroup limits; see the
[resource limits](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md#limits-and-defaults).

See the [usage reference](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md)
for selectors and configuration, the
[plan/verify workflow](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md#plan-and-verify-with-an-agent),
and [results](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md#results),
or run `hoimin --help`.
For questions and bug reports, see the [reporting guide](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/support.md).
External Issues and pull requests are not accepted.
For maintainer development, see [development](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/development.md)
and [building and releases](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/releases.md).

Licensed under [Elastic License 2.0](https://github.com/HayaoSuzuki/hoimin/blob/main/LICENSE).
See [licensing details](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/licensing.md).
