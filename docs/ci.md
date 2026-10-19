# CI selection and workflow checks

This page describes CI for developing and releasing hoimin itself. The product's
primary use is [local, resource-bounded test improvement by an AI agent](product-principles.md).
Running hoimin against a user's project in CI is optional; a universal mutation-score
gate is not the default product goal. This distinction does not relax the checks below.

CI always runs change selection and `CI result`.
For pull requests, it computes the diff from the merge base of the base and head commits; for pushes and merge queues, it uses the event's base commit.
It also classifies deleted paths and the old paths of renamed files. Unknown paths, failed diff retrieval, and manual runs select all checks.
All runners explicitly use Python 3.14, including change selection and result aggregation.

The dependency audit workflow checks licenses and sources in both Rust workspaces in addition to vulnerability scanning.
See the [dependency policy](dependency-policy.md) for its scope and build-script diff reviews.

| Change | Selected checks |
| --- | --- |
| `docs/**/*.md` (except `docs/json-schema/` and `docs/cli-reference.md`), `CONTRIBUTING.md` | Python quality and workflow checks |
| `docs/cli-reference.md` | Rust validation, CLI reference synchronization, quality and workflow checks |
| `crates/`, `vendor/` | Also run Rust, formal, and distribution checks |
| `formal/` | Also run Rust and formal checks |
| `tests/**/*.py` | Also run Python, Rust, and formal checks |
| `tools/release.py`, `tools/sbom.py`, `tools/capture_sbom.py` | Also run Python and distribution checks |
| `README.md`, `LICENSE` | Also run distribution checks (these are wheel inputs) |
| Schemas, fixtures, wheel smoke checks, workflows, lockfiles, and other paths | All checks |

Python changes also run wheel smoke checks.
The restrictions on nightly shuffle and fuzzing remain in place.
Live cgroup validation is selected only for pushes to main with a delegated runner explicitly enabled.
`CI result` checks that change selection succeeded, that its outputs are valid, and that results are present for the fixed set of jobs.
A required job with a `failure`, `cancelled`, or `skipped` result, or a missing result, fails the aggregate check.
Only jobs that were not selected may be skipped.
Only superseded PR CI and preview runs are cancelled.
Preview intentionally skips tag reservation, so downstream build, validation, and publication jobs explicitly use status functions to prevent an ancestor's skipped result from propagating through the implicit `success()` condition.
Each job still requires its direct dependencies to succeed; failures and cancellations are not accepted.

## Configuring required checks

The GitHub API inspection on 2026-10-10 found that the `HayaoSuzuki/hoimin` ruleset prohibited deletion and force pushes but did not configure required checks for main.
When enabling protection, require the aggregate `CI result` rather than conditional jobs.
The former organization's Pulumi configuration is outside this scope following the repository transfer.
The CI selection change did not modify external rulesets.

## Running workflow checks locally

```console
uv sync --frozen --group workflow --no-install-project
uv run --frozen --no-sync python -m tools.workflow_lint
uv run --frozen --no-sync pytest tests/test_workflow_lint.py tests/test_ci_selection.py tests/test_ci_selection_workflow.py tests/test_ci_metrics.py
```

The checker enumerates every `.yml` and `.yaml` file directly under `.github/workflows/`.
actionlint 1.7.12, ShellCheck 0.11.0, and zizmor 1.30.1 are pinned in the dependency group and lockfile and updated together by Renovate.
zizmor runs offline in auditor mode at every severity, including informational findings; ShellCheck rules are not excluded wholesale.
These checks need no secrets, write permissions, or SARIF uploads, and fork PRs run the same checks.

actionlint 1.7.12 writes stdin before starting its child process, which can hang on Windows when a long script exceeds the pipe capacity.
Its internal ShellCheck invocation is disabled; the same wrapper CLI runs ShellCheck separately for each bash/sh step.
All operating systems use this path to avoid platform differences.
actionlint and zizmor check Actions expression syntax and trust boundaries; expressions are replaced with placeholders before scripts are passed to ShellCheck.

