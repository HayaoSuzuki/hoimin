# Automatic Linux and Manual Non-Linux CI Design

## Goal

Run the Linux validation suite for every pull request and push to `main`.
Run GitHub-hosted Windows and macOS validation only when an operator explicitly
starts it, and keep its results outside the automatic CI dependency and merge
decision paths.

This is an execution-frequency boundary, not a claim that Linux runners have no
cost.

## Current Behavior

`.github/workflows/ci.yml` statically expands four OS matrices. Every pull
request and push to `main` creates Windows and macOS jobs for `quality`, `rust`,
and `wheel-smoke`, plus a Windows `core-dependency-purity` job. The `rust`,
purity, and wheel jobs also depend on the cross-platform `quality` matrix.

The repository currently has no branch protection or ruleset for `main`, so no
existing Windows or macOS check is configured as a required merge condition.

## Decision

Split the workflows by execution policy rather than selecting operating systems
with a conditional matrix.

### Automatic workflow

`.github/workflows/ci.yml` keeps its `pull_request`, `main` push, and manual
triggers, but every hosted-runner matrix becomes the static list
`[ubuntu-latest]`. Existing Ubuntu-only jobs and the separately guarded
self-hosted Linux cgroup job remain unchanged. No Windows or macOS runner label
or selection condition remains in this workflow.

Keeping a one-element matrix preserves existing Linux check names such as
`Quality (ubuntu-latest)` and minimizes unrelated workflow changes.

### Manual workflow

`.github/workflows/non-linux-ci.yml` has exactly one trigger:
`workflow_dispatch`. It contains the former Windows and macOS variants of:

- quality (`windows-latest`, `macos-14`);
- Rust tests (`windows-latest`, `macos-14`);
- core dependency purity (`windows-latest`); and
- wheel smoke (`windows-latest`, `macos-14`).

The manual jobs have names prefixed with `Manual`, and have neither `needs` nor
job-level `if` expressions. Their outcomes do not control an automatic job or
another manual job. They are diagnostic evidence from a deliberate run, not an
automatic merge condition.

The workflow is started once against a selected final ref:

```console
gh workflow run non-linux-ci.yml --ref <REF>
```

GitHub requires a manually dispatched workflow file to exist on the default
branch. Therefore the command becomes available after this policy change is
merged.

## Release Boundary

`.github/workflows/release.yml` is separate and is triggered by an explicit
version tag to create release artifacts. This change does not alter release
validation or artifact production. The weekly Rust canary is Linux-only and is
also unchanged.

## Contract Tests

`tests/test_ci_workflow.py` enforces these boundaries:

- automatic CI contains no Windows or macOS hosted-runner labels;
- its four matrices contain only `ubuntu-latest`;
- the non-Linux workflow has only `workflow_dispatch`;
- the manual workflow contains all seven existing non-Linux job variants and
  no Linux hosted runner;
- manual jobs have no `needs` or `if` keys;
- manual job names are distinct from automatic check names; and
- development documentation gives the exact one-shot dispatch command.

## Self-Review Record

### Review 1: Runner-Creation Boundary

Rejected step-level guards because a runner is allocated before a step is
skipped. Rejected job guards that refer to `matrix.os` because GitHub evaluates
`jobs.<job_id>.if` before matrix expansion. Rejected a trigger-dependent dynamic
matrix because it would retain non-Linux selection logic in automatic CI.
Separate workflows make the execution boundary structural and inspectable.

### Review 2: Decision Independence

Removed non-Linux jobs from the automatic job graph instead of merely skipping
them. The manual jobs do not feed `needs`, outputs, status conditions, or check
names used by automatic jobs. A read-only repository-settings audit found no
branch protection or ruleset that must be migrated.

### Review 3: Coverage and Cost Control

Enumerated all seven current non-Linux variants before the split so manual
coverage is not silently lost. A single deliberate dispatch retains that
coverage without any per-commit invocation. Linux remains automatic despite
also consuming runner capacity. Release jobs remain tied to their existing
explicit tag event and are outside this CI split.
