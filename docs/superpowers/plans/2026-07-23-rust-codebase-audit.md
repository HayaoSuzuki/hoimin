# Rust Codebase Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a comprehensive, evidence-backed audit of the Rust codebase, create one GitHub issue per actionable semantic unit, and define the dependency order for issue-specific remediation worktrees and pull requests.

**Architecture:** The audit branch stores documentation and evidence only; it never contains product-code fixes. Six semantic audit areas feed a common coverage matrix and findings ledger, then a consolidation pass deduplicates root causes, creates GitHub issues, and publishes a final limitations-aware audit report.

**Tech Stack:** Rust 1.85+, Cargo, Clippy, rustfmt, Cargo contracts, Python 3.14, uv, maturin, ripgrep, Git, GitHub CLI, Markdown

## Global Constraints

- Perform all audit work on branch `audit/rust-codebase-2026-07` in `.worktrees/rust-codebase-audit`.
- The audit branch contains audit documentation and findings, not product-code fixes.
- Every actionable semantic unit becomes a GitHub issue; each issue is later implemented in its own branch, worktree, design, implementation plan, and pull request.
- Include confirmed defects, high-risk designs with a concrete failure path, and maintainability problems with a clear boundary and regression strategy.
- Exclude naming preferences, speculative rewrites, and claims without a specific code path or design boundary.
- Record platform checks that cannot run locally as explicit coverage limitations.
- Do not treat a static match, tool warning, or surviving mutant as a defect without reviewing its semantics.
- Preserve the existing public behavior while auditing; suspected fixes begin in later issue-specific worktrees with regression or characterization tests.
- Do not modify or stage files from the main checkout, including `.idea/` and `tests/fixtures/projects/basic/uv.lock`.

---

## File Structure

- Create `docs/audits/2026-07-rust-codebase/README.md`: audit identity, commands, evidence conventions, and current status.
- Create `docs/audits/2026-07-rust-codebase/coverage.md`: module-by-module coverage matrix and limitations.
- Create `docs/audits/2026-07-rust-codebase/findings.md`: candidate and accepted findings ledger.
- Create `docs/audits/2026-07-rust-codebase/issues.md`: GitHub issue mapping, dependencies, and recommended execution order.
- Create `docs/audits/2026-07-rust-codebase/report.md`: final audit summary, risk themes, clean areas, and limitations.
- Create `.audit/rust-codebase/`: ignored local command outputs used as evidence while executing the plan; never commit this directory.

### Task 1: Establish the audit artifact contracts

**Files:**
- Create: `docs/audits/2026-07-rust-codebase/README.md`
- Create: `docs/audits/2026-07-rust-codebase/coverage.md`
- Create: `docs/audits/2026-07-rust-codebase/findings.md`
- Create: `docs/audits/2026-07-rust-codebase/issues.md`
- Create: `docs/audits/2026-07-rust-codebase/report.md`

**Interfaces:**
- Consumes: `docs/superpowers/specs/2026-07-23-rust-codebase-audit-design.md`.
- Produces: stable table columns and finding identifiers consumed by Tasks 2–10.

- [ ] **Step 1: Create the audit README**

Create `docs/audits/2026-07-rust-codebase/README.md` with these sections and exact conventions:

```markdown
# Rust Codebase Audit — July 2026

Branch: `audit/rust-codebase-2026-07`
Base commit: `4a93b2720ee4be3ef9c0664b4c8b6116776eabc9`

## Status

`in_progress`

## Evidence conventions

- Command evidence records the command, host platform, exit code, and output path.
- Code evidence names exact files and line numbers at the base commit.
- `confirmed bug` requires a reproduction or a violated executable contract.
- `high-risk design` requires a concrete failure path and an unenforced invariant.
- `maintainability` requires a proposed boundary and characterization strategy.
- Platform behavior not executed locally is marked `limited`, never `pass`.

## Finding identifiers

Use `RUST-AUDIT-NNN`, assigned monotonically when a lead first enters the findings ledger.
Rejected leads keep their identifiers so later rows and issue links never shift.

## Artifacts

- [Coverage matrix](coverage.md)
- [Findings ledger](findings.md)
- [Issue map](issues.md)
- [Final report](report.md)
```

