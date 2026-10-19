---
name: hoimin-mutation-testing
license: Elastic-2.0
description: Use when developing or changing Python code and automated tests, and hoimin mutation testing can expose missing behavioral coverage.
---

# Test Python Changes with hoimin

Use a read-only plan to choose mutation candidates, then verify only the candidates whose
behavioral contracts need investigation. A survivor is evidence to investigate, not a reason to
modify production code solely to make the mutant fail.

This is a local test-improvement workflow for an external AI agent. Humans resolve
unclear requirements and review changes. Investigate a small batch under an agreed
budget; do not seek a target kill ratio or assume every survivor needs a new test.
Equivalent and specification-permitted changes can survive useful tests.

## Respect resource stops

Memory, disk, process, output, and time limits protect the developer's machine
during unattended iteration. Check the reported backend and size per-root memory
caps together with concurrency; the analyzer itself is outside descendant memory
limits. Hard memory enforcement depends on the platform/backend, and disk monitoring
is cooperative without a quota. These limits do not make arbitrary test code a sandbox.

Stop and diagnose resource-limit, measurement, or cleanup failures. Keep incomplete,
cancelled, error, and not-run results distinct from conclusive outcomes. Do not
silently relax limits or retry an unchanged resource-exhausting run. Consult the
user before increasing agreed resource consumption or accepting weaker enforcement.

In schema-9 JSON `mutants[]` and JSONL `mutant_finished`, read the derived `status`
for the candidate verdict. Do not infer killed from a nonzero exit alone:
`Exit(1)` with `outcome.executed.output_state: "close_timed_out"` is `error`.
Strict readers reject status/outcome disagreement with exit 2; generic JSON Schema
validation checks shape, not that semantic agreement. Reused killed/survived
outcomes describe saved evidence, and `not_run` can include a started process
cancelled before classified completion.

## Obtain hoimin

When using hoimin on another project, install from PyPI:

```console
uv tool install --python 3.14 hoimin
```

Follow [PyPI setup](installation.md) for platform requirements, version pinning,
and the `uvx` alternative. Keep the same hoimin version throughout plan and
verify. When validating unreleased changes to hoimin itself, build the working
tree instead: a released wheel cannot test those changes.

The [usage guide](https://hayaosuzuki.github.io/hoimin/docs/usage/)
documents target selection, plan/verify, reports, and platform limits. This skill
can be installed independently; use the improvement skill when it is available
or follow the usage guide when investigating survivors.

## Keep disk use bounded

Default every plan to `--jobs 1`, `--max-workspace-size 8GiB`, and
`--min-free-space 10GiB`. Keep those limits unchanged for every verify that consumes the plan.
Raising the workspace limit or job count, or lowering the free-space reserve, requires explicit
user approval and a new plan.

Stop before `plan` or `verify` if free-space measurement fails, the repository or temporary
filesystem has 10 GiB or less available, or another mutation run is using the same root. Stop the
loop after a disk-limit stop, measurement failure, or failed/deferred cleanup; diagnose it and
account for any retained path before another verify.

Keep all manifests and reports in one `mktemp -d` directory outside the repository. Install a
cleanup trap immediately and keep the resolved path immutable.
Remove only that exact temporary directory. Do this on success, failure, interruption, or
cancellation; never delete its parent or use a glob. Treat cleanup failure as terminal and report
the retained exact path.

## Plan candidates

1. Inspect changed production Python files, their tests, and the normal test command. Target
   production code, never test modules. Prefer `--source <dir> --changed`; otherwise use
   `--file <path>`, `--line`, or `--symbol`.
2. Run the normal test command. Stop and repair or report a failure before planning.
3. Create a temporary directory outside the repository. Keep the root, selector, profile,
   operators, limits, test argv, and fingerprint inputs unchanged for every verify that consumes
   its plan.

```console
readonly temp_dir="$(mktemp -d)"
plan_path="$temp_dir/PLAN.json"
cleanup_temp_dir() {
  rm -rf -- "$temp_dir"
  test ! -e "$temp_dir"
}
trap cleanup_temp_dir EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
python3 -c 'import shutil, sys; limit = 10 * 1024**3; sys.exit(0 if all(shutil.disk_usage(path).free > limit for path in sys.argv[1:]) else 1)' . "$temp_dir"
hoimin plan --root . --source <dir> --changed --profile focused \
  --jobs 1 --max-workspace-size 8GiB --min-free-space 10GiB \
  --fingerprint-include pyproject.toml -- python -m pytest -q > "$plan_path"
```

`plan` always writes one JSON document to standard output and diagnostics to standard error;
do not pass `--format` to `plan`.

`--fingerprint-include` records root-relative configuration or fixtures that affect test behavior
but are not selected production sources. It does not copy files into workers. Add `--include`
only when normal worker-copy policy would otherwise omit a required file.

On macOS, `--max-memory` is accepted for plan compatibility but is not enforced. CPU-time limits
and process-group cleanup remain available. When this best-effort memory policy is acceptable,
include `--allow-best-effort-memory` in the plan:

```console
hoimin plan --root . --source <dir> --allow-best-effort-memory \
  --jobs 1 --max-workspace-size 8GiB --min-free-space 10GiB -- python -m pytest -q
```

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

For a survivor, investigate whether its mutated expression violates an externally
observable requirement. Use `hoimin-mutation-improvement` for justified test changes,
or report why the candidate is unhelpful, suspected equivalent, or unresolved.
An agent's equivalence assessment is not a proof; do not turn it into a kill or
silently remove it from the mutation-score denominator.
