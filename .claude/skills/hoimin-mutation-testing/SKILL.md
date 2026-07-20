---
name: hoimin-mutation-testing
description: Use when developing or changing Python code and automated tests, and hoimin mutation testing can expose missing behavioral coverage.
---

# Test Python Changes with hoimin

Use hoimin on changed production code to find behavioral gaps in automated tests. A survivor is evidence to investigate, not a reason to modify production code solely to make the mutant fail.

## Workflow

1. Inspect the changed production Python files, their existing tests, and the project's normal test command.
2. Run the normal test command first. If it fails, repair or report the failure before mutation testing.
3. Target production code, never the test module. Use `--source <dir> --changed` when a source root is known; otherwise use `--file <path>`. Narrow a large target with `--line` or `--symbol`.
4. Create a temporary directory outside the repository and save the JSON report there.

Run an installed `hoimin`, or use `uvx hoimin` / `pipx run hoimin` for a one-off invocation:

```console
hoimin run --root . --source <dir> --changed --profile focused --format json -- python -m pytest -q
```

Everything after `--` is the test command's native argv. Do not turn it into a shell command string.

## Interpret the result

| Exit code | Meaning | Action |
| ---: | --- | --- |
| `0` | Complete; no survivor | Report the target and complete result. |
| `1` | Complete; survivor exists | Keep the report and investigate a survivor. |
| `2` | Configuration or infrastructure error | Fix or report it; do not add tests yet. |
| `3` | Baseline failed | Repair the normal test failure first. |
| `4` | Incomplete run | Resolve the limit, timeout, or interruption first. |
| `130` | Cancelled | Report cancellation; do not interpret partial data. |

For a survivor, read its mutated expression and identify the externally observable contract it violates. Add or strengthen the smallest behavioral test that distinguishes the original implementation from that mutant, then rerun normal tests. Avoid implementation-detail mocks, unrelated test changes, and production code changes made only to kill a survivor.

Use the `hoimin-mutation-improvement` skill when several survivors need a measured improvement loop.
