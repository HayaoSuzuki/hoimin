# Lean top-budget projection audit report

## Result

This audit found and repaired one implementation bug in
`project_top_budget`. On a 64-bit target, a wave count above `u32::MAX` was
treated as immediate duration overflow even when the mathematical product was
small enough for `Duration`.

The minimal strict counterexample was:

```text
selected:                    4,294,967,296
jobs:                        1
fixed timeout:               1 ns
remaining budget:            4,294,967,295 ns
required waves:              4,294,967,296
Lean expected capacity:      4,294,967,296 ns
old Rust capacity:           Duration::MAX
expected shortfall:          true
old Rust shortfall:          true
classification:              confirmed bug
```

The warning boolean happened to agree in this witness, but the public
`projected_capacity` observation and diagnostic value were drastically wrong.
Other remaining-budget values could also turn the premature saturation into a
false shortfall warning.

Rust now multiplies nanosecond counts by the full `usize` wave count using
checked `u128` arithmetic, returning the exact `Duration` when representable
and `Duration::MAX` only on real arithmetic or duration overflow. After the
repair, all eight strict Lean-generated cases match every stable field returned
by the public Rust function.

Lean proves properties of the independent model. It does not prove the Rust
implementation. The generated corpus and Rust adapter provide the exercised
same-premise correspondence described below.

## Claim and boundary

The audited claim is:

> For a positive worker count, top verification requires the least whole
> number of worker waves that covers every selected mutant. Projected capacity
> is the effective per-mutant timeout multiplied by that wave count, saturated
> at the representable duration maximum. A shortfall is reported if and only
> if projected capacity is strictly greater than remaining budget.

Included behavior is zero and positive selection counts, positive job counts,
divisible and non-divisible wave boundaries, fixed and automatic timeouts,
duration saturation, and equality/strict shortfall boundaries.

Excluded behavior is scheduler fairness, real completion time, OS timer
resolution, configuration parsing, `RunState` phase transitions, warning
formatting, and whether a projection guarantees actual failure.

## Declared and implicit behavior

Declared behavior was already visible in tests and callers:

- waves use ceiling division of selected work by positive jobs;
- automatic timeout is `max(5 seconds, 2 * baseline + 1 second)`;
- projected capacity saturates rather than panics;
- equality with remaining budget is not a shortfall;
- the result reports its inputs and all derived values.

The implementation also had an undocumented implicit boundary: it converted
`waves` to `u32` before using `Duration::checked_mul`. A conversion failure
returned `Duration::MAX`, although Rust's public input and returned wave count
are `usize`. That narrower multiplication premise was neither required by the
API nor equivalent to duration overflow on 64-bit systems.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public observation | Evidence | Mode | Final status |
| --- | --- | --- | --- | --- | --- | --- |
| selected count | `Input.selected : Nat` | `selected : usize` | returned `selected`, `waves` | direct public call | `strict` when representable | match after repair |
| positive jobs | `Input.jobs : Nat`, theorem premise `0 < jobs` | `NonZeroUsize` | returned `jobs`, `waves` | direct public call | `strict` | match |
| fixed timeout | `TimeoutMode.fixed ticks` | owned `RawRunConfig` conversion | effective timeout and capacity | public configuration plus call | `strict` | match |
| automatic timeout | `TimeoutMode.auto` | `MutantTimeout::Auto` | effective timeout and capacity | direct public call | `strict` | match |
| duration maximum | `durationMax = 18446744073709551615999999999 ns` | `Duration::MAX` | projected capacity | direct public call | `strict` | match |
| remaining budget | `Input.remaining` | `Duration` argument | returned remaining and `is_shortfall()` | direct public call | `strict` | match |
| planned total timeout | pass-through natural-number ticks | `Duration` argument | returned planned timeout | direct public call | `strict` | match |
| large 64-bit selection | `selected = 2^32` | representable `usize` on 64-bit | complete projection | direct public call | `strict` on 64-bit | old mismatch; match after repair |
| large selection on 32-bit | same model value | not representable as `usize` | cannot call same premise | adapter classification | `infrastructure-error` | no semantic conclusion |

The final run was on a 64-bit target, so all eight strict rows were comparable.
The adapter compares selected, jobs, planned timeout, baseline, effective
timeout, remaining, waves, projected capacity, and shortfall. It does not
encode expected arithmetic independently in Rust.

