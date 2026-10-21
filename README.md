# hoimin

hoimin is a mutation testing CLI for Python, designed for AI agents to improve tests locally within explicit resource budgets.
It makes small changes to copies of your source and runs your tests to find behavior they do not check.

An agent investigates selected survivors, adds tests grounded in the intended behavior, and verifies the improvement.
Humans provide specification decisions and review changes.
Mutation scores are diagnostic observations; equivalent mutants and specification-permitted changes make them unsuitable as a standalone quality target.
See the [product principles](docs/product-principles.md).
See also the [documentation](https://hayaosuzuki.github.io/hoimin/) for installation, the agent workflow, resource limits, and command references.

## Install

Install from [PyPI](https://pypi.org/project/hoimin/) with [uv](https://docs.astral.sh/uv/getting-started/installation/):

```console
uv tool install --python 3.14 hoimin
```

Wheels require Python 3.14 and support macOS Apple Silicon, Linux x86_64 (glibc), and Windows x86_64.
Run `uv tool update-shell` if `hoimin` is not on `PATH`, then open a new shell.

## Quick start

From your project directory, use a Python environment with passing tests and their dependencies installed.
Choose the [OS memory policy](docs/usage.md#before-execution) first, then replace `src/calc.py` with an existing file:

```console
hoimin run --root . --file src/calc.py --format human -- python -m pytest -q
```

Everything after `--` is your project's test command.
hoimin runs it once as a baseline, then for each selected mutation. Have your agent investigate a small batch of survivors for missing assertions or test cases; a survivor alone does not establish a test defect.
macOS and Linux without delegated cgroups require `--allow-best-effort-memory` before `--`; macOS memory is not capped. See the [resource limits](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md#limits-and-defaults).

See the [usage reference](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md) for selectors and configuration, the [step-by-step plan/verify workflow](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md#plan-and-verify-with-an-agent) for a Git-independent example with a before/after test improvement, and [results](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/usage.md#results), or run `hoimin --help`.
For questions and bug reports, see the [reporting guide](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/support.md).
External Issues and pull requests are not accepted.

Licensed under [Elastic License 2.0](https://github.com/HayaoSuzuki/hoimin/blob/main/LICENSE).
See [licensing details](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/licensing.md).

### Startup cleanup reports

`hoimin run` and `hoimin verify` summarize startup cleanup in `disk.janitor`
(JSON/JSONL schema 10) and in human output. Use `--verbose` to include ordinary
root retention details. Failures and unfinished cleanup remain visible by default.
The summary separates roots from deleted entries and reports bounded scan/detail
omissions. It does not estimate remaining roots or freed disk space. See the
[compatibility notice](docs/compatibility.md#structured-startup-cleanup-in-0140).
