# Manual Paid CI Runners Design

## Goal

Keep pull-request and `main` CI automatic on Ubuntu while ensuring GitHub-hosted
Windows and macOS runners are created only by an explicit manual dispatch. A
manual dispatch must default to the Ubuntu-only configuration so an operator
cannot incur paid-runner usage by accepting the dialog unchanged.

## Current Behavior

`.github/workflows/ci.yml` statically expands four OS matrices. Every pull
request and push to `main` therefore creates Windows and macOS jobs for
`quality`, `rust`, and `wheel-smoke`, plus a Windows
`core-dependency-purity` job. This happens again for each CI-triggering update.

The release workflow is separate: a version tag creates release artifacts.
This change does not alter release validation or publishing behavior.

## Decision

Add a required Boolean `workflow_dispatch` input named `run_paid_runners` with
the default `false`. Each affected matrix chooses one of two JSON arrays:

| Invocation | Three-OS jobs | Core purity |
| --- | --- | --- |
| Pull request | Ubuntu | Ubuntu |
| Push to `main` | Ubuntu | Ubuntu |
| Manual, input `false` | Ubuntu | Ubuntu |
| Manual, input `true` | Ubuntu, Windows, macOS | Ubuntu, Windows |

The selection is evaluated in `strategy.matrix.os` with `fromJSON`. It is not a
step condition: a skipped step still requires a runner. It is not a job-level
condition based on `matrix.os`: GitHub evaluates `jobs.<job_id>.if` before
matrix expansion. Dynamic matrix selection prevents the paid jobs from being
created at all.

The manual full-platform run is:

```console
gh workflow run ci.yml --ref <REF> -f run_paid_runners=true
```

Run it once for the final commit that needs cross-platform evidence. Ordinary
pushes and pull requests continue to exercise the complete Ubuntu gate.

## Files and Contracts

- `.github/workflows/ci.yml` owns the dispatch input and the four dynamic
  matrices.
- `tests/test_ci_workflow.py` checks the fail-closed default, the exact affected
  jobs, the allowed runner labels, and the manual guard in every matrix.
- `docs/development.md` explains when and how to request paid runners.

No application code, release workflow, scheduled canary, self-hosted cgroup
job, or mutation-test configuration changes.

## Verification

Run the workflow contract suite, all repository Python tests, YAML parsing,
formatting checks, and `git diff --check` locally. Do not dispatch Windows or
macOS merely to validate this policy change; the contract is configuration-only
and the purpose of the change is to avoid unnecessary paid execution.

## Self-Review Record

### Review 1: Trigger and Billing Boundary

Rejected filtering at the step level because the paid runner would already be
allocated. Rejected `jobs.if` with `matrix.os` because GitHub evaluates the job
condition before expanding the matrix. Dynamic matrix selection is the earliest
point that has both trigger inputs and control over runner creation.

### Review 2: Scope and Compatibility

Enumerated every Windows or macOS entry in the CI workflow. The four matrix jobs
are covered; Ubuntu-only jobs remain automatic. The tag-triggered release
workflow is intentionally unchanged because it is an explicit release action,
not per-change CI. A manual full run preserves the existing runner labels and
job contents.

### Review 3: Failure-Closed Operation

Made the manual input required but defaulted it to `false`. Non-dispatch events
cannot opt in, and a manual run that omits the flag remains Ubuntu-only. Contract
tests require the opt-in expression on all and only the four affected matrices,
so a future static paid-runner matrix fails locally. Documentation tells
operators to dispatch once against the final ref rather than on intermediate
commits.