## Lean evidence

The imported library contains pure semantics and kernel-checked proofs. With a
local `maxHeartbeats 100000` bound on the nontrivial arithmetic proofs, Lean
establishes:

- `waves_zero`: zero selected work requires zero waves;
- `waves_cover`: for arbitrary naturals and positive jobs, the computed wave
  count covers every selected item;
- `one_fewer_wave_does_not_cover`: for positive selection and jobs, one fewer
  wave cannot cover the selection;
- `waves_eq_zero_iff`: with positive jobs, zero waves is equivalent to zero
  selected work;
- `projected_capacity_eq_capped_product`: modeled capacity is exactly the
  mathematical timeout/wave product capped at the explicit maximum;
- `equality_is_not_shortfall` and `greater_capacity_is_shortfall`: equality is
  accepted and strictly greater capacity is rejected;
- `fixed_timeout_is_preserved` and `auto_timeout_uses_saturated_rule`: both
  timeout branches have explicit semantics.

These are unbounded theorems about the Lean model under their visible premises.
They do not prove `std::time::Duration`, Rust casts, or `project_top_budget`.

No bounded trace exploration was needed because the audited function is pure.
The executable evaluates eight named semantic boundary cases rather than an
arbitrary numeric range. The corpus is 8 JSONL rows and 3,232 bytes. Duration
values are decimal strings so the serialization does not lose precision.

## Fixed cases

| Case | Premise | Expected boundary | Mode | Final result |
| --- | --- | --- | --- | --- |
| `zero_selection` | selected 0, jobs 4 | zero waves and capacity | `strict` | match |
| `divisible_parallel` | selected 8, jobs 4 | exactly 2 waves | `strict` | match |
| `ceiling_parallel` | selected 9, jobs 4 | ceiling to 3 waves | `strict` | match |
| `equal_capacity` | capacity equals remaining | no shortfall | `strict` | match |
| `auto_minimum` | 1-second baseline | 5-second minimum | `strict` | match |
| `auto_scaled` | 8-second baseline | 17-second timeout | `strict` | match |
| `large_wave_exact` | `2^32` waves at 1 ns | exact 4.294967296 seconds | `strict` | old mismatch; match after repair |
| `duration_saturation` | 10 billion waves at the accepted 100-year timeout ceiling | `Duration::MAX` | `strict` | match |

## Refutation sensitivity

The executable refuses corpus output or freshness success unless fixed broken
variants are distinguished:

| Risk family | Broken behavior | Fixed witness | Detected |
| --- | --- | --- | --- |
| boundary: wave rounding | floor division instead of ceiling | selected 9, jobs 4: 2 versus 3 waves | yes |
| boundary: precedence | `capacity >= remaining` | exact 6-second equality | yes |
| boundary: representation | treat wave count above `u32::MAX` as overflow | `2^32 * 1 ns` | yes |
| boundary: automatic timeout | omit one-second increment or five-second floor | 8-second and 1-second baselines | yes |

Atomicity/transactionality does not apply because this is a pure calculation
with no partial state mutation. Uniqueness/idempotency does not apply because
there is no identity allocation, durable marker, effect, or replay transition.
Both exclusions are domain-specific rather than omitted checks.

## Counterexample record

```text
claim:
  projected capacity is the exact timeout × least wave count, capped only at
  Duration::MAX
model boundary:
  pure arithmetic; 64-bit usize and nanosecond Duration projection
finite domain or theorem premises:
  strict fixed case selected=2^32, jobs=1, timeout=1ns; general Lean product
  theorem uses natural-number ticks and explicit maximum
minimal input or trace:
  no trace; selected=4294967296, jobs=1, fixed timeout=1ns,
  remaining=4294967295ns
intermediate states:
  waves=4294967296; exact product=4294967296ns; old u32 conversion fails;
  old capacity=Duration::MAX
classification:
  confirmed bug
implementation correspondence:
  strict mismatch before repair; strict match after repair on all compared fields
owner question:
  none; the public usize API and existing saturation contract own exact
  representable multiplication
reproduction command:
  cargo test -p hoimin-core --test budget_projection \
    wave_counts_above_u32_max_multiply_exactly_when_duration_fits -- --exact
```

## Rust repair

The old implementation used:

