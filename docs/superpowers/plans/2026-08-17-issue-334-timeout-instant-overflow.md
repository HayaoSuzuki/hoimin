# Issue #334: Timeout deadline validation implementation plan

**Goal:** Reject every configured or derived timeout outside the supported
deadline range before execution can perform an overflowing `Instant` addition.

**Architecture:** Define one deterministic core ceiling and enforce it through
the shared normalized limit validator. Model the direct and derived boundaries
in Lean and compare its strict corpus with public Rust configuration behavior.

**Tech stack:** Rust, Tokio, serde, Lean 4, Cargo tests, hoimin mutation tests.

## Task 1: Add failing boundary tests

**Files:**

- Modify: `crates/hoimin-core/tests/target_policy.rs`
- Modify: `crates/hoimin-core/tests/plan_config.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

1. Cover the inclusive direct-timeout ceiling and one nanosecond above it for
   raw and normalized configurations.
2. Cover the accepted and rejected baseline edges for derived auto timeout.
3. Assert huge total and fixed mutant CLI values return code 2, identify the
   corresponding flag, and do not reach project execution.
4. Run focused tests and record the expected failures before implementation.

## Task 2: Implement the shared invariant

**Files:**

- Modify: `crates/hoimin-core/src/config.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`

1. Add the public 100-year `MAX_TIMEOUT` constant.
2. Validate every direct timeout as nonzero and at most the ceiling.
3. Validate the derived auto mutant timeout and attribute failure to
   `baseline_timeout`.
4. Make raw limit conversion call the shared normalized validator.
5. Keep `RunConfig` and `PlanConfig` validation on the same path.
6. Derive shutdown-grace deadlines with checked addition, retaining the
   original deadline when the grace is not representable.

## Task 3: Audit the boundary in Lean

**Files:**

- Add: `formal/HoiminOracle/HoiminOracle/TimeoutLimitModel.lean`
- Add: `formal/HoiminOracle/HoiminOracle/TimeoutLimitProofs.lean`
- Add: `formal/HoiminOracle/HoiminOracle/TimeoutLimitCases.lean`
- Add: `formal/HoiminOracle/TimeoutLimitAuditMain.lean`
- Add: `formal/HoiminOracle/corpus/timeout-limit.jsonl`
- Add: `crates/hoimin-core/tests/lean_timeout_limit_oracle.rs`
- Add: `docs/superpowers/reports/2026-08-17-issue-334-timeout-limit-audit.md`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

1. Model direct values, fixed mutant mode, and the derived auto mutant value.
2. Prove accepted configurations bound every effective timeout.
3. Add exact boundary cases and sensitivity to omitted derived validation.
4. Generate the corpus and compare every strict row with public Rust APIs.
5. Document the model-to-implementation correspondence and external
   `Instant` portability assumption.

## Task 4: Verify behavior and coverage

1. Run the Lean build, generator, corpus check, sensitivity checks, and Rust
   adapter.
2. Run focused core and CLI regressions.
3. Run focused mutation testing for configuration validation.
4. Run formatting, clippy, the Rust workspace, Python tests, and wheel smoke.
5. Inspect the complete diff and request independent review.

## Task 5: Deliver and clean up

1. Commit the design, plan, formal audit, implementation, and tests.
2. Push the issue branch and open a PR that closes #334.
3. Merge only after review and all CI jobs succeed.
4. Fast-forward local main, rerun the focused regressions, and remove the
   remote/local branch and issue worktree.
