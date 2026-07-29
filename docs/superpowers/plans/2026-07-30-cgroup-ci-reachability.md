# Delegated cgroup CI Reachability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the delegated cgroup-v2 hard-limit job execute automatically on opted-in main pushes.

**Architecture:** Add the missing workflow trigger without weakening the job guard. Protect reachability and privileged-runner isolation through repository-local workflow contract tests.

**Tech Stack:** GitHub Actions YAML, Python 3.14 unittest, Markdown.

## Global Constraints

- Self-hosted delegated execution is restricted to pushes to `refs/heads/main`.
- `vars.HOIMIN_CGROUP_V2_DELEGATED == 'true'` remains mandatory.
- All existing self-hosted runner labels and the fail-closed `SKIP:` check remain mandatory.

---

### Task 1: Add failing workflow contracts

**Files:**
- Modify: `tests/test_ci_workflow.py`

**Interfaces:**
- Consumes: `.github/workflows/ci.yml`
- Produces: general event-condition reachability and delegated-job policy checks

- [ ] Extract top-level event names and job-level `if` event literals.
- [ ] Assert every referenced event is present in the workflow triggers.
- [ ] Assert the cgroup job preserves its ref, variable, runner labels, and `SKIP:` rejection.
- [ ] Run `uv run --frozen python -m unittest tests/test_ci_workflow.py -v` and confirm reachability fails.

### Task 2: Add the main push trigger

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: GitHub Actions `push.branches`
- Produces: reachable `github.event_name == 'push'` on `refs/heads/main`

- [ ] Add `push` with `branches: [main]`.
- [ ] Run the workflow contract and confirm it passes.

### Task 3: Document and verify operations

**Files:**
- Modify: `docs/development.md`

**Interfaces:**
- Consumes: GitHub CLI run inspection commands
- Produces: post-merge execution and runner-troubleshooting procedure

- [ ] Document run listing, job/log inspection, and the no-`SKIP:` requirement.
- [ ] Document queued/offline/label mismatch behavior and the 24-hour failure boundary.
- [ ] Run the full Python suite and repository quality checks.
- [ ] After merge, verify an actual opted-in main push executes the delegated job.
