# Lean Top-Budget Projection Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the top-budget projection arithmetic in Lean, replay Lean-generated expectations through the public Rust API, and repair the confirmed large-wave multiplication mismatch.

**Architecture:** Three imported Lean modules own pure projection semantics, unbounded theorems, and fixed cases. A non-imported executable performs sensitivity checks and emits a deterministic JSONL corpus; a `hoimin-core` integration test is the strict adapter. Rust retains the existing API and replaces only the `u32`-limited duration multiplication with exact-or-saturating `u128` arithmetic.

**Tech Stack:** Lean 4.32.2, Lake, Rust 2024 edition, Serde JSON, Cargo integration tests.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-top-budget-projection` on branch `audit/lean-top-budget-projection`.
- Include specification, plan, formal files, corpus, Rust tests, production repair, and audit report in that worktree.
- Use only `strict`, `model-only`, `internal-fixture`, and `infrastructure-error` correspondence modes.
- Keep imported Lean modules free of exhaustive evaluation and corpus I/O.
- Use local `maxHeartbeats 100000` limits for nontrivial proofs.
- Run one Lean command at a time with a 20-second deadline and 768 MiB RSS ceiling when the resource guard is available.
- Treat resource-guard setup or observation failure as `infrastructure-error`, never as a model result.
- Add every Rust production change only after observing its focused regression test fail.
- Do not weaken Lean semantics to match Rust behavior.

---

## File Structure

- Create `formal/HoiminOracle/HoiminOracle/TopBudgetProjectionModel.lean`: pure duration-tick, timeout, wave, projection, and broken-variant semantics.
- Create `formal/HoiminOracle/HoiminOracle/TopBudgetProjectionProofs.lean`: unbounded arithmetic and boundary theorems.
- Create `formal/HoiminOracle/HoiminOracle/TopBudgetProjectionCases.lean`: strict/model-only fixed cases and sensitivity predicates.
- Create `formal/HoiminOracle/TopBudgetProjectionAuditMain.lean`: corpus serialization, freshness, cases, sensitivity, and stats commands.
- Create `formal/HoiminOracle/corpus/top-budget-projection.jsonl`: deterministic Lean-generated expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import the three proof-oriented modules.
- Modify `formal/HoiminOracle/lakefile.toml`: register `generate_top_budget_projection`.
- Create `crates/hoimin-core/tests/lean_top_budget_projection_oracle.rs`: strict corpus parser and public-function adapter.
- Modify `crates/hoimin-core/tests/budget_projection.rs`: direct large-wave regression test.
- Modify `crates/hoimin-core/src/budget_projection.rs`: exact-or-saturating multiplication without a `u32` wave limit.
- Create `docs/superpowers/reports/2026-08-12-lean-top-budget-projection-audit.md`: self-contained correspondence and verification report.

### Task 1: Pure Lean model and unbounded arithmetic proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/TopBudgetProjectionModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/TopBudgetProjectionProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-top-budget-proof-consumer.lean`

**Interfaces:**
- Produces: `TimeoutMode`, `Input`, `Projection`, `waves`, `autoTimeout`, `effectiveTimeout`, `saturatingMul`, `project`, and the named theorems below.
- Consumes: Lean `Std` arithmetic only.

- [ ] **Step 1: Write the failing proof consumer**

Create `/tmp/hoimin-top-budget-proof-consumer.lean` with:

```lean
import HoiminOracle.TopBudgetProjectionProofs

open HoiminOracle.TopBudgetProjection

example : waves 30 4 = 8 := by decide
example : waves 0 4 = 0 := by decide

example (selected jobs : Nat) (positive : 0 < jobs) :
    selected ≤ waves selected jobs * jobs :=
  waves_cover selected jobs positive

example (input : Input) :
    (project input).projectedCapacity =
      min input.durationMax
        ((effectiveTimeout input) * waves input.selected input.jobs) :=
  projected_capacity_eq_capped_product input

example (input : Input)
    (equal : (project input).projectedCapacity = input.remaining) :
    (project input).shortfall = false :=
  equality_is_not_shortfall input equal
```

- [ ] **Step 2: Run the consumer and verify RED**

Run from `formal/HoiminOracle`:

```bash
lake env lean /tmp/hoimin-top-budget-proof-consumer.lean
```

Expected: FAIL because `HoiminOracle.TopBudgetProjectionProofs` does not exist.

