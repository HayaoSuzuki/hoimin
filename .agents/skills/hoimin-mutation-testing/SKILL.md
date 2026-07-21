---
name: hoimin-mutation-testing
description: Use when developing or changing Python code and automated tests, and hoimin mutation testing can expose missing behavioral coverage.
---

# Test Python Changes with hoimin

Use a read-only plan to choose mutation candidates, then verify only the candidates whose
behavioral contracts need investigation. A survivor is evidence to investigate, not a reason to
modify production code solely to make the mutant fail.

## Plan candidates

1. Inspect changed production Python files, their tests, and the normal test command. Target
   production code, never test modules. Prefer `--source <dir> --changed`; otherwise use
   `--file <path>`, `--line`, or `--symbol`.
2. Run the normal test command. Stop and repair or report a failure before planning.
3. Create a temporary directory outside the repository. Keep the root, selector, profile,
   operators, limits, test argv, and fingerprint inputs unchanged for every verify that consumes
   its plan.

```console
temp_dir="$(mktemp -d)"
plan_path="$temp_dir/PLAN.json"
hoimin plan --root . --source <dir> --changed --profile focused \
  --fingerprint-include pyproject.toml -- python -m pytest -q > "$plan_path"
```

`plan` always writes one JSON document to standard output and diagnostics to standard error;
do not pass `--format` to `plan`.

`--fingerprint-include` records root-relative configuration or fixtures that affect test behavior
but are not selected production sources. It does not copy files into workers. Add `--include`
only when normal worker-copy policy would otherwise omit a required file.

## Select and verify candidates

Read `candidates[].id` from `PLAN.json`; choose one ID and pass that exact ID to `verify`:

```console
hoimin verify "$plan_path" --candidate '<ID_FROM_PLAN_JSON>' --format json \
  > "$temp_dir/verify-001.json"
```

Everything after `--` in `plan` remains the normal test command's native argv; do not turn it
into a shell command string. `plan` runs no baseline, test command, worker copy, or session.
Each `verify` runs a fresh baseline and never reuses a session.

## Interpret plan and verify results

- `plan` exit 0 produces a complete manifest. Exit 4 produces a partial manifest; IDs present in
  it are still valid verification choices. Exit 2 writes no usable manifest: repair the input and
  plan again.
- `verify` exit 0 means the selected candidates completed with no survivor. Exit 1 means at least
  one selected candidate survived. Exit 3 means the fresh baseline failed. Exit 4 is incomplete;
  exit 130 is cancelled. For exits 2, 3, 4, or 130, stop and diagnose before changing tests.
- A `plan.source.changed` or `plan.fingerprint_input.changed` rejection means the manifest is
  stale. Regenerate it before any further verify. Also replan when selector, operators, profile,
  limits, or test argv must change.

For a survivor, identify the externally observable contract its mutated expression violates and
use `hoimin-mutation-improvement` for the test-improvement loop.