The SBOM tools' PATH is set only in the steps that use them, removing the need to write to `GITHUB_PATH` or suppress a finding for that write.
On Windows, Git Bash uses `cygpath` to convert the runner's native paths.

Suppressions are limited to an inline `zizmor: ignore[...]` on the relevant line with a comment explaining the reason.

| Rule | Location and reason |
| --- | --- |
| `dangerous-triggers` | The release closed/merged event; jobs with write permissions check out only the merged base commit |
| `artipacked` | Tag reservation; pushing the tag requires credentials |
| `self-hosted-runner` | Live delegated-cgroup validation; main pushes with explicit opt-in only |

zizmor warns that it does not support evaluating `GITHUB_ENV` writes inside Python shell steps.
This tool limitation is not counted as successful detection.
Isolated fixtures check that the actual tools reject invalid expressions, ShellCheck violations, template injection, and excessive permissions.

## Measuring execution time

```console
uv run --frozen --no-sync python -m tools.ci_metrics --run RUN_ID --output docs/performance/ci/after-ci.json
```

The tool uses authenticated `gh` to fetch the run and all jobs from its latest attempt.
It records elapsed seconds, the number of executed jobs, total job time, and the wait from `created_at` to `started_at`.
Delays from the start of the run, including waits for dependencies, are recorded separately from queue time.
Missing timestamps are not replaced with zero, and skipped jobs are not counted as executed.

Before CI selection was introduced, CI run `38023706140` took 694 seconds, executed 10 jobs, and totaled 2803 job-seconds and 28 queue-seconds.
The corresponding preview run `38023706209` took 624 seconds, executed 5 jobs, and totaled 1387 job-seconds and 19 queue-seconds.
The retrieved data is stored in the [measurement records](performance/ci/).
This is one SBOM implementation change, not an average for documentation changes or a measured reduction rate.
After CI selection was introduced, a comparable full-validation PR had CI run `38031404421` at 723 seconds, 13 jobs, 3080 job-seconds, and 37 queue-seconds; preview run `38031404452` took 690 seconds, 6 jobs, 1470 job-seconds, and 20 queue-seconds.
Both PRs changed workflows and distribution inputs, selecting all checks.
The job count increased with the new lightweight jobs. A comparison of single runs with different changes and cache states does not establish that CI selection caused a speedup or slowdown.
Documentation-only, Rust-only, and formal-only changes need separate measurements under the same conditions. The documentation-only reduction rate has not been measured.
Post-change hosted-runner measurements must not be inferred from local validation.

The design and five self-reviews at each stage are documented in the [verification record](reviews/2026-10-10-ci-selection.md).

## Rust test runner evaluation

On 2026-10-11, cargo-nextest 0.9.148 was evaluated in the existing Linux Rust job with two concurrent workers.
The test lists matched, but neither workspace execution nor repeated timeout tests showed an improvement in execution time, so the existing cargo test runner was retained.
See the [evaluation results](performance/nextest/2026-10-11-linux.md) for measurement conditions, resource usage, ignored tests versus runtime skips, and reproduction steps.
Nightly shuffle, contracts, and OS-specific validation also remain in place.


## Required production arithmetic proof

The `creusot` job in `ci.yml` runs on Linux x86_64 and participates in `CI result`.
It builds or loads the pinned tool image, checks installed tools and effective
project configuration, then invokes `tools/verify_creusot.py`. Each of the three
release targets gets fresh Coma generation and no-cache strategy replay.
Missing contracts/tools, empty or stale output, unproved final goals, timeouts
and unintended job skips fail acceptance. The workflow measures setup within
its 30-minute job budget; solver processes each have a 30-second deadline and
2-GiB address-space limit, with at most two solvers and sequential targets.
Tool-image caching does not cache successful proof answers. See the
[proof guide](../formal/creusot/README.md) for maintenance and recorded cold/warm
costs. This workflow change does not modify remote branch-protection settings.
