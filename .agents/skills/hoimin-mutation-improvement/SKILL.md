---
name: hoimin-mutation-improvement
description: Use when iterating on hoimin mutation-test survivors and test improvements until the current target's progress is saturated or complete.
---

# Improve Python Tests with hoimin

Improve tests against explicitly selected planned candidates. A surviving candidate identifies a
behavioral contract to test; it does not justify changing production code only to make a mutant
fail.

## Use the same hoimin version

Keep the hoimin version used to create the plan throughout the improvement loop.
For setup on another project, follow
[Release wheel setup](../hoimin-mutation-testing/release-wheel.md): download a
prebuilt wheel with authenticated `gh` and use `uv tool install` or `uvx`.
If the version must change, regenerate the plan. To validate unreleased changes
to hoimin itself, use a build of the working tree.

## Keep one plan valid

1. Keep `PLAN.json` and verify reports in a temporary directory outside the repository.
2. Keep the plan's root, selector, profile, operators, limits, fingerprint inputs, and test argv
   unchanged while improving tests. Test-only changes are expected: every verify still runs a
   fresh baseline with those new tests.
3. Discard and regenerate the plan before verify if production target source or a fingerprint
   input changes. Also replan before changing selector, operators, profile, limits, or test argv.

## Keep verification disk-safe

Use a plan with explicit disk limits, defaulting to `--jobs 1`,
`--max-workspace-size 8GiB`, and `--min-free-space 10GiB`. Changing them requires explicit user
approval and a new plan. Stop before `verify` if free-space measurement fails, the repository or
temporary filesystem has 10 GiB or less available, or another mutation run is using the same
root. Stop the loop after a disk-limit stop, measurement failure, or failed/deferred cleanup;
diagnose it and account for any retained path before another verify.

Keep the plan and reports in the immutable `mktemp -d` path established by the planning workflow,
with its cleanup trap active for success, failure, interruption, and cancellation.
Remove only that exact temporary directory. Never delete its parent or use a glob. Treat cleanup
failure as terminal and report the retained exact path.

Before every verify or reverify, check both filesystems:

```console
python3 -c 'import shutil, sys; limit = 10 * 1024**3; sys.exit(0 if all(shutil.disk_usage(path).free > limit for path in sys.argv[1:]) else 1)' . "$temp_dir"
```

## Improve one selected candidate

1. Read a candidate ID from `PLAN.json` and verify it. Preserve the report.

```console
hoimin verify "$plan_path" --candidate '<ID_FROM_PLAN_JSON>' --format json \
  > "$temp_dir/verify-001.json"
```

2. If it survives, read its original expression, replacement, symbol, and line. Add or strengthen
   the smallest behavioral test that observes the violated contract. Do not add implementation-
   detail mocks, unrelated tests, or production-code changes made only to kill the mutant.
3. Run the normal test command. If it fails, repair or report that failure before verifying again.
4. Reverify the same candidate. A completed killed result closes that candidate; a survivor needs
   another contract investigation; a baseline failure, incomplete run, cancellation, or error
   stops the loop for diagnosis.

## Compare and stop honestly

Use `hoimin progress --format json` only when every supplied report covers the identical candidate-ID set.
Do not use arbitrary one-candidate or changing-subset verify reports to infer
improvement, stalls, or saturation. When a planned candidate remains unverified, say so. When
`PLAN.json` was truncated, also say that candidates outside its retained partial set were never
enumerated.

## Final report

State the production target, test argv, plan profile and fingerprint inputs, selected candidate
IDs, each candidate's final status, tests added or strengthened, any replan reason, and unverified
or truncated-away candidates with their rationale. Keep temporary manifests and reports out of the
repository unless the user asks to retain them.