- [ ] **Step 3: Implement the pure model**

Create `TopBudgetProjectionModel.lean` with these exact public definitions:

```lean
import Std

namespace HoiminOracle.TopBudgetProjection

inductive TimeoutMode
  | auto
  | fixed (ticks : Nat)
  deriving Repr, DecidableEq, BEq

structure Input where
  selected : Nat
  jobs : Nat
  plannedTotalTimeout : Nat
  baseline : Nat
  timeoutMode : TimeoutMode
  remaining : Nat
  durationMax : Nat
  ticksPerSecond : Nat
  deriving Repr, DecidableEq, BEq

structure Projection where
  selected : Nat
  jobs : Nat
  plannedTotalTimeout : Nat
  baseline : Nat
  effectiveMutantTimeout : Nat
  remaining : Nat
  waves : Nat
  projectedCapacity : Nat
  shortfall : Bool
  deriving Repr, DecidableEq, BEq

def waves (selected jobs : Nat) : Nat :=
  selected / jobs + if selected % jobs = 0 then 0 else 1

def saturatingAdd (maximum left right : Nat) : Nat :=
  min maximum (left + right)

def saturatingMul (maximum left right : Nat) : Nat :=
  min maximum (left * right)

def autoTimeout (maximum ticksPerSecond baseline : Nat) : Nat :=
  max (min maximum (5 * ticksPerSecond))
    (saturatingAdd maximum
      (saturatingMul maximum baseline 2) ticksPerSecond)

def effectiveTimeout (input : Input) : Nat :=
  match input.timeoutMode with
  | .auto => autoTimeout input.durationMax input.ticksPerSecond input.baseline
  | .fixed ticks => ticks

def project (input : Input) : Projection :=
  let waveCount := waves input.selected input.jobs
  let timeout := effectiveTimeout input
  let capacity := saturatingMul input.durationMax timeout waveCount
  { selected := input.selected
    jobs := input.jobs
    plannedTotalTimeout := input.plannedTotalTimeout
    baseline := input.baseline
    effectiveMutantTimeout := timeout
    remaining := input.remaining
    waves := waveCount
    projectedCapacity := capacity
    shortfall := capacity > input.remaining }

end HoiminOracle.TopBudgetProjection
```

- [ ] **Step 4: Implement the theorem module**

Create `TopBudgetProjectionProofs.lean` with `set_option maxHeartbeats 100000 in` around proofs and establish:

```lean
theorem waves_zero (jobs : Nat) : waves 0 jobs = 0
theorem waves_cover (selected jobs : Nat) (positive : 0 < jobs) :
  selected ≤ waves selected jobs * jobs
theorem one_fewer_wave_does_not_cover
    (selected jobs : Nat) (positiveJobs : 0 < jobs)
    (positiveSelected : 0 < selected) :
  (waves selected jobs - 1) * jobs < selected
theorem waves_eq_zero_iff (selected jobs : Nat) (positive : 0 < jobs) :
  waves selected jobs = 0 ↔ selected = 0
theorem projected_capacity_eq_capped_product (input : Input) :
  (project input).projectedCapacity =
    min input.durationMax
      ((effectiveTimeout input) * waves input.selected input.jobs)
theorem equality_is_not_shortfall (input : Input)
    (equal : (project input).projectedCapacity = input.remaining) :
  (project input).shortfall = false
theorem greater_capacity_is_shortfall (input : Input)
    (greater : input.remaining < (project input).projectedCapacity) :
  (project input).shortfall = true
theorem fixed_timeout_is_preserved (input : Input) (ticks : Nat)
    (fixed : input.timeoutMode = .fixed ticks) :
  effectiveTimeout input = ticks
```

Use `Nat.div_add_mod`, `Nat.mod_lt`, `Nat.le_div_iff_mul_le`, case splits on the remainder, `simp`, and `omega`; keep all positivity premises explicit.

- [ ] **Step 5: Import the modules and verify GREEN**

Add the model and proof imports to `HoiminOracle.lean`, then run:

```bash
lake env lean HoiminOracle/TopBudgetProjectionProofs.lean
lake env lean /tmp/hoimin-top-budget-proof-consumer.lean
```

Expected: both PASS.

- [ ] **Step 6: Commit the model and proofs**

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/HoiminOracle/TopBudgetProjectionModel.lean \
  formal/HoiminOracle/HoiminOracle/TopBudgetProjectionProofs.lean
