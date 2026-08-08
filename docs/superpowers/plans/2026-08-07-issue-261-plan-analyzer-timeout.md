# Issue 261 Plan Analyzer Timeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` or `superpowers:executing-plans` to
> implement this plan task by task.

**Goal:** Enforce the configured analyzer timeout during plan creation and
verification rediscovery with deterministic cancellation tests and consistent
diagnostics.

**Architecture:** Run complete discovery as an owned blocking task with a
cooperative cancellation token. Race it against one absolute analyzer
deadline, detach it on timeout, and map the stable timeout identity through
both plan entry points.

## Global constraints

- The normalized `analyzer_timeout` remains schema- and fingerprint-relevant.
- One timeout covers complete discovery; it is not restarted per target.
- Timeout returns without awaiting the blocking thread, but the detached task
  owns and eventually releases every resource.
- Plan create and verify rediscovery use the same timeout diagnostic and exit
  code 2.
- Candidate ordering, diagnostics, truncation, and successful manifests remain
  byte-semantically unchanged.
- Behavior commits must not use `[skip ci]`; pure documentation commits must.

### Task 1: Build cancellable, deadline-bound discovery

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`
- Modify: analyzer unit/integration tests as needed

1. Add deterministic tests that pause after the blocking task starts, expire a
   short configured timeout, and assert the stable `analyzer.timeout` identity.
2. Assert the caller returns before release, then release the task and prove
   its owned resource sentinel is dropped.
3. Assert a subsequent discovery succeeds and ordinary discovery preserves
   candidate order, diagnostics, and truncation.
4. Implement an owned discovery request, absolute deadline, cooperative
   cancellation checks, and join-error mapping.
5. Run analyzer tests, fmt, Clippy, and diff check.
6. Commit as `fix: enforce discovery analyzer timeout`.

### Task 2: Apply one diagnostic to plan and verify

**Files:**

- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs` or plan unit tests

1. Add deterministic paused plan-create and verify-rediscovery tests. Assert
   exit/error identity, empty stdout, and that verify never launches its test
   command.
2. Add successful controls proving the configured timeout remains recorded and
   accepted during verification.
3. Pass `config.limits.analyzer_timeout` into both discovery calls and map only
   timeout to the common `plan.discovery: analyzer.timeout` diagnostic.
4. Run exact plan tests, the full plan integration suite, fmt, Clippy, and diff
   check.
5. Commit as `fix: enforce plan analyzer timeout`.

### Task 3: Document and verify the contract

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`

1. Document plan/verify discovery scope and detached cooperative cancellation.
2. Re-run timeout acceptance tests and the full repository gates:
   `cargo fmt`, workspace Clippy with warnings denied, `cargo test --workspace`,
   Python unittest discovery, and `git diff --check origin/main...HEAD`.
3. Commit pure documentation as
   `docs: document analyzer discovery timeout [skip ci]`.
4. Add a diff-empty CI trigger commit before the first push because the branch
   ends in `[skip ci]` documentation.

The PR closes #261. Merge only after every repository CI job passes; the hard
cgroup job may remain at its configured skip state.