- [ ] **Step 2: Create the coverage matrix**

Create `coverage.md` with the columns below and one initial `pending` row for every production
Rust file returned by `rg --files crates -g '*.rs' | rg '/src/'`:

```markdown
# Coverage Matrix

| Area | Module | Static scan | Invariant trace | Dynamic evidence | Platform | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
```

Assign each module to exactly one design area: `core`, `orchestration`, `isolation`,
`persistence`, `analysis-output`, or `delivery`.

- [ ] **Step 3: Create the findings ledger**

Create `findings.md` with:

```markdown
# Findings Ledger

## Status vocabulary

- `lead`: requires semantic review
- `accepted`: actionable root cause
- `rejected`: not actionable, with rationale
- `issue_created`: accepted and linked to GitHub

| ID | Area | Classification | Severity | Status | Locations | Evidence | Root cause / boundary | Disposition |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
```

The classification column accepts only `confirmed bug`, `high-risk design`, or
`maintainability`. Severity accepts only `P0`, `P1`, `P2`, or `P3`.

- [ ] **Step 4: Create the issue map and report skeleton**

Create `issues.md`:

```markdown
# Issue Map

| Finding IDs | GitHub issue | Priority | Depends on | Worktree branch | Status |
| --- | --- | --- | --- | --- | --- |

## Recommended execution order

No issues have been created yet.
```

Create `report.md`:

```markdown
# Final Rust Codebase Audit Report

## Executive summary

Audit in progress.

## Quality-gate baseline

Audit in progress.

## Findings by severity

Audit in progress.

## Cross-cutting risk themes

Audit in progress.

## Areas with no actionable findings

Audit in progress.

## Coverage limitations

Audit in progress.

## Remediation order

Audit in progress.
```

- [ ] **Step 5: Verify artifact completeness**

Run:

```bash
test "$(rg --files crates -g '*.rs' | rg '/src/' | wc -l | tr -d ' ')" -eq \
  "$(rg -c '^\\| (core|orchestration|isolation|persistence|analysis-output|delivery) \\|' \
    docs/audits/2026-07-rust-codebase/coverage.md)"
rg -n 'TBD|TODO|FIXME' docs/audits/2026-07-rust-codebase
git diff --check
```

Expected: the row-count comparison succeeds, the placeholder scan prints nothing, and
`git diff --check` succeeds.

- [ ] **Step 6: Commit the artifact contracts**

```bash
git add docs/audits/2026-07-rust-codebase
git commit -m "docs: establish Rust audit evidence contracts"
```

### Task 2: Record the executable quality-gate baseline

**Files:**
- Modify: `docs/audits/2026-07-rust-codebase/README.md`
- Modify: `docs/audits/2026-07-rust-codebase/coverage.md`
- Modify: `docs/audits/2026-07-rust-codebase/report.md`
- Local only: `.audit/rust-codebase/quality-gates/`

**Interfaces:**
- Consumes: evidence conventions from Task 1.
- Produces: dated command evidence and baseline limitations used by every subsystem review.

- [ ] **Step 1: Create an ignored evidence directory**

Run:

```bash
mkdir -p .audit/rust-codebase/quality-gates
git check-ignore .audit/rust-codebase/quality-gates
```

If `git check-ignore` fails, use `apply_patch` to create this local file:

```diff
*** Begin Patch
*** Add File: .audit/.gitignore
+*
*** End Patch
```

Then rerun `git check-ignore .audit/rust-codebase/quality-gates`.

Expected: the evidence directory is ignored. Keep `.audit/.gitignore` untracked and do not
commit it.

- [ ] **Step 2: Run formatting and lint gates**

Run and save complete output:

```bash
cargo fmt --all -- --check \
  > .audit/rust-codebase/quality-gates/fmt.log 2>&1
cargo clippy --workspace --all-targets --all-features -- -D warnings \
  > .audit/rust-codebase/quality-gates/clippy.log 2>&1
```