git commit -m "test(lean): prove top-budget projection arithmetic"
```

### Task 2: Fixed cases, sensitivity, executable, and corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/TopBudgetProjectionCases.lean`
- Create: `formal/HoiminOracle/TopBudgetProjectionAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/top-budget-projection.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 1 projection semantics.
- Produces: `cases`, `sensitivityPasses`, `renderCorpus`, and executable `generate_top_budget_projection` with `--output`, `--check`, `--stats`, `--sensitivity`, and `--cases`.

- [ ] **Step 1: Register an absent executable and verify RED**

Add to `lakefile.toml`:

```toml
[[lean_exe]]
name = "generate_top_budget_projection"
root = "TopBudgetProjectionAuditMain"
```

Run:

```bash
lake exe generate_top_budget_projection -- --sensitivity
```

Expected: FAIL because `TopBudgetProjectionAuditMain.lean` does not exist.

- [ ] **Step 2: Define strict cases and broken witnesses**

Create `TopBudgetProjectionCases.lean`. Use one nanosecond as a tick,
`ticksPerSecond := 1000000000`, and
`durationMax := 18446744073709551615999999999`. Define strict cases for:

```text
zero_selection: selected=0 jobs=4 fixed=1ns remaining=0
divisible_parallel: selected=8 jobs=4 fixed=2s remaining=4s
ceiling_parallel: selected=9 jobs=4 fixed=2s remaining=5s
equal_capacity: selected=2 jobs=1 fixed=3s remaining=6s
auto_minimum: selected=1 jobs=1 auto baseline=1s remaining=5s
auto_scaled: selected=2 jobs=1 auto baseline=8s remaining=33s
large_wave_exact: selected=4294967296 jobs=1 fixed=1ns remaining=4294967295ns
duration_saturation: selected=2 jobs=1 fixed=Duration::MAX remaining=Duration::MAX
```

`large_wave_exact` is `strict` on 64-bit targets and produces capacity
`4294967296ns`, not `Duration::MAX`. A 32-bit adapter reports this case as an
`infrastructure-error` because the same `usize` premise is not configurable.

Define and retain these sensitivity predicates:

```lean
def brokenFloorWaves (selected jobs : Nat) : Nat := selected / jobs
def brokenInclusiveShortfall (capacity remaining : Nat) : Bool := capacity >= remaining
def brokenU32LimitedCapacity (maximum timeout waveCount : Nat) : Nat :=
  if waveCount > 4294967295 then maximum else saturatingMul maximum timeout waveCount
def brokenAutoWithoutIncrement (maximum ticksPerSecond baseline : Nat) : Nat :=
  max (min maximum (5 * ticksPerSecond)) (saturatingMul maximum baseline 2)
```

Assert fixed minimal witnesses for 9/4 ceiling, exact equality, 1ns times
`u32::MAX + 1`, and the 8-second automatic timeout.

- [ ] **Step 3: Implement deterministic JSONL output**

Create `TopBudgetProjectionAuditMain.lean` using `Lean.Data.Json`. Encode all
duration ticks as decimal strings to preserve values beyond JSON's portable
integer range. Each row contains:

```json
{"schema":1,"id":"large_wave_exact","mode":"strict","selected":"4294967296","jobs":"1","planned_total_timeout_ns":"10000000000","baseline_ns":"1","timeout_mode":"fixed","fixed_timeout_ns":"1","remaining_ns":"4294967295","duration_max_ns":"18446744073709551615999999999","expected_effective_timeout_ns":"1","expected_waves":"4294967296","expected_capacity_ns":"4294967296","expected_shortfall":true}
```

Before writing or checking the corpus, require every fixed case to satisfy its
contract and require all four broken witnesses to be detected. `--stats`
prints case count, strict count, duration bound, and sensitivity booleans.

- [ ] **Step 4: Verify sensitivity and generate the corpus**

Run one at a time:

```bash
lake -Kjobs=1 build
lake exe generate_top_budget_projection -- --sensitivity
lake exe generate_top_budget_projection -- --cases
lake exe generate_top_budget_projection -- --stats
lake exe generate_top_budget_projection -- --output corpus/top-budget-projection.jsonl
lake exe generate_top_budget_projection -- --check corpus/top-budget-projection.jsonl
```

Expected: all commands PASS; all four sensitivity flags are `true`; eight
cases are emitted; freshness check exits zero.

- [ ] **Step 5: Commit cases and corpus**

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/HoiminOracle/TopBudgetProjectionCases.lean \
  formal/HoiminOracle/TopBudgetProjectionAuditMain.lean \
  formal/HoiminOracle/corpus/top-budget-projection.jsonl \
  formal/HoiminOracle/lakefile.toml
git commit -m "test(lean): generate top-budget projection oracle"
```

