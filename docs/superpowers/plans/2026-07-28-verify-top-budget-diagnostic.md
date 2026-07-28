# Verify Top Budget Diagnostic Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Warn before mutation execution when a saved plan's immutable timeout capacity is likely insufficient for a `verify --top N` selection.

**Architecture:** A pure `hoimin-core` projection policy calculates timeout-capacity from the fresh baseline, selected count, jobs, mutant timeout policy, and observed remaining total budget. After a successful top-N baseline, the core state machine requests a typed remaining-budget observation from the shell, optionally emits the existing typed diagnostic event, and then proceeds without changing plan settings or run semantics.

**Tech Stack:** Rust 2024, hoimin-core state machine, Tokio monotonic time, existing report handlers, Cargo integration tests

## Global Constraints

- Diagnose only `verify --top N`; explicit candidate IDs and ordinary `run` do not observe or warn about budget.
- Use `nightly-2026-07-27` only for the existing randomized test command; production remains stable Rust with MSRV 1.85.
- Never modify jobs, timeouts, selected candidates, exit codes, or incomplete-result semantics.
- Use strict shortfall comparison: projected capacity equal to remaining budget does not warn.
- Saturate all duration and wave arithmetic instead of overflowing.
- Keep JSON and JSONL stdout machine-readable; the warning uses the existing diagnostic-to-stderr path.
- Warning code is exactly `budget.projected_shortfall`.
- The message states that the estimate is not a guaranteed failure and instructs the user to create a new plan with different `--jobs` and/or `--total-timeout`.
- Do not add third-party dependencies.

---

### Task 1: Add the pure timeout-capacity projection policy

**Files:**
- Create: `crates/hoimin-core/src/budget_projection.rs`
- Modify: `crates/hoimin-core/src/lib.rs`
- Create: `crates/hoimin-core/tests/budget_projection.rs`

**Interfaces:**
- Consumes: `MutantTimeout`, `NonZeroDuration`, `auto_mutant_timeout`, `std::num::NonZeroUsize`, and `std::time::Duration`
- Produces:

```rust
pub struct TopBudgetProjection {
    pub selected: usize,
    pub jobs: usize,
    pub planned_total_timeout: Duration,
    pub baseline: Duration,
    pub effective_mutant_timeout: Duration,
    pub remaining: Duration,
    pub waves: usize,
    pub projected_capacity: Duration,
}

impl TopBudgetProjection {
    #[must_use]
    pub fn is_shortfall(&self) -> bool;
}

#[must_use]
pub fn project_top_budget(
    selected: usize,
    jobs: NonZeroUsize,
    planned_total_timeout: Duration,
    baseline: Duration,
    mutant_timeout: MutantTimeout,
    remaining: Duration,
) -> TopBudgetProjection;
```

- [ ] **Step 1: Write failing pure policy tests**

Create tests for the approved examples:

```rust
#[test]
fn serial_auto_timeout_projects_thirty_waves_and_a_shortfall() {
    let projection = project_top_budget(
        30,
        NonZeroUsize::new(1).unwrap(),
        Duration::from_secs(300),
        Duration::from_secs(17),
        MutantTimeout::Auto,
        Duration::from_secs(281),
    );

    assert_eq!(projection.effective_mutant_timeout, Duration::from_secs(35));
    assert_eq!(projection.waves, 30);
    assert_eq!(projection.projected_capacity, Duration::from_secs(1_050));
    assert!(projection.is_shortfall());
}

#[test]
fn parallel_projection_uses_ceiling_wave_count() {
    let projection = project_top_budget(
        30,
        NonZeroUsize::new(4).unwrap(),
        Duration::from_secs(600),
        Duration::from_secs(17),
        MutantTimeout::Auto,
        Duration::from_secs(565),
    );

    assert_eq!(projection.waves, 8);
    assert_eq!(projection.projected_capacity, Duration::from_secs(280));
    assert!(!projection.is_shortfall());
}
```

Also add:

