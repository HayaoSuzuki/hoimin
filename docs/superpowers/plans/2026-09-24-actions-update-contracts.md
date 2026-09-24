# Combined Actions updates and contract repair

## Design

PRs #579/#580/#581 update actions/cache to v4.3.0, actions/checkout to
v6.1.0 and astral-sh/setup-uv to v8.3.2. Their Wheel smoke failures are
comparisons against duplicated old action SHAs in `tests/test_ci_workflow.py`.
Create one independent branch from current main with all three verified pins.
Apply the updates consistently to all workflows, including commentless canary
and boundary jobs missed by the original Renovate patches; add version comments
to those updated uses for subsequent dependency discovery.

Workflow files remain the source of exact 40-character commit pins. Structural
tests validate the pin syntax, then compare action identities (`owner/repo`)
without copying SHA values into expected fixtures. Only step `uses` values are
normalized. Repository identity, permissions, inputs, job/step structure, shell,
environment and artifact-only release restrictions remain exact. A separate
inventory test checks every workflow's actions against the existing identities
and full SHA requirement. It does not require different jobs to use one shared
version, or assert that every syntactically valid hash exists upstream.

## Implementation plan

1. Run the original workflow suite; update only the three workflow pins and
   reproduce the SHA comparison failures. Official GitHub tag refs have already
   confirmed each proposed commit.
2. Add `workflow_contract(text)` to parse YAML, validate complete remote-action
   references, and replace only parsed step action references with their identity.
   Use it at exact action-comparison boundaries, keeping raw YAML where complete
   automatic/manual step equality is intentional. Expected action constants hold
   identities only; retain all other expected release fields.
3. Add tests accepting new complete pins and rejecting tags/branches/short or
   malformed hashes/non-string references and different repositories. Exercise
   all workflow files, release configuration changes and existing hostile-release
   cases. Use valid 40-character pins for hostile action fixtures so repository
   checks, not malformed-pin rejection, detect those changes.
4. Document the pin-versus-workflow-contract boundary and update relevant OKF
   source references. Run the complete Python suite and an independent review.
   Rust source, compiler, lockfile and wheel implementation are unchanged, so
   repeated Rust compilation is not required for this workflow/test-only change.
5. Complete three implementation/test self-reviews, record actual results, commit
   and create one PR replacing the three Renovate proposals. Do not modify or
   close their branches/PRs, merge main, or dispatch release publishing.

## Design self-reviews

1. Cause: logs identify old SHA equality in Lean cache, native Windows jobs and
   release shape assertions, not failed Action execution. Fix the duplicated
   expected revision while retaining immutable workflow pins.
2. Trust boundary: stripping arbitrary suffixes would admit floating versions or
   attacker repositories. Validate the entire reference first; compare identities
   independently and leave permissions/inputs/scripts untouched.
3. Coverage boundary: existing upload detection and auto/manual consistency use
   raw action strings. Keep those call sites raw rather than replacing every YAML
   load and silently bypassing their guards.

## Plan self-reviews

1. Update completeness: inspect every workflow, including missing version comments.
   Preserve all non-reference configuration and use the upstream tag's exact SHA.
2. Regressions: require actual updated workflows to fail before the test fix, then
   retain positive alternate-pin cases and negatives with valid malicious pins.
3. Verification: full Python tests include workflow/release/wheel contracts. Run
   with normal process inspection for the existing Lean resource guard tests.
   No production Python changes means hoimin mutation testing has no eligible
   production target; its skill excludes mutating test modules.

## Execution and final reviews

Pending execution evidence.
