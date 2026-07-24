# Rust Codebase Audit Design

## Purpose

The audit will examine the complete Rust codebase for confirmed defects, high-risk design
choices, and maintainability problems that make future defects more likely. It includes
medium-sized design improvements, but excludes preference-only rewrites and broad changes
without a concrete safety or maintainability benefit.

The audit itself is isolated on branch `audit/rust-codebase-2026-07` in
`.worktrees/rust-codebase-audit`. This branch contains audit documentation and findings, not
product-code fixes. Each actionable semantic unit becomes a GitHub issue. Every issue is then
implemented in its own branch, worktree, design, implementation plan, and pull request.

## Baseline

The repository contains approximately 33,000 lines of Rust across two crates:

- `hoimin-core`: pure configuration, target normalization, budgets, candidates, events,
  reports, resume compatibility, and the mutation state machine;
- `hoimin-cli`: CLI parsing, orchestration, process control, platform resource enforcement,
  workspaces, sessions, plans, fingerprint inputs, analysis, reporting, and progress.

CI already enforces formatting, strict Clippy lints, workspace tests, contract-enabled tests,
core dependency purity, cross-platform builds, wheel smoke tests, and selected Linux resource
tests. Local development also defines full mutation testing.

The isolated worktree initially lacked `.venv/bin/python`, causing all 17 `plan` integration
tests to fail before executing their behavior. After `uv sync --frozen`, `cargo build` and
`cargo test --workspace` passed. The missing environment is setup evidence, not a product
finding.

## Audit Areas

The codebase is divided into semantic audit areas:

1. **Core state and policy**
   - `machine`, `config`, `target`, `budget`, `candidate`, `event`, `report`, and `resume`;
   - effect/completion pairing, state transitions, ordering, limits, overflow, and terminal
     semantics.
2. **CLI orchestration and processes**
   - `shell`, `process`, cancellation, deadlines, child reaping, output collection, and
     metrics;
   - concurrency races, cleanup precedence, bounded queues, and error propagation.
3. **Isolation and platform resources**
   - workspace copy/reset/mutation and Linux, Windows, and portable resource backends;
   - path and symlink safety, rollback, process trees, memory/process accounting, and
     cross-platform semantic parity.
4. **Persistence and external inputs**
   - sessions, schemas, plan/verify, fingerprints, and target discovery;
   - transactions, migration, compatibility fingerprints, hostile paths, malformed manifests,
     and partial failure.
5. **Analysis and output**
   - Rust Python analysis, candidate spooling, report rendering, JSON/JSONL contracts, and
     progress;
   - parser assumptions, ordering, bounded memory, schema completeness, and ambiguous input.
6. **Delivery**
   - CLI configuration, Python packaging, wheel smoke tests, CI, and documented developer
     workflows;
   - test-environment reproducibility and platform coverage gaps.

## Audit Method

### 1. Establish quality-gate evidence

Run and record:

- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace`;
- `cargo test -p hoimin-core --features contracts`;
- `cargo test -p hoimin-cli --features contracts`;
- `uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v`;
- release build and wheel smoke checks;
- dependency-purity and dependency-policy checks available locally.

Platform-specific checks that cannot run on the current host are recorded as coverage limits,
not silently treated as passing.

### 2. Perform a mechanical risk scan

Search and classify uses of:

- `unsafe`, `unwrap`, `expect`, `panic`, unchecked indexing and numeric conversions;
- ignored errors, lossy error replacement, cleanup in `Drop`, and partial-write handling;
- locks held across await points, blocking work in async paths, unbounded collections/channels,
  spawned tasks, and cancellation races;
- filesystem traversal, symlinks, permissions, temporary files, and path normalization;
- SQL transactions, schema migrations, replacement semantics, and interrupted writes;
- platform `cfg` branches whose public behavior may diverge.

The scan produces leads only. A match is not a finding until its surrounding invariant and
failure path are reviewed.

### 3. Trace boundaries and invariants

For each audit area, follow data from external input through validated domain values, effects,
I/O handlers, completions, state transitions, persistence, and final reports. Record the
owner of each invariant and flag invariants that are duplicated, implicit, or split across
layers.

Priority traces include:

- start/cancel/spawn/reap linearization;
- deadline and cancellation terminal ordering;
- workspace reservation, materialization, reset, and cleanup;
- resource attach, accounting, kill, and backend cleanup;
- candidate spool ordering and limits;
- session transaction and resume replacement;
- plan reconstruction and verification;
- report completeness and exit-code selection.

### 4. Validate findings dynamically

For suspected defects, first add the smallest reproduction outside the audit branch or use a
temporary read-only experiment. Confirmed fixes must later begin with a failing regression
test in the issue-specific worktree.

Use characterization tests before design refactors. Use contract-enabled tests, property
tests, mutation testing, stress/concurrency tests, and platform-gated tests when they provide
specific evidence. Do not equate a surviving mutant or tool warning with a defect without
examining semantics.

### 5. Review design boundaries

Review large orchestration modules such as `shell.rs`, core `machine.rs`, workspace handling,
and resource backends for:

- mixed responsibilities and multiple reasons to change;
- duplicated state or cleanup policies;
- interfaces that expose invalid intermediate states;
- platform implementations that cannot be compared through one shared contract;
- functions whose size prevents local reasoning and reliable testing.

Refactoring findings must name a concrete boundary, preserved behavior, characterization
tests, and an incremental migration path.

## Finding Classification

Every finding uses one classification:

- **confirmed bug**: a reproducible panic, wrong result, leak, corruption, unsafe path, or
  violated documented contract;
- **high-risk design**: no current reproduction, but a concrete failure path exists because an
  invariant or cleanup obligation is structurally unenforced, duplicated, or platform-divergent;
- **maintainability**: responsibility mixing or module size materially reduces change safety,
  and a clear boundary and regression strategy can be described.

Severity is independent of classification:

- **P0/P1**: corruption, security-boundary failure, resource leak, incorrect result, or failed
  cleanup with immediate operational impact;
- **P2**: high-risk design, platform mismatch, or responsibility coupling likely to produce
  defects;
- **P3**: maintainability improvement with a measurable reduction in change risk.

Naming preferences, speculative rewrites, and findings without a specific code path or design
boundary are excluded. Duplicate symptoms are grouped by one semantic root cause.

## Audit Artifacts

The audit branch will contain:

- a coverage matrix mapping each module to checks performed and evidence;
- a findings ledger with classification, severity, locations, evidence, and disposition;
- an issue dependency graph and recommended execution order;
- a final audit report listing limitations and areas with no actionable finding.

An audit finding is either converted to a GitHub issue or closed in the ledger with a concrete
reason. The audit report never claims absence of defects beyond the checks actually performed.

## GitHub Issue Contract

Each issue is one independently reviewable and reversible semantic change. It contains:

- summary and classification;
- exact code locations;
- reproduction evidence or a structural failure path;
- impact and severity;
- proposed design boundary;
- explicit non-goals;
- acceptance criteria;
- required tests and platform conditions;
- predecessor and successor issues.

Confirmed bugs and structural-risk issues are both eligible. Closely coupled findings are
combined; independent findings are never bundled merely to reduce issue count.

## Issue Implementation Workflow

Issues are addressed in priority and dependency order:

1. create `fix/issue-N-*` or `refactor/issue-N-*` and a dedicated worktree;
2. write the issue-specific design and implementation plan in that worktree;
3. for bugs, demonstrate a failing regression test before the fix;
4. for refactors, add characterization tests before changing boundaries;
5. implement only that issue's accepted scope;
6. run focused checks plus the full applicable quality gate;
7. request review and open one pull request closing that issue.

Platform-only findings require a reproducible CI job or explicit platform test in their
acceptance criteria when they cannot be verified locally.

## Completion Criteria

The audit is complete when:

- every audit area has recorded evidence;
- every lead is classified as an issue or dismissed with a reason;
- all actionable findings have GitHub issues with dependencies and priority;
- limitations and untested platform behavior are explicit;
- the audit branch contains no product-code fixes.

The remediation program is complete only when each created issue is merged, explicitly
deferred, or closed with a documented rationale.