- fixed timeout uses the exact configured duration;
- `projected_capacity == remaining` is not a shortfall;
- `selected == 0` produces zero waves and zero capacity;
- maximum selected count and duration saturate without panic.

- [ ] **Step 2: Run tests and verify RED**

Run:

```bash
cargo test -p hoimin-core --test budget_projection
```

Expected: compilation fails because `budget_projection` and its public interfaces do not exist.

- [ ] **Step 3: Implement ceiling division and saturating duration multiplication**

Implement without floating point:

```rust
let jobs = jobs.get();
let waves = selected.saturating_add(jobs - 1) / jobs;
let effective_mutant_timeout = match mutant_timeout {
    MutantTimeout::Auto => auto_mutant_timeout(baseline),
    MutantTimeout::Fixed(value) => value.get(),
};
let projected_capacity = effective_mutant_timeout
    .checked_mul(u32::try_from(waves).unwrap_or(u32::MAX))
    .unwrap_or(Duration::MAX);
```

If `waves` exceeds `u32::MAX`, set `projected_capacity` to `Duration::MAX`
instead of treating `u32::MAX` as the real wave count.

Export the module and symbols from `lib.rs`. Derive `Clone`, `Copy`, `Debug`,
`Eq`, and `PartialEq` for the projection.

- [ ] **Step 4: Run focused and core tests**

Run:

```bash
cargo test -p hoimin-core --test budget_projection
cargo test -p hoimin-core
```

Expected: all pass.

- [ ] **Step 5: Commit the policy**

```bash
git add crates/hoimin-core/src/budget_projection.rs \
  crates/hoimin-core/src/lib.rs \
  crates/hoimin-core/tests/budget_projection.rs
git commit -m "feat: project verify top timeout capacity"
```

---

### Task 2: Add typed budget observation to the state machine

**Files:**
- Modify: `crates/hoimin-core/src/effect.rs`
- Modify: `crates/hoimin-core/src/event.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-core/tests/workspace_effects.rs`

**Interfaces:**
- Consumes: `TopBudgetProjection`, `RunState::verification_selection`, fresh `baseline_elapsed`, and immutable `RunConfig::limits`
- Produces:

```rust
pub struct ObserveRemainingBudget {
    pub id: EffectId,
}

pub struct RemainingBudgetObserved {
    pub id: EffectId,
    pub remaining: Duration,
}

RunEffect::ObserveRemainingBudget(ObserveRemainingBudget)
RunEvent::RemainingBudgetObserved(RemainingBudgetObserved)
```

- [ ] **Step 1: Write failing lifecycle tests**

In `crates/hoimin-core/tests/machine.rs`, extend the baseline-success fixture
with three tests:

```rust
#[test]
fn top_verification_observes_budget_after_successful_baseline() {
    // Build state with VerificationSelectionMode::Top and selected=30.
    // Advance through worker creation and complete a 17-second baseline.
    // Assert one ObserveRemainingBudget effect and no AnalyzeFile effect yet.
}

#[test]
fn explicit_verification_skips_budget_observation() {
    // Use VerificationSelectionMode::CandidateIds.
    // Assert baseline success proceeds directly to AnalyzeFile.
}

#[test]
fn ordinary_run_skips_budget_observation() {
    // Leave verification_selection unset.
    // Assert baseline success proceeds directly to AnalyzeFile.
}
```