Expected: both exit 0. A failure becomes a lead with the exact diagnostic, not an immediate
code change.

- [ ] **Step 3: Run Rust behavior and contract gates**

```bash
cargo test --workspace \
  > .audit/rust-codebase/quality-gates/workspace-tests.log 2>&1
cargo test -p hoimin-core --features contracts \
  > .audit/rust-codebase/quality-gates/core-contracts.log 2>&1
cargo test -p hoimin-cli --features contracts \
  > .audit/rust-codebase/quality-gates/cli-contracts.log 2>&1
```

Expected: all exit 0 after `uv sync --frozen` has created `.venv/bin/python`.

- [ ] **Step 4: Run Python, release, and wheel gates**

```bash
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v \
  > .audit/rust-codebase/quality-gates/python-tests.log 2>&1
cargo build --workspace --release \
  > .audit/rust-codebase/quality-gates/release-build.log 2>&1
uv run --frozen maturin build --release \
  > .audit/rust-codebase/quality-gates/wheel-build.log 2>&1
wheel_path=$(find target/wheels -type f -name '*.whl' -print | sort | tail -1)
HOIMIN_WHEEL="$wheel_path" uv run --frozen python tests/wheel_smoke.py \
  > .audit/rust-codebase/quality-gates/wheel-smoke.log 2>&1
```

Expected: all exit 0 and `wheel_path` names the wheel built by the preceding command.

- [ ] **Step 5: Verify core dependency purity and dependency duplication**

```bash
cargo tree -p hoimin-core --edges normal --prefix none \
  > .audit/rust-codebase/quality-gates/core-tree.log
! rg '(^| )(tokio|rusqlite|tempfile|windows-sys|libc|hoimin-cli)( |$)' \
  .audit/rust-codebase/quality-gates/core-tree.log
cargo tree --workspace --duplicates \
  > .audit/rust-codebase/quality-gates/duplicate-dependencies.log
```

Expected: the purity assertion succeeds. Duplicate dependencies are reviewed as maintenance
leads only when they create a concrete compatibility, binary-size, or security burden.

- [ ] **Step 6: Record baseline results**

In `README.md`, add a table with each exact command, current date, host OS/architecture, exit
status, and local log path. In `report.md`, replace the quality-gate placeholder with a concise
summary. Mark Windows and delegated Linux cgroup execution `limited` in `coverage.md` because
they are not executed on the current macOS host.

- [ ] **Step 7: Commit baseline evidence summaries**

```bash
git add docs/audits/2026-07-rust-codebase
git commit -m "docs: record Rust audit quality baseline"
```

### Task 3: Perform the cross-codebase mechanical risk scan

**Files:**
- Modify: `docs/audits/2026-07-rust-codebase/coverage.md`
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`
- Local only: `.audit/rust-codebase/scans/`

**Interfaces:**
- Consumes: all production Rust modules in the Task 1 matrix.
- Produces: classified leads with exact locations; no match is accepted without semantic review.

- [ ] **Step 1: Capture panic and unsafe-surface matches**

```bash
mkdir -p .audit/rust-codebase/scans
rg -n '\\b(unsafe|unwrap|expect|panic!|unreachable!|todo!|unimplemented!)\\b' \
  crates -g '*.rs' \
  > .audit/rust-codebase/scans/panic-unsafe.log
rg -n '\\[[^]]+\\]| as (u|i)(8|16|32|64|128|size)' crates -g '*.rs' \
  > .audit/rust-codebase/scans/index-cast.log
```

Expected: matches are leads. Test-only matches are recorded separately from production matches.

- [ ] **Step 2: Capture error-loss and cleanup matches**

```bash
rg -n 'let _ =|\\.ok\\(\\)|unwrap_or|unwrap_or_default|impl Drop|remove_(file|dir)|kill\\(|wait\\(' \
  crates -g '*.rs' \
  > .audit/rust-codebase/scans/error-cleanup.log