### Task 3: Strict Rust adapter and confirmed RED mismatch

**Files:**
- Create: `crates/hoimin-core/tests/lean_top_budget_projection_oracle.rs`
- Test: `formal/HoiminOracle/corpus/top-budget-projection.jsonl`

**Interfaces:**
- Consumes: Task 2 JSONL schema and public `project_top_budget` API.
- Produces: corpus schema validation and field-for-field strict correspondence.

- [ ] **Step 1: Write the strict adapter**

Define `OracleCase` with `#[serde(deny_unknown_fields)]`; store decimal fields
as `String`, parse them to `u128`, and reject duplicate IDs, unknown modes,
zero jobs, zero fixed timeouts, unexpected schema versions, inconsistent
timeout fields, and values beyond `Duration::MAX`.

Use these conversion helpers:

```rust
const NANOS_PER_SECOND: u128 = 1_000_000_000;

fn duration_from_nanos(value: u128) -> Result<Duration, String> {
    if value > Duration::MAX.as_nanos() {
        return Err(format!("duration {value} exceeds Duration::MAX"));
    }
    Ok(Duration::new(
        (value / NANOS_PER_SECOND) as u64,
        (value % NANOS_PER_SECOND) as u32,
    ))
}
```

Construct fixed timeouts through `RawRunConfig` so the adapter uses the owned
public configuration path. For every strict, platform-representable case,
compare `selected`, `jobs`, `planned_total_timeout`, `baseline`,
`effective_mutant_timeout`, `remaining`, `waves`, `projected_capacity`, and
`is_shortfall()`.

- [ ] **Step 2: Run the adapter and verify the semantic RED**

Run:

```bash
cargo test -p hoimin-core --test lean_top_budget_projection_oracle -- --nocapture
```

Expected on 64-bit: FAIL only for `large_wave_exact`; Lean expects
`4294967296ns`, while current Rust returns `Duration::MAX`. This establishes a
strict same-premise mismatch caused by converting wave count to `u32` before
multiplication.

- [ ] **Step 3: Commit the red adapter evidence**

```bash
git add crates/hoimin-core/tests/lean_top_budget_projection_oracle.rs
git commit -m "test: expose top-budget large-wave mismatch"
```

### Task 4: TDD Rust repair for exact large-wave multiplication

**Files:**
- Modify: `crates/hoimin-core/tests/budget_projection.rs`
- Modify: `crates/hoimin-core/src/budget_projection.rs`

**Interfaces:**
- Consumes: existing `project_top_budget` signature unchanged.
- Produces: private `saturating_duration_mul(Duration, usize) -> Duration` and correct exact-or-saturated capacity.

- [ ] **Step 1: Add the focused regression test**

On 64-bit targets, add:

```rust
#[cfg(target_pointer_width = "64")]
#[test]
fn wave_counts_above_u32_max_multiply_exactly_when_duration_fits() {
    let selected = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
    let projection = project_top_budget(
        selected,
        NonZeroUsize::new(1).unwrap(),
        Duration::from_secs(10),
        Duration::from_nanos(1),
        fixed_timeout(Duration::from_nanos(1)),
        Duration::from_nanos(u64::from(u32::MAX)),
    );

    assert_eq!(projection.waves, selected);
    assert_eq!(
        projection.projected_capacity,
        Duration::from_nanos(u64::from(u32::MAX) + 1),
    );
    assert!(projection.is_shortfall());
}
```

- [ ] **Step 2: Run the focused test and verify RED**

```bash
cargo test -p hoimin-core --test budget_projection \
  wave_counts_above_u32_max_multiply_exactly_when_duration_fits -- --exact
```

Expected: FAIL because actual capacity is `Duration::MAX`.

- [ ] **Step 3: Implement exact-or-saturating multiplication**

In `budget_projection.rs`, add:

