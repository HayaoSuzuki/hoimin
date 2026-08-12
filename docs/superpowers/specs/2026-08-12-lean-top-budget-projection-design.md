# Lean top-budget projection audit design

## Goal

Formally audit the arithmetic contract behind `project_top_budget`, compare a
Lean-generated corpus with the public Rust function under the same premises,
and repair any confirmed Rust mismatch with a regression test first.

The durable claim is:

> For a positive worker count, top verification requires the least whole
> number of worker waves that covers every selected mutant. Its projected
> capacity is the effective per-mutant timeout multiplied by that wave count,
> saturated at the representable duration maximum. A warning is required if
> and only if that capacity is strictly greater than the observed remaining
> budget.

This work is isolated in `.worktrees/lean-top-budget-projection` on branch
`audit/lean-top-budget-projection`. The specification, implementation plan,
formal artifacts, generated corpus, Rust tests, and audit report all belong to
that worktree and branch.

## Scope

Included behavior:

- zero and positive selected counts;
- positive parallel job counts;
- floor-versus-ceiling wave boundaries;
- fixed timeouts and the current automatic timeout rule
  `max(5 seconds, 2 * baseline + 1 second)` with saturation;
- duration multiplication and saturation;
- strict `projected_capacity > remaining` warning semantics;
- stable observations returned by the public `project_top_budget` Rust API.

Excluded behavior:

- scheduler fairness and actual wall-clock completion;
- OS timer resolution;
- `RunState` phase transitions and warning presentation;
- whether the warning is a guaranteed prediction of failure;
- configuration parsing, which already establishes nonzero jobs and fixed
  timeouts before this function is called.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| selected mutant count | `selected : Nat` | `selected : usize` argument | returned `selected` and `waves` | public function call | `strict` for representable corpus values |
| positive worker count | `jobs : Nat` plus `0 < jobs` | `NonZeroUsize` argument | returned `jobs` and `waves` | public function call | `strict` |
| fixed timeout | fixed `Nat` duration ticks | `MutantTimeout::Fixed` | `effective_mutant_timeout` | public function call | `strict` |
| automatic timeout | saturating `max 5 (2 * baseline + 1)` | `MutantTimeout::Auto` | `effective_mutant_timeout` | public function call | `strict` |
| duration maximum | explicit `durationMax` bound | `Duration::MAX` | `projected_capacity` | public function call | `strict` at exact Rust values; reduced bounds are `model-only` |
| remaining budget | `remaining : Nat` | `Duration` argument | returned `remaining` and `is_shortfall()` | public function call | `strict` |
| planned total timeout | opaque pass-through duration | `Duration` argument | returned `planned_total_timeout` | public function call | `strict` |

Only corpus rows whose inputs and complete observations are exactly
representable by Rust are marked `strict`. Reduced-cap sensitivity witnesses
remain `model-only` and cannot establish a Rust mismatch.

## Formal model and proofs

Add focused `TopBudgetProjectionModel`, `TopBudgetProjectionProofs`, and
`TopBudgetProjectionCases` modules. Imported modules contain pure definitions
and kernel-checked proofs; corpus serialization and bounded checks stay in a
dedicated executable.

The model defines:

- ceiling division for positive divisors;
- effective fixed and automatic timeout selection;
- duration-capped multiplication;
- the returned projection and shortfall predicate.

The proofs establish for arbitrary natural-number inputs under visible
positivity and representability premises:

1. the computed wave count covers every selected mutant;
2. when a wave exists, one fewer wave cannot cover the selection;
3. zero waves occurs exactly when zero mutants are selected;
4. projected capacity equals the capped mathematical product;
5. equality with remaining capacity is not a shortfall, while a strictly
   greater capacity is;
6. fixed timeouts are preserved and automatic timeouts follow the documented
   saturated rule.

Every potentially expensive theorem receives a local finite heartbeat limit.
No exhaustive evaluator is imported by the library root.

## Refutation sensitivity

The executable must reject these deliberately broken variants before it may
write or check a corpus:

- floor division instead of ceiling division, witnessed by a non-divisible
  selected/job pair;
- `projected_capacity >= remaining`, witnessed at exact equality;
- wrapping or uncapped multiplication, witnessed at the duration maximum;
- an automatic timeout that omits either the one-second increment or the
  five-second minimum.

Atomicity and uniqueness/idempotency are not applicable: the audited function
is pure, has no state transition, identity allocation, replay marker, or
partial mutation. Boundary and precedence sensitivity is applicable and is
covered by all four broken variants.

## Corpus and Rust adapter

Lean defines the cases once and emits deterministic JSON Lines. Cases cover:

- zero selection;
- divisible and non-divisible parallel selections;
- exact remaining-capacity equality and one-tick shortfall boundaries;
- fixed and automatic timeout branches;
- maximum-duration saturation;
- the largest practical wave-count conversion boundary needed to distinguish
  exact multiplication from saturation.

A new `hoimin-core` integration test parses each `strict` row, constructs the
same public inputs, invokes `project_top_budget`, and compares all stable
returned fields plus `is_shortfall()`. Parsing, unsupported platform widths,
construction failures, and unexpected exits are infrastructure errors, not
semantic mismatches. Generated expectations are never duplicated in Rust.

## Rust repair policy

If strict correspondence finds a mismatch, add the smallest direct Rust
regression test and observe it fail for the semantic reason recorded by the
corpus. Then make the minimal production change and rerun focused and workspace
verification. Lean is not weakened to match existing behavior.

If strict correspondence matches, do not make a speculative production
behavior change. The formalization, sensitivity checks, corpus, and adapter are
still the requested improvement; a report will explicitly state that no Rust
repair was necessary.

## Resource limits and verification

Run only one Lean command at a time. Each proof build, corpus operation, or
bounded check uses a 20-second external deadline, a 768 MiB RSS ceiling, and a
local theorem heartbeat limit of 100,000. The finite domain starts with the
listed semantic boundary cases and is not enlarged unless measured cost stays
flat and an uncovered claim requires it.

The current worktree baseline is:

- `cargo build --workspace`: pass;
- `cargo test -p hoimin-core --test budget_projection`: 7 passed;
- `lake -Kjobs=1 build`: pass, 38 jobs;
- resource-guard baseline: `infrastructure-error` because sandboxed process
  table observation failed; semantic Lean checks are not inferred from that
  failed monitor run.

Final verification includes the Lean build, proof consumer, sensitivity check,
corpus freshness, focused Rust adapter and projection tests, formatting,
clippy, workspace tests, and `git diff --check`.

## Deliverables

- Lean model, proof, case, and executable modules;
- versioned generated JSONL corpus;
- strict Rust correspondence adapter;
- regression test and Rust correction if a mismatch is confirmed;
- implementation plan and this design document;
- a self-contained audit report separating Lean guarantees, finite checks,
  Rust observations, resource measurements, and unresolved decisions.