rg -n 'spawn|JoinSet|channel|Mutex|RwLock|Semaphore|select!|timeout|cancel' \
  crates/hoimin-cli/src -g '*.rs' \
  > .audit/rust-codebase/scans/concurrency.log
```

- [ ] **Step 3: Capture filesystem, SQL, and platform matches**

```bash
rg -n 'canonicalize|symlink|permissions|temp|rename|persist|transaction|execute_batch|PRAGMA' \
  crates/hoimin-cli/src -g '*.rs' \
  > .audit/rust-codebase/scans/io-persistence.log
rg -n '#\\[cfg|cfg!|target_os|target_family' crates -g '*.rs' \
  > .audit/rust-codebase/scans/platform.log
```

- [ ] **Step 4: Semantically triage every production match**

For each production match, inspect its enclosing function, callers, error type, and covering
tests. Add a `lead` row only when a plausible invariant or failure path remains. Otherwise add
a short rejected-lead note beneath the relevant module in `coverage.md`. Assign identifiers in
ascending order without reuse.

- [ ] **Step 5: Mark static-scan coverage**

Set `Static scan` to `complete` for every production module whose matches have been reviewed.
Run:

```bash
! rg '^\\| .* \\| .* \\| pending \\|' docs/audits/2026-07-rust-codebase/coverage.md
git diff --check
```

Expected: no production module retains `pending` static-scan status.

- [ ] **Step 6: Commit the mechanical scan**

```bash
git add docs/audits/2026-07-rust-codebase/coverage.md \
  docs/audits/2026-07-rust-codebase/findings.md