```rust
const NANOS_PER_SECOND: u128 = 1_000_000_000;

fn saturating_duration_mul(duration: Duration, factor: usize) -> Duration {
    let Some(nanos) = duration.as_nanos().checked_mul(factor as u128) else {
        return Duration::MAX;
    };
    if nanos > Duration::MAX.as_nanos() {
        return Duration::MAX;
    }
    Duration::new(
        (nanos / NANOS_PER_SECOND) as u64,
        (nanos % NANOS_PER_SECOND) as u32,
    )
}
```

Replace the `u32::try_from(waves)` match with:

```rust
let projected_capacity = saturating_duration_mul(effective_mutant_timeout, waves);
```

No public signature or warning boundary changes.

- [ ] **Step 4: Verify GREEN and all focused projection behavior**

```bash
cargo test -p hoimin-core --test budget_projection
cargo test -p hoimin-core --test lean_top_budget_projection_oracle -- --nocapture
cargo test -p hoimin-core --test machine \
  top_budget_shortfall_warns_before_requesting_analysis -- --exact
```

Expected: all PASS and the strict mismatch set is empty.

- [ ] **Step 5: Refactor only after GREEN**

Run `cargo fmt --all`, then rerun the two projection integration tests. Keep
the helper private and do not modify `TopBudgetProjection` fields.

- [ ] **Step 6: Commit the repair**

```bash
git add crates/hoimin-core/src/budget_projection.rs \
  crates/hoimin-core/tests/budget_projection.rs
git commit -m "fix: multiply large top-budget wave counts exactly"
```

### Task 5: Audit report and complete verification

**Files:**
- Create: `docs/superpowers/reports/2026-08-12-lean-top-budget-projection-audit.md`
- Modify only if evidence requires: files from Tasks 1-4.

**Interfaces:**
- Consumes: all proof, corpus, adapter, and Rust repair evidence.
- Produces: self-contained audit handoff with exact commands and correspondence classifications.

- [ ] **Step 1: Run final Lean evidence one command at a time**

Use the resource guard with a writable `/tmp` stats path when process-table
observation is available. Otherwise record `infrastructure-error` and run the
same command with an independent 20-second wall-clock deadline:

```bash
lake -Kjobs=1 build
lake env lean /tmp/hoimin-top-budget-proof-consumer.lean
lake exe generate_top_budget_projection -- --sensitivity
lake exe generate_top_budget_projection -- --cases
lake exe generate_top_budget_projection -- --stats
lake exe generate_top_budget_projection -- --check corpus/top-budget-projection.jsonl
```

- [ ] **Step 2: Run focused and workspace Rust verification**

```bash
cargo test -p hoimin-core --test budget_projection
cargo test -p hoimin-core --test lean_top_budget_projection_oracle -- --nocapture
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Expected: every command exits zero.

- [ ] **Step 3: Write the audit report**

The report must contain:

- the durable claim and explicit boundary;
- the correspondence worksheet and per-mode status;
- declared versus previously implicit `u32` behavior;
- theorem premises and what Lean established only inside the model;
- fixed cases, finite bounds, sensitivity witnesses, and why atomicity and
  idempotency do not apply;
- the minimal strict counterexample: selected `4294967296`, jobs `1`, fixed
  timeout `1ns`, expected capacity `4294967296ns`, old actual
  `Duration::MAX`;
- classification `confirmed bug`, source location, impact, and repair;
- post-repair strict correspondence result;
- exact resource limits, elapsed time/RSS measurements, and every
  infrastructure error;
- exact reproduction commands and unresolved owner decisions.

- [ ] **Step 4: Verify documentation and worktree state**

```bash
rg -n "T[B]D|T[O]DO|implement lat[e]r|fill in" \
  docs/superpowers/reports/2026-08-12-lean-top-budget-projection-audit.md
git diff --check
git status --short
```

Expected: no placeholders, no whitespace errors, and only the intended report
is uncommitted.

- [ ] **Step 5: Commit the report**

```bash
git add docs/superpowers/reports/2026-08-12-lean-top-budget-projection-audit.md
git commit -m "docs: report Lean top-budget projection audit"
```

- [ ] **Step 6: Record final branch evidence**

```bash
git status -sb
git log --oneline --decorate -8
```

Expected: clean `audit/lean-top-budget-projection` worktree containing the
design, plan, formal evidence, generated corpus, Rust adapter, repair, and
report.