```rust
match u32::try_from(waves) {
    Ok(waves) => effective_mutant_timeout.checked_mul(waves),
    Err(_) => Duration::MAX,
}
```

The repair computes `duration.as_nanos().checked_mul(factor as u128)`, checks
against `Duration::MAX.as_nanos()`, converts seconds with checked `u64`
conversion, and constructs the exact normalized duration. The API, wave
formula, timeout policy, and shortfall comparison are unchanged.

The direct regression test was observed failing before the repair with old
actual `Duration::MAX` and expected `4.294967296s`. The independent strict
corpus adapter failed on the same case before the production edit. Both pass
afterward.

## Resource observations

Lean commands ran one at a time under a 20-second wall-clock deadline, a
768 MiB (786,432 KiB) process-tree RSS ceiling, 250 ms samples, and the local
100,000-heartbeat theorem limit.

| Command | Elapsed | Peak RSS | Result |
| --- | ---: | ---: | --- |
| `lake -Kjobs=1 build` | 322 ms | 2,352 KiB | pass, 41 jobs |
| proof consumer | 2,501 ms | 655,136 KiB | pass |
| sensitivity | 299 ms | 2,832 KiB | pass |
| fixed cases | 294 ms | 2,784 KiB | pass |
| stats | 309 ms | 2,784 KiB | pass |
| corpus freshness | 298 ms | 2,800 KiB | pass |

The initial sandboxed resource-guard attempt returned exit 126 with reason
`monitor_error` because process-table observation was unavailable. That run is
classified as `infrastructure-error` and supports no semantic conclusion. The
same commands were rerun through the approved observation path and produced
the successful measurements above. No timeout, RSS kill, swap, unbounded
heartbeat setting, or abandoned larger bound occurred.

## Verification record

| Command | Result |
| --- | --- |
| resource-guarded `lake -Kjobs=1 build` | pass, 41 jobs |
| resource-guarded proof consumer | pass |
| `generate_top_budget_projection -- --sensitivity` | all four broken boundaries detected |
| `generate_top_budget_projection -- --cases` | 8/8 true |
| `generate_top_budget_projection -- --stats` | 8 fixed / 8 strict cases |
| `generate_top_budget_projection -- --check corpus/top-budget-projection.jsonl` | pass/fresh |
| `cargo test -p hoimin-core --test budget_projection` | 8 passed |
| `cargo test -p hoimin-core --test lean_top_budget_projection_oracle -- --nocapture` | 2 passed |
| `cargo test -p hoimin-core --test machine top_budget_shortfall_warns_before_requesting_analysis -- --exact` | 1 passed |
| `cargo test --workspace --all-features` | pass after worktree `.venv` setup |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo fmt --all -- --check` and `git diff --check` | pass |

The first workspace run exposed a missing worktree-local `.venv` symlink when
an existing candidate-ranking test tried to spawn its recorded Python path.
The exact test reproduced the same `os error 2`; comparison with the existing
audit worktree showed its setup symlink `.venv -> ../../.venv`. Adding that
git-ignored worktree setup restored the test, and the complete workspace run
then passed. This was an infrastructure error, not a semantic mismatch and not
a repository change.

## Classification and owner decisions

- Confirmed bugs: one, premature `u32`-limited capacity saturation; repaired.
- Final strict mismatches: none on the 64-bit run.
- Model defects found: none after the approved design boundary was encoded.
- Unresolved specification ambiguities: none within the audited projection.
- Unresolved infrastructure errors: none; both setup errors were diagnosed and
  rerun successfully.
- Production changes outside the confirmed arithmetic repair: none.
- Further owner decision: none required.

## Exact reproduction commands

From the worktree root:

```bash
cd formal/HoiminOracle
python3 tools/lean_resource_guard.py --timeout-seconds 20 \
  --rss-limit-mib 768 --sample-ms 250 \
  --stats /tmp/hoimin-top-budget-build.json -- lake -Kjobs=1 build
lake exe generate_top_budget_projection -- --sensitivity
lake exe generate_top_budget_projection -- --cases
lake exe generate_top_budget_projection -- --stats
lake exe generate_top_budget_projection -- \
  --check corpus/top-budget-projection.jsonl

cd ../..
cargo test -p hoimin-core --test budget_projection
cargo test -p hoimin-core --test lean_top_budget_projection_oracle -- --nocapture
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```