git commit -m "docs: record Rust mechanical risk scan"
```

### Task 4: Audit core state and policy

**Files:**
- Inspect: `crates/hoimin-core/src/*.rs`
- Inspect: `crates/hoimin-core/tests/*.rs`
- Modify: `docs/audits/2026-07-rust-codebase/coverage.md`
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`
- Local only: `.audit/rust-codebase/core/`

**Interfaces:**
- Consumes: Task 3 leads for `hoimin-core`.
- Produces: accepted or rejected findings for state transitions, policy normalization, budgets, reports, and resume semantics.

- [ ] **Step 1: Map core invariants**

Record a plain-text call/effect map in `.audit/rust-codebase/core/invariants.md` covering:

```text
RawRunConfig -> RunConfig -> RunState
RunState::next_effects -> RunEffect -> EffectCompletion -> RunState::accept_completion
MutationCandidate -> OutputEvent -> RunReport -> exit policy
Budget reservation -> grant -> release
RunConfig fingerprint -> compatible resume lookup
```

For each arrow, name the validation function and error type that rejects invalid state.

- [ ] **Step 2: Trace state-machine terminal paths**

Review deadline, cancellation, analyzer failure, baseline failure, mutant failure, cleanup
failure, session failure, and output failure. Verify each active candidate receives exactly
one terminal status and each cleanup obligation is emitted at most once. Cross-reference
`crates/hoimin-core/tests/machine.rs`.

- [ ] **Step 3: Trace numeric and normalization invariants**

Review all limit conversions, duration calculations, candidate sequence arithmetic, span
arithmetic, target unions/intersections, and fingerprint canonicalization. Use existing
property tests as evidence and add a lead where boundary coverage or checked arithmetic is
missing.

- [ ] **Step 4: Review report and resume semantics**

Verify report completeness, score denominator, exit precedence, event ordering, compatibility
fields, and stored-result replacement rules remain aligned across `report.rs`, `resume.rs`,
and tests.

- [ ] **Step 5: Run focused evidence**

```bash
cargo test -p hoimin-core --features contracts \
  > .audit/rust-codebase/core/contracts.log 2>&1
cargo test -p hoimin-core --test machine --test target_policy \
  --test report_policy --test resume_policy --test budget \
  > .audit/rust-codebase/core/policies.log 2>&1
```

Expected: exit 0. Tool success does not automatically reject structural leads.

- [ ] **Step 6: Update and commit core findings**

Set all core rows' `Invariant trace` and `Dynamic evidence` statuses. Every Task 3 core lead
must become `accepted` or `rejected` with rationale.

```bash
git add docs/audits/2026-07-rust-codebase/coverage.md \
  docs/audits/2026-07-rust-codebase/findings.md
git commit -m "docs: audit Rust core state and policy"
```

### Task 5: Audit CLI orchestration and process lifecycle

**Files:**
- Inspect: `crates/hoimin-cli/src/shell.rs`
- Inspect: `crates/hoimin-cli/src/process/*.rs`
- Inspect: `crates/hoimin-cli/src/metrics.rs`
- Inspect: `crates/hoimin-cli/tests/process_handler.rs`
- Inspect: `crates/hoimin-cli/tests/run_e2e.rs`
- Modify: `docs/audits/2026-07-rust-codebase/coverage.md`
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`

**Interfaces:**
- Consumes: core effect/completion contracts and Task 3 concurrency leads.
- Produces: lifecycle findings and a concrete decomposition assessment for `shell.rs`.

- [ ] **Step 1: Trace process lifecycle linearization**

Follow prepare, queue, spawn gate, resource attach, output collection, cancellation, kill,
wait, classification, and cleanup. Verify cancellation cannot race a spawn into an untracked
child and every spawned descendant reaches a wait/reap path.

- [ ] **Step 2: Trace orchestration cleanup precedence**

For each `RunEffect` handler in `shell.rs`, record acquisition and release obligations. Verify
the first operational error is retained while cleanup failures remain observable, metrics
stages terminate, and output failure cannot allow further state-machine progress.

- [ ] **Step 3: Audit boundedness**

Review `JoinSet`, completion queues, retained output, worker maps, candidate spools, metrics
collections, and channel capacity. Relate every collection to a configured bound or lifecycle
drain.

- [ ] **Step 4: Assess medium-sized boundaries**

Document whether `shell.rs` can be split along these exact responsibilities without changing
behavior:

```text
effect dispatch
process preparation and completion
workspace/session adapters
run-loop termination and cleanup
```

Accept a maintainability finding only if characterization tests and a staged dependency
direction can be named.

- [ ] **Step 5: Run process evidence**

```bash
cargo test -p hoimin-cli --test process_handler --test run_e2e \
  > .audit/rust-codebase/orchestration-tests.log 2>&1
```

Expected: exit 0 on the current host; delegated cgroup behavior remains a separate limitation.

- [ ] **Step 6: Update and commit orchestration findings**

```bash
git add docs/audits/2026-07-rust-codebase/coverage.md \
  docs/audits/2026-07-rust-codebase/findings.md
git commit -m "docs: audit CLI process orchestration"
```

### Task 6: Audit workspace isolation and resource backends

**Files:**
- Inspect: `crates/hoimin-cli/src/workspace/*.rs`
- Inspect: `crates/hoimin-cli/src/resource/*.rs`
- Inspect: `crates/hoimin-cli/tests/workspace_handler.rs`
- Inspect: `crates/hoimin-cli/tests/workspace_recovery.rs`
- Inspect: `crates/hoimin-cli/tests/process_handler.rs`
- Modify: `docs/audits/2026-07-rust-codebase/coverage.md`
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`

**Interfaces:**
- Consumes: process lifecycle obligations from Task 5.
- Produces: isolation, rollback, accounting, and platform-parity findings.

- [ ] **Step 1: Trace workspace capability lifecycle**

Trace manifest discovery, copy reservation, worker materialization, mutation, reset, original
integrity checks, close, and drop cleanup. Verify normalized relative paths and capability
identity are checked before every filesystem operation.

- [ ] **Step 2: Trace rollback under partial failure**

Review partial copy, permission change, reset failure, cleanup failure, read-only trees,
symlinks, and original-source changes. Verify each failure releases the correct core budget
exactly once and cannot reuse a poisoned worker.

- [ ] **Step 3: Compare resource backend contracts**

Build a comparison table in the coverage notes for Linux, Windows, and portable backends:

```text
capability probe
attach timing
memory accounting
process accounting
descendant termination
classification
cleanup retry
reported mode
```

Flag semantic differences only when the public report or safety guarantee differs.

- [ ] **Step 4: Review platform limitations**

Mark Windows Job Object execution and delegated Linux cgroup v2 hard enforcement `limited`.
Review their code and tests statically, but do not label them dynamically verified.

- [ ] **Step 5: Run local isolation evidence**

```bash
cargo test -p hoimin-cli --test workspace_handler --test workspace_recovery \
  > .audit/rust-codebase/workspace-tests.log 2>&1
cargo test -p hoimin-cli --test process_handler portable:: \
  > .audit/rust-codebase/portable-resource-tests.log 2>&1
```

Expected: exit 0.

- [ ] **Step 6: Update and commit isolation findings**

```bash
git add docs/audits/2026-07-rust-codebase/coverage.md \
  docs/audits/2026-07-rust-codebase/findings.md
git commit -m "docs: audit workspace and resource isolation"
```

### Task 7: Audit persistence, plans, fingerprints, and target inputs

**Files:**
- Inspect: `crates/hoimin-cli/src/session/*.rs`
- Inspect: `crates/hoimin-cli/src/plan.rs`
- Inspect: `crates/hoimin-cli/src/fingerprint_inputs.rs`
- Inspect: `crates/hoimin-cli/src/target/*.rs`
- Inspect: corresponding files in `crates/hoimin-cli/tests/`
- Modify: `docs/audits/2026-07-rust-codebase/coverage.md`
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`

**Interfaces:**
- Consumes: normalized core config and target invariants from Task 4.
- Produces: transaction, compatibility, manifest, and hostile-input findings.

- [ ] **Step 1: Trace session transactions and migration**

Verify schema upgrades are atomic and idempotent, busy behavior is bounded, failed result
replacement restores prior state, completed runs reject mutation, and lookup queries cannot
combine candidates/results from incompatible runs.

- [ ] **Step 2: Trace plan/verify reconstruction**

Follow serialized config, target records, fingerprints, candidate descriptors, requested ID
normalization, source revalidation, baseline execution, and selected candidate replay. Verify
malformed or incoherent manifests fail before project execution.

- [ ] **Step 3: Trace filesystem and Git inputs**

Review exact fingerprint files, glob inputs, symlinks, ignored files, non-UTF-8 paths, staged
and unstaged changes, rename handling, hostile revisions, and root containment.

- [ ] **Step 4: Run persistence and input evidence**

```bash
cargo test -p hoimin-cli --test session_handler --test plan \
  --test fingerprint_inputs --test target_handler \
  > .audit/rust-codebase/persistence-input-tests.log 2>&1
```

Expected: exit 0.

- [ ] **Step 5: Update and commit persistence findings**

```bash
git add docs/audits/2026-07-rust-codebase/coverage.md \
  docs/audits/2026-07-rust-codebase/findings.md
git commit -m "docs: audit persistence and external inputs"
```

### Task 8: Audit analysis, report output, progress, and delivery

**Files:**
- Inspect: `crates/hoimin-cli/src/analyzer/*.rs`
- Inspect: `crates/hoimin-cli/src/report/*.rs`
- Inspect: `crates/hoimin-cli/src/progress/*.rs`
- Inspect: `crates/hoimin-cli/src/cli.rs`
- Inspect: `crates/hoimin-cli/src/lib.rs`
- Inspect: `crates/hoimin-cli/src/main.rs`
- Inspect: `.github/workflows/ci.yml`
- Inspect: `pyproject.toml`
- Inspect: `tests/*.py`
- Modify: `docs/audits/2026-07-rust-codebase/coverage.md`
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`

**Interfaces:**
- Consumes: candidate/event/report invariants from Task 4 and baseline delivery evidence from Task 2.
- Produces: analyzer, schema, progress-comparison, packaging, and CI findings.

- [ ] **Step 1: Trace analyzer input to candidate spool**

Review parser errors, byte spans, Unicode line/column metadata, symbol scopes, type-annotation
resolution, operator filtering, candidate limits, spool framing, replay offsets, and memory
boundedness.

- [ ] **Step 2: Trace report serialization**

Verify JSON and JSONL schema versions, exact event kinds, streaming behavior, retained output,
diagnostic channels, final summary completeness, and partial-write poisoning.

- [ ] **Step 3: Trace progress comparison**

Review report validation, exact mutant keys, ambiguity, incomplete inputs, added/removed
candidates, stall counting, saturation, and output schema. Ensure no code path infers whole
coverage from arbitrary candidate subsets.

- [ ] **Step 4: Review CLI and delivery contracts**

Compare Clap parsing, README examples, CI commands, supported Rust/Python versions, maturin
configuration, wheel smoke behavior, and platform matrices. Record redundant CI execution as
a lead only if it creates a concrete cost or coverage problem.

- [ ] **Step 5: Run focused evidence**

```bash
cargo test -p hoimin-cli --test analyzer_handler --test rust_analyzer \
  --test report_handler --test report_heap --test progress --test cli_config \
  > .audit/rust-codebase/analysis-output-tests.log 2>&1
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v \
  > .audit/rust-codebase/delivery-tests.log 2>&1
```

Expected: exit 0.

- [ ] **Step 6: Update and commit analysis and delivery findings**

All production-module coverage rows must now have final static, invariant, and dynamic statuses.

```bash
git add docs/audits/2026-07-rust-codebase/coverage.md \
  docs/audits/2026-07-rust-codebase/findings.md
git commit -m "docs: audit analysis output and delivery"
```

### Task 9: Validate leads and consolidate semantic root causes

**Files:**
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`
- Modify: `docs/audits/2026-07-rust-codebase/issues.md`
- Local only: `.audit/rust-codebase/validation/`

**Interfaces:**
- Consumes: every lead and coverage note from Tasks 3–8.
- Produces: only accepted or rejected findings, deduplicated into issue-sized root causes.

- [ ] **Step 1: Validate confirmed-bug candidates**

For each candidate, use an existing test, a temporary test patch that is reverted after
execution, or a standalone minimal fixture under `.audit/rust-codebase/validation/`. Record
the exact command, observed result, and expected contract. Classify as `confirmed bug` only
when the reproduction violates the contract.

- [ ] **Step 2: Validate high-risk design candidates**

For each candidate, document:

```text
trigger
unenforced invariant
failure propagation
observable impact
existing mitigation
why mitigation is insufficient
```

Reject the lead if any transition in the failure path is prevented by a checked type,
validation, test-backed state transition, or guaranteed cleanup owner.

- [ ] **Step 3: Validate maintainability candidates**

For each candidate, document the current responsibilities, proposed module boundary,
dependency direction, characterization tests, and incremental steps. Reject changes that
require a flag day, alter unrelated public behavior, or cannot name a reviewable first PR.

- [ ] **Step 4: Deduplicate by semantic root cause**

Combine findings only when one fix and one regression/characterization strategy resolves all
listed symptoms. Preserve all source finding IDs in the combined `Finding IDs` field in
`issues.md`.

- [ ] **Step 5: Assign priority and dependencies**

Order accepted roots by P0/P1, then P2 correctness risk, then P2 design, then P3. Add a
dependency only when one issue changes an interface or invariant required by another; do not
serialize independent work for convenience.

- [ ] **Step 6: Verify ledger closure and commit**

Run:

```bash
! rg '\\| lead \\|' docs/audits/2026-07-rust-codebase/findings.md
git diff --check
```

Expected: no lead remains and the diff is clean.

```bash
git add docs/audits/2026-07-rust-codebase/findings.md \
  docs/audits/2026-07-rust-codebase/issues.md
git commit -m "docs: consolidate Rust audit root causes"
```

### Task 10: Create GitHub issues and publish the final audit report

**Files:**
- Modify: `docs/audits/2026-07-rust-codebase/findings.md`
- Modify: `docs/audits/2026-07-rust-codebase/issues.md`
- Modify: `docs/audits/2026-07-rust-codebase/report.md`
- Modify: `docs/audits/2026-07-rust-codebase/README.md`
- Local only: `.audit/rust-codebase/issues/`

**Interfaces:**
- Consumes: accepted, deduplicated root causes from Task 9.
- Produces: GitHub issues, issue links, remediation order, and a complete audit report.

- [ ] **Step 1: Render one body file per issue root**

For each row in `issues.md`, create `.audit/rust-codebase/issues/RUST-AUDIT-NNN.md` using these
sections populated only from accepted evidence:

```markdown
# bug: describe the semantic outcome imperatively

## Classification and priority

## Problem

## Evidence

## Impact

## Proposed design boundary

## Non-goals

## Acceptance criteria

## Required verification

## Dependencies
```

Titles use `bug:`, `refactor:`, or `test:` according to the accepted root cause, followed by
an imperative description of the semantic outcome. Do not put the audit identifier in the
title. Also create `.audit/rust-codebase/issues/creation-order.txt` with one body-file path per
line in dependency order.

- [ ] **Step 2: Review issue independence before external creation**

For every body, verify:

- acceptance criteria can be satisfied by one dedicated worktree and pull request;
- non-goals prevent adjacent findings from expanding scope;
- a bug includes reproduction evidence;
- a refactor includes preserved behavior and characterization tests;
- dependencies name only already mapped issue roots.

- [ ] **Step 3: Create issues in dependency order**

Run once per reviewed body:

```bash
set -e
while IFS= read -r body; do
  title=$(sed -n '1s/^# //p' "$body")
  gh issue create --repo tokyogas-tech/hoimin \
    --title "$title" \
    --body-file "$body"
done < .audit/rust-codebase/issues/creation-order.txt
```

Each created filename contains its exact accepted finding ID. Record each returned URL
immediately in `issues.md` and change its findings-ledger status to
`issue_created`. If a GitHub call fails, stop before creating dependent issues and preserve
the already returned URLs.

- [ ] **Step 4: Complete the remediation order**

Replace the default text under `Recommended execution order` with numbered waves:

1. correctness and cleanup prerequisites;
2. shared-boundary refactors;
3. independent subsystem refactors;
4. maintainability-only follow-ups.

For each issue, record its future branch as `fix/issue-N-*` or `refactor/issue-N-*`; do not
create those worktrees on the audit branch.

- [ ] **Step 5: Write the final report**

Replace every `Audit in progress.` section in `report.md` with evidence-backed content:

- counts by classification and severity;
- the highest-risk root causes and why;
- areas with no actionable finding;
- commands that passed;
- current-host and platform limitations;
- links to all issues and their dependency order.

Set the README status to `complete`.

- [ ] **Step 6: Verify audit completeness**

Run:

```bash
! rg 'in_progress|Audit in progress|\\| lead \\||\\| accepted \\|' \
  docs/audits/2026-07-rust-codebase
test "$(rg -c 'https://github.com/tokyogas-tech/hoimin/issues/[0-9]+' \
  docs/audits/2026-07-rust-codebase/issues.md)" -eq \
  "$(rg -c '\\| issue_created \\|' \
  docs/audits/2026-07-rust-codebase/findings.md)"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check
git status --short
```

Expected: no incomplete audit state remains, issue counts agree, all Rust gates pass, and only
the intended audit documents are modified.

- [ ] **Step 7: Commit the completed audit**

```bash
git add docs/audits/2026-07-rust-codebase
git commit -m "docs: publish comprehensive Rust audit"
```

- [ ] **Step 8: Request final review and open the audit PR**

Request a whole-branch review against
`docs/superpowers/specs/2026-07-23-rust-codebase-audit-design.md`. After all Critical and
Important review findings are resolved, push `audit/rust-codebase-2026-07` and open a pull
request containing only the audit design, plan, evidence summaries, findings, issue map, and
report.