Add an effect/event serialization contract in `workspace_effects.rs` that
round-trips the new public types and confirms the effect ID is preserved.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test -p hoimin-core --test machine top_verification_observes_budget
cargo test -p hoimin-core --test workspace_effects
```

Expected: compilation fails because the effect/event variants do not exist.

- [ ] **Step 3: Add the effect/event types and completion registration**

Update all exhaustive mappings:

- `RunEffect::id`;
- `RunEvent` serialization;
- `expected_completion`;
- `completion_identity`;
- the private `CompletionKind` enum.

Use a dedicated completion kind such as `RemainingBudgetObserved`. Do not
classify it as a process completion and do not attach a worker.

- [ ] **Step 4: Gate the observation on a successful top-N baseline**

After emitting `BaselineFinished`, branch as follows:

```rust
if success && state.is_top_verification() {
    state.phase = RunPhase::BudgetCheck;
    effects.push(RunEffect::ObserveRemainingBudget(
        ObserveRemainingBudget { id: state.allocate_id()? },
    ));
} else if success {
    state.phase = RunPhase::Analyze;
    effects.extend(state.analyze_next()?);
} else {
    // Preserve existing baseline-failure finalization.
}
```

Add `RunPhase::BudgetCheck` only if phase validation requires a distinct
state. Include it in copy-grant and worker invariants like `Baseline` and
`Analyze`.

- [ ] **Step 5: Write failing diagnostic transition tests**

Add deterministic state-machine tests that feed:

```rust
RemainingBudgetObserved {
    id: observation.id,
    remaining: Duration::from_secs(281),
}
```

For selected=30, jobs=1, baseline=17s, and auto timeout, assert:

- first produced effect is `EmitOutput` containing a warning `Diagnostic`;
- code equals `budget.projected_shortfall`;
- message contains `selected=30`, `jobs=1`, `planned_total_timeout=300s`,
  `baseline=17s`, `effective_mutant_timeout=35s`, `remaining=281s`,
  `projected_capacity=1050s`, `not a guaranteed failure`,
  `--jobs`, `--total-timeout`, and `new plan`;
- analysis is not requested until `OutputEmitted` acknowledges the warning.

Add a sufficient-budget case that emits no diagnostic and proceeds directly
to analysis. Assert both cases preserve the previous exit/incomplete flags.

- [ ] **Step 6: Implement diagnostic ordering**

Add a specific output action or pending diagnostic continuation so the warning
is acknowledged before `AnalyzeFile` is emitted. Reuse `Diagnostic::new` and
the normal output sequence. Format all durations deterministically as whole
seconds with a saturating conversion:

```text
planned_total_timeout=300s
baseline=17s
effective_mutant_timeout=35s
remaining=281s
projected_capacity=1050s
```

Do not use a free-standing `eprintln!`.

- [ ] **Step 7: Run core tests**

Run:

```bash
cargo test -p hoimin-core --test machine
cargo test -p hoimin-core --test workspace_effects
cargo test -p hoimin-core
```

Expected: all pass.

- [ ] **Step 8: Commit typed state-machine behavior**

```bash
git add crates/hoimin-core/src/effect.rs \
  crates/hoimin-core/src/event.rs \
  crates/hoimin-core/src/machine.rs \
  crates/hoimin-core/tests/machine.rs \
  crates/hoimin-core/tests/workspace_effects.rs
git commit -m "feat: diagnose verify top budget shortfalls"
```

---

### Task 3: Observe the live deadline in the shell

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Consumes: `ObserveRemainingBudget`, the existing absolute Tokio `deadline`, and `tokio::time::Instant::now()`
- Produces: `RemainingBudgetObserved { id, remaining }`

- [ ] **Step 1: Write a failing shell unit test for the clock boundary**

Extract a pure helper and test it with paused Tokio time or explicit instants:

```rust
fn remaining_budget_observed(
    request: ObserveRemainingBudget,
    deadline: tokio::time::Instant,
    now: tokio::time::Instant,
) -> RemainingBudgetObserved
```

The test asserts:

- a deadline 281 seconds ahead produces `remaining == 281s`;
- an expired deadline produces `Duration::ZERO`;
- the effect ID is unchanged.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test -p hoimin-cli shell::tests::remaining_budget
```

Expected: compilation fails because the helper and dispatch branch do not
exist.

- [ ] **Step 3: Implement the shell observation**

Use:

```rust
let remaining = deadline.saturating_duration_since(now);
```

Handle `RunEffect::ObserveRemainingBudget` in the run loop's serial effect
path, where the absolute deadline is available. Return the typed event through
`ShellCompletion`; do not perform report I/O or projection policy in the shell.

