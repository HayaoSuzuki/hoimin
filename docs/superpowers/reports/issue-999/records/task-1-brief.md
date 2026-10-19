### Task 1: Extract the real production arithmetic and preserve integration

**Files:**
- Create: `crates/hoimin-budget-math/Cargo.toml`, `src/lib.rs`, `src/duration.rs`, `src/projection.rs`, `tests/arithmetic.rs`, `tests/serialization.rs` under that crate.
- Modify: root `Cargo.toml`, `Cargo.lock`, `fuzz/Cargo.lock`, `crates/hoimin-core/Cargo.toml`, `crates/hoimin-core/src/{lib,config,machine}.rs`; remove `crates/hoimin-core/src/budget_projection.rs` after migration.
- Modify as integration requires: existing `crates/hoimin-core/tests/{budget_projection,config,machine,lean_top_budget_projection_oracle}.rs`, `tools/{sbom,release}.py`, `tests/{test_sdist,test_sbom,test_release_version_contracts,sbom_fixtures}.py`, `pyproject.toml`.
- Create/update durable record: `docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md`.

**Interfaces:**
- Produces `NonZeroDuration::new(Duration) -> Option<Self>`, `NonZeroDuration::get(self) -> Duration`, `MutantTimeout::{Auto, Fixed(NonZeroDuration)}`, `auto_mutant_timeout(Duration) -> Duration` in `hoimin-budget-math`.
- Produces `project_top_budget(selected: usize, jobs: NonZeroUsize, planned_total_timeout: Duration, baseline: Duration, mutant_timeout: MutantTimeout, remaining: Duration) -> TopBudgetProjection`, same public result fields and `is_shortfall(&self) -> bool`.
- `serde` is an optional additive feature for derives/manual deserialization only; verification will compile without it. It must not change arithmetic, type shapes, constructors, or branches. Core enables it and maps `None` into existing named `ConfigError::InvalidLimit` at conversion sites. Callers migrate directly to the new owner; no compatibility function forwarding layer.

- [ ] Run baseline focused core projection/config/machine/Lean oracle tests before edits; record command and results. Use the shared target and no redundant separate build.
- [ ] Add failing new-crate API tests: zero duration returns `None`; positive boundary values round-trip; serialization rejects zero and keeps current format; manually constructed projection uses only capacity/remaining for shortfall.
- [ ] Add property tests with independent bounded mathematical expectations: waves equals `selected / jobs + (selected % jobs != 0)`; auto equals clamped `2 * baseline_nanos + 1_000_000_000`; product uses division threshold before multiplication to avoid oracle overflow. Include zero selected, usize maximum, Duration maximum, >u32 waves, saturation at multiplication and addition, equality shortfall. Each property checks one behavior per execution; use named parameterized cases for fixed boundaries.
- [ ] Extract production types/calculation without behavior change; migrate all callers and configuration/Serde integration. Preserve existing config error text and serialized representation. Update crate graph, locks, release/SBOM/sdist assumptions in the same commit.
- [ ] Run focused new and existing tests, `cargo check --workspace --all-features --locked`, relevant Python packaging/release/SBOM tests, formatting and focused lints. Record five implementation and five testing self-reviews, findings and fixes, and commit.


## Global Constraints

- Work only in `.worktrees/issue-999-creusot-budget-proofs` on `issue-999-creusot-budget-proofs`, starting at `b65f62834b35a9ff85795b041e9687b21e5c51ba`. Preserve other worktrees and untracked personal files.
- Product and verification use the same implementation and value types. No compatibility adapter, proof-only arithmetic, target-specific substitute, hoimin-authored `trusted`, external specification, or `assume` hole.
- Cover all public typed inputs: all `usize`, positive `NonZeroUsize`, all `Duration`, and positive fixed durations. Preserve observable results, serialized forms, config error behavior, and warning-only projection semantics.
- Required proof targets: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, generated and proved separately, sequentially, on Linux.
- Per solver process: 30 seconds, 2 GiB; maximum 2 concurrent solvers. Entire CI job, including cold environment setup: 30 minutes. Measure cold/warm duration, memory and disk. Escalate measured insufficiency; never silently relax caps or scope.
- Pin Creusot v0.14.0, nightly-2026-08-03, OCaml 5.3.0, Why3 `05bf4ddf55cb3042f34c3bc0ed3aff56a226978a`, Why3find `0f054b93c86ac7ba20bca7df5d1563a9a4434a30`, Z3 5.1.0, CVC5 1.4.1; verify artifact hashes and actual tools/configuration.
- Regenerate from current source; require nonempty obligations and named required functions/contracts; prohibit successful-result cache reuse. Final unproved obligations, missing tools, timeouts, and unintended skips fail CI.
- Stable Rust 1.99.0 and all-features checks remain supported; normal builds do not require proof tools. Preserve Lean oracle correspondence and runtime contracts.
- AAA, one case per test execution, parameterized finite cases and property tests for arithmetic domains. Record at least five self-review passes for requirements, design, implementation, and testing in durable review notes.
- One heavy build at a time, `CARGO_BUILD_JOBS=2`, task-owned shared `CARGO_TARGET_DIR=/tmp/hoimin-issue999-target`. Check disk before/after heavy work; clean owned obsolete artifacts only. Do not spawn nested agents.
- Complete issue update, implementation, actual verification, documentation, and PR. Do not merge, release, or publish a container image.


Approved spec: docs/superpowers/specs/2026-10-19-issue-999-creusot-adoption-design.md

## Plan correction before Task 1 review
Use the smallest-covering-wave characterization for the waves property, with u128 products. The capacity oracle uses widened additive ceiling (selected as u128 + jobs as u128 - 1) / jobs as u128. Do not repeat the production quotient/remainder expression in test expectations. Preserve all test-domain and API requirements.
