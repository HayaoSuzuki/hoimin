---
name: hoimin-mutation-improvement
description: Use when iterating on hoimin mutation-test survivors and test improvements until the current target's progress is saturated or complete.
---

# Improve Python Tests with hoimin

Improve tests against explicitly selected planned candidates. A surviving candidate identifies a
behavioral contract to test; it does not justify changing production code only to make a mutant
fail.

## Keep one plan valid

1. Keep `PLAN.json` and verify reports in a temporary directory outside the repository.
2. Keep the plan's root, selector, profile, operators, limits, fingerprint inputs, and test argv
   unchanged while improving tests. Test-only changes are expected: every verify still runs a
   fresh baseline with those new tests.
3. Discard and regenerate the plan before verify if production target source or a fingerprint
   input changes. Also replan before changing selector, operators, profile, limits, or test argv.

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
