# Creusot production budget proofs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Prove the actual production budget projection and auto timeout implementation on all three release targets and require fresh proofs in PR CI.

**Architecture:** Move the arithmetic and its value types into the production `hoimin-budget-math` crate. Add contracts to that implementation, a pinned Linux verification environment, and a fail-closed proof command consumed by mandatory CI. Keep configuration, serialization integration, and operating-system effects outside the arithmetic's proof claims.

**Tech Stack:** Rust 1.99.0, Creusot v0.14.0, nightly-2026-08-03, OCaml 5.3.0, Why3/Why3find, Z3 5.1.0, CVC5 1.4.1, Python tooling, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-19-issue-999-creusot-adoption-design.md` (approved; read this with the task brief).

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

## Review Focus

1. Huge typed inputs exceed configured limits: no overflow or premature saturation (Task 1 property tests; Task 3 full-domain contracts).
2. Hand-built projection results and deserialized positive durations: no invented provenance assumption or zero accepted (Task 1 tests; Task 3 type contracts).
3. A tool missing from configuration or an old proof artifact: fail explicitly, never silently narrow coverage (Task 2 preflight tests; Task 4 fresh-run tests).
4. Cold setup and cross-target proof differ from warm/native runs: record real measurements and require each target (Tasks 2–4 execution and gate tests).
5. New crate disappears from source distributions, release versioning, or SBOM: generated products include the real dependency (Tasks 1 and 5 packaging tests).

---

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

- [x] Run baseline focused core projection/config/machine/Lean oracle tests before edits; record command and results. Use the shared target and no redundant separate build.
- [x] Add failing new-crate API tests: zero duration returns `None`; positive boundary values round-trip; serialization rejects zero and keeps current format; manually constructed projection uses only capacity/remaining for shortfall.
- [x] Add property tests with independent bounded mathematical expectations: waves is the smallest covering count (`selected <= waves * jobs`, and for positive waves `(waves - 1) * jobs < selected`, with widened u128 products); use widened additive ceiling `(selected as u128 + jobs as u128 - 1) / jobs as u128` for the independent capacity oracle; auto equals clamped `2 * baseline_nanos + 1_000_000_000`; product uses division threshold before multiplication to avoid oracle overflow. Include zero selected, usize maximum, Duration maximum, >u32 waves, saturation at multiplication and addition, equality shortfall. Each property checks one behavior per execution; use named parameterized cases for fixed boundaries.
- [x] Extract production types/calculation without behavior change; migrate all callers and configuration/Serde integration. Preserve existing config error text and serialized representation. Update crate graph, locks, release/SBOM/sdist assumptions in the same commit.
- [x] Run focused new and existing tests, `cargo check --workspace --all-features --locked`, relevant Python packaging/release/SBOM tests, formatting and focused lints. Record five implementation and five testing self-reviews, findings and fixes, and commit.

### Task 2: Build and measure the pinned Linux verification environment

**Files:**
- Create: `formal/creusot/toolchain.json`, `formal/creusot/Dockerfile`, `tools/creusot_environment.py`, `tests/test_creusot_environment.py`.
- Update: durable review record; `formal/creusot/README.md` for reproducible environment commands and measured prerequisites.

**Interfaces:**
- Produces manifest with exact source revisions, download hashes, executable versions and prover configuration for approved tools; no mutable unverified downloads.
- Produces `python tools/creusot_environment.py check --manifest formal/creusot/toolchain.json --project-root PROJECT`, exit zero only if all actual executables, installed configuration and selected project-local `why3find.json` match; actionable nonzero diagnostics otherwise. `check-installation` preserves image self-check before a project exists and cannot substitute for proof preflight.
- Produces Linux x86_64 Docker build/run commands and a native Linux ARM64 development variant using the same source pins and proof targets. CI may install directly from the same pinned recipe if that avoids unnecessary container build overhead. Local ARM64 measurements do not replace native Linux x86_64 cold/warm acceptance in Task 4. Do not publish an image.

- [x] Read pinned upstream installation/prover CLI sources, resolve full revisions and checksums, and record their authoritative URLs. No unsupported guesses about CLI flags.
- [x] Write parameterized failing tests for a missing solver, wrong version, wrong configuration and malformed manifest; successful version printing with mismatched content must fail. Use controlled executable fixtures without asserting implementation-internal command ordering.
- [x] Implement isolated install recipe and strict preflight. Download only pinned sources/artifacts with hash verification, bound build parallelism, and arrange exact Rust target standard libraries. Keep installation inside task-owned container/cache paths.
- [x] Begin local environment setup from cold with a 30-minute wall-clock deadline per attempt, recording elapsed time, peak memory where measurable, and disk delta. During repair, reuse successful tool layers and distinguish failed cold attempts from the final working warm setup. Check the actual toolchain and an ordinary crate verification invocation. Task 4 must measure the final recipe from cold and warm on native Linux x86_64, including setup and all proofs within the accepted job cap; local cached success does not satisfy that gate. Stop and report measured cap violations rather than increasing limits; diagnose target support with real compiler invocation.
- [x] Run focused Python tests/lint/types (and relevant hoimin mutation testing per skill), record five implementation and five testing self-reviews and measured limitations, then commit. Task 3 owns actual arithmetic proofs, not this environment smoke check.

### Task 3: Prove the complete production arithmetic on three targets

**Files:**
- Modify: `crates/hoimin-budget-math/{Cargo.toml,src/lib.rs,src/duration.rs,src/projection.rs}`, root/fuzz lockfiles as needed.
- Create: `formal/creusot/why3find.json`, `formal/creusot/proofs/` strategies as appropriate, `formal/creusot/obligations.json` required function/contract manifest.
- Update: `formal/creusot/README.md` and durable review record.

**Interfaces:**
- Consumes Task 1 production API and Task 2 pinned environment.
- Produces contracts on actual production constructors/accessors, auto timeout, waves, saturation/reconstruction, projection and shortfall covering all eight spec clauses.
- Produces reproducible generation/prove/replay commands, actual generated function/goal identifiers, and no-cache proof strategies suitable for Task 4's runner.

- [x] Establish native Linux and both cross-target generation with current production crate; diagnose unsupported operations from the pinned Creusot source. Use equivalent provable production operations where needed, never verification-only alternate behavior.
- [x] Add contracts and proof helpers for all eight mathematical spec clauses. Deny `creusot::trusted_code` in owned proof code; model Duration in integer nanoseconds and encode positive duration invariants. Do not assume a public projection was produced by its constructor.
- [x] Generate fresh obligations and prove every goal for each required target sequentially, with solver 30 seconds/2048 MiB and maximum two concurrent processes. Record goal counts, required functions, elapsed time and logs, including final unproved goals if any.
- [x] Perform sensitivity checks by temporarily changing production ceil, saturation, auto timeout and equality comparison one at a time; each must compile and produce an unproved relevant goal. Restore source after each and retain diagnostic evidence; compile errors are not proof-sensitivity evidence.
- [x] Recheck stable Rust/all-features and focused arithmetic/Lean/runtime tests after proof-driven refactors. Record five implementation and five proof/test self-reviews, trust boundaries, repair effort and actual resource measurements; commit only correct production code and useful proof artifacts.

### Task 4: Enforce fresh complete verification in mandatory CI

**Files:**
- Create: `tools/verify_creusot.py`, `tests/test_verify_creusot.py`.
- Modify: `.github/workflows/ci.yml`, `tools/ci_selection.py`, `tests/{test_ci_selection,test_ci_selection_workflow}.py`, `formal/creusot/{README.md,obligations.json}` as needed.
- Update: durable review record.

**Interfaces:**
- Consumes environment preflight, required function/goal manifest, and proven generation/replay commands from Tasks 2–3.
- Produces `python tools/verify_creusot.py --output <directory>`: preflight, freshly generate each target, check function/contract coverage and nonempty goals, replay every goal without success cache, fail closed; emit JSON measurements/diagnostics and logs into the given directory.
- Produces mandatory CI job `creusot` with `timeout-minutes: 30`, including environment preparation, selected for source/spec/tools/dependency changes and required by CI aggregation when selected.

- [x] Write parameterized failing runner tests: missing required function/contract, empty goals, old artifacts, one target unresolved, missing/mismatched tool, solver timeout recovered by a successful alternative, overall timeout, and success only after all three fresh target results. Test observable failure and subprocess boundaries with fixtures, not an alternate fake proof engine.
- [x] Implement fail-closed orchestration using actual Task 3 commands and structured manifests; clear owned generated output before each generation, prohibit prove-only stale success and enforce caps/process counts. Identify required coverage by actual named contracts/functions, not total counts alone.
- [x] Add CI selection/aggregate tests for changed math crate, proof specs/strategies, tool pins and runner, plus failed/cancelled/unintentionally skipped proof job. Integrate installation/cache without solver-success cache; upload diagnostics on failure.
- [x] Execute full runner cold/warm on Linux and all three targets with actual tools; record elapsed time/memory/disk. Run focused tests, lint/types and relevant mutation checks, five implementation and five testing self-reviews, then commit.

### Task 5: Complete distribution, documentation and branch acceptance

**Files:**
- Update: `formal/creusot/README.md`, approved spec status, durable review record, relevant `docs/knowledge` OKF contracts/catalog and release/proof documentation.
- Adjust packaging files/tests from Task 1 only if full acceptance exposes a concrete gap.

**Interfaces:**
- Consumes production crate, fresh proof runner, mandatory CI, and measurement evidence.
- Produces complete reproducible maintenance guidance, verified distribution and review record ready for PR.

- [x] Document actual guarantees, exclusions/trust boundary, commands to regenerate/replay and repair proofs, pinned updates, cold/warm resource measurements, additional code/spec size and sensitivity results. Never claim Ledger/OS timing or unsupported targets are proved.
- [x] Run repository-required workspace tests, all-features clippy/formatting, Python quality checks, Lean oracle/proof correspondence and runtime contracts. Reuse passing focused checks where unchanged; fix actual failures without repeated full-suite churn.
- [x] Verify root/fuzz lock consistency and dependency policy, generated SBOM, sdist includes the math crate and proof annotations' normal-build dependencies, build from sdist and execute wheel. Record results with exact source commit.
- [x] Validate relevant OKF artifacts, record five final implementation/testing self-reviews, archive task/review evidence durably, and commit. Controller then obtains whole-branch review, addresses findings, pushes the issue branch and creates PR closing #999 only when all accepted conditions have evidence.

## Plan self-review (five passes)

1. Coverage: all Q1–Q7 and eight arithmetic obligations map to Tasks 1–4; distribution/docs/PR map to Tasks 1 and 5. Actual cold setup remains a measured gate.
2. Steps: each task has failing behavioral tests or actual proof sensitivity, a concrete deliverable, verification and commit. No task ends at an unexecuted environment proposal.
3. Interfaces: Task 1 owns arithmetic API, Task 2 environment manifest/preflight, Task 3 goal identifiers/strategies, Task 4 runner/CI. The optional Serde feature does not alter proved arithmetic.
4. Failure classes: all five Review Focus items have explicit owners/tests; manually constructed output and missing solver are included.
5. Proportion/resources: five deliverables, sequential heavy builds, scoped reviews, no parallel tool installations or unapproved cap increases. Existing approved spec governs remaining implementation choices.


## Integration status (2026-10-19)

Tasks 1–4 are implemented and independently approved; original reports, review
rounds, Rulings and execution evidence are retained in the
[durable archive](../reports/issue-999/README.md). Task 5's current-code checkpoint
is `5033e1bb`; local source-distribution/wheel/SBOM and dependency acceptance
passed. Native current-source full CI and warm/cold proofs passed. Windows-only
harness scheduling at `70ea57b2` did not resolve the readiness failures. Diagnostic
checkpoint `dd2674dd` passed those cases but exposed a separate lifecycle adapter
exit guard; its contract-derived repair at `312be2c4` passed all nine native
non-Linux jobs in `39202749924`. Scoped review found no new breakage; its native
acceptance gate is now closed. Final whole-branch review remains pending. Use the
[integration record](../reports/2026-10-19-issue-999-creusot-adoption.md) for their
actual disposition. Whole-branch review and PR publication are the next controller steps.