Keep `execute_effect_with_cancellation` exhaustive by either delegating the new
variant to the helper with explicit deadline/now parameters or isolating this
scheduler-owned effect before the general handler match. Document why wall
clock observation is scheduler-owned.

- [ ] **Step 4: Run shell and workspace tests**

Run:

```bash
cargo test -p hoimin-cli shell::tests::remaining_budget
cargo test --workspace
```

Expected: all pass.

- [ ] **Step 5: Commit the runtime observation**

```bash
git add crates/hoimin-cli/src/shell.rs
git commit -m "feat: observe remaining verify budget"
```

---

### Task 4: Prove output isolation and immutable verify behavior

**Files:**
- Modify: `crates/hoimin-cli/tests/plan.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: saved top-ranked manifest, exact `verify --top` CLI path, and the
  `budget.projected_shortfall` diagnostic
- Produces: end-to-end evidence for warning content, stdout isolation, and
  unchanged manifest settings

- [ ] **Step 1: Write a failing end-to-end JSON test**

Using the existing plan fixture helpers in `crates/hoimin-cli/tests/plan.rs`:

1. create a plan with one job, a deliberately small total timeout, and at least
   two ranked candidates;
2. invoke `verify --plan <PATH> --top 2 --format json`;
3. use a successful baseline fixture long enough for timeout capacity to exceed
   the remaining budget;
4. parse stdout as exactly one JSON document;
5. assert stderr contains one `warning[budget.projected_shortfall]` with every
   approved field and replan instruction;
6. reload the plan and assert jobs, mutant timeout, and total timeout are
   byte-for-byte unchanged.

Also add:

- a JSONL case where every nonempty stdout line parses as JSON and the warning
  appears only on stderr;
- an explicit candidate-ID case with the same limits and no budget warning.

- [ ] **Step 2: Run focused integration tests and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test plan budget_shortfall -- --nocapture
```

Expected: assertions fail because no budget warning exists.

- [ ] **Step 3: Add concise user documentation**

In the plan-and-verify section of `README.md`, document:

- `verify --top` retains all limits from the manifest;
- timeout-capacity warnings are estimates, not guaranteed failures;
- users must create a new plan to change `--jobs` or `--total-timeout`.

Do not document automatic adjustment because none exists.

- [ ] **Step 4: Run plan and report regression tests**

Run:

```bash
cargo test -p hoimin-cli --test plan
cargo test -p hoimin-cli --test report_handler
cargo test -p hoimin-cli --test cli_config
```

Expected: all pass.

- [ ] **Step 5: Commit integration coverage and docs**

```bash
git add crates/hoimin-cli/tests/plan.rs README.md
git commit -m "test: cover verify top budget warnings"
```

---

### Task 5: Verify the complete feature and request review

**Files:**
- Inspect: all files changed since `origin/main`

**Interfaces:**
- Consumes: completed Tasks 1-4
- Produces: integration-ready branch with fresh stable, Python, and randomized-order evidence

- [ ] **Step 1: Run formatting and static analysis**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

Expected: exit 0.

- [ ] **Step 2: Run stable and Python suites**

```bash
cargo test --workspace
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: all pass.

- [ ] **Step 3: Run randomized Rust order**

```bash
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

Expected: all test binaries print generated shuffle seeds and pass.

- [ ] **Step 4: Audit requirements and repository state**

Verify:

- only top-N paths issue `ObserveRemainingBudget`;
- no code mutates normalized plan limits;
- no warning is printed directly with `eprintln!`;
- release workflows are unchanged;
- `.serena/` remains untracked and is not committed;
- worktree contains no unrelated changes.

- [ ] **Step 5: Request independent code review**

Use `superpowers:requesting-code-review` with base `origin/main` and the final
HEAD. Resolve every Critical or Important finding, rerun affected tests, and
request a focused re-review.

- [ ] **Step 6: Finish the branch**

Use `superpowers:finishing-a-development-branch`. Keep this dedicated worktree
for PR feedback if the user selects push-and-PR.
