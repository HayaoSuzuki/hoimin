# Issue 259 Complete Plan Ranking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure every mutation operator has exactly one deterministic ranking reason and prevent `plan` from returning a manifest that `verify` rejects.

**Architecture:** Make the canonical `MutationOperator` classification exhaustive, adding explicit exception-handling and behavioral ranking categories while assigning bitwise operators to arithmetic. Advance the rule version, validate generated manifests through the existing header validator, and exercise all new families through plan-to-verify integration tests.

**Tech Stack:** Rust workspace (`hoimin-core`, `hoimin-cli`), Tokio integration tests, Serde JSON manifests, Python unittest contract tests.

## Global Constraints

- Every one of the 43 current `MutationOperator` variants maps to exactly one operator ranking reason.
- `exception_handling` scores 90, `behavioral` scores 80, and bitwise operators use `arithmetic` at 70.
- Existing category scores remain unchanged: high-value control 100 and type annotation 50.
- `RANKING_RULE_VERSION` changes from 2 to 3; `PLAN_SCHEMA_VERSION` remains 2.
- Selector bonuses, candidate tie-breaking, operator IDs, defaults, and report schemas remain unchanged.
- Generated manifests pass the same `validate_header` checks used at the verify boundary before `create` returns.
- Behavior-changing commits do not use `[skip ci]`; only pure documentation commits may use it.

## File Map

- `crates/hoimin-cli/src/plan/ranking.rs`: reason codes, fixed scores, exhaustive operator classification, and rule version.
- `crates/hoimin-cli/src/plan/ranking_tests.rs`: all-operator category and ranking validation tests.
- `crates/hoimin-cli/src/plan.rs`: validate the completed manifest before returning it.
- `crates/hoimin-cli/tests/plan.rs`: JSON rule-version assertions and plan-to-verify family integration.
- `docs/development.md`: ranking category/version and output-boundary invariant.
- `docs/superpowers/specs/2026-08-07-issue-259-ranking-reasons-design.md`: approved design committed before this plan.

---

### Task 1: Classify every mutation operator exhaustively

**Files:**
- Modify: `crates/hoimin-cli/src/plan/ranking.rs`
- Modify: `crates/hoimin-cli/src/plan/ranking_tests.rs`

**Interfaces:**
- Adds serialized `RankingReasonCode::ExceptionHandling` and `RankingReasonCode::Behavioral`.
- Changes `RANKING_RULE_VERSION` to `3`.
- Keeps `operator_reason(operator: &str) -> Option<RankingReasonCode>` but makes the `MutationOperator` match exhaustive.

- [ ] **Step 1: Write the failing all-operator classification test**

Replace the three-representative test with a literal table covering all 43 canonical IDs and expected reasons. For each row, rank one candidate, assert exactly the expected reason, and call `validate_ranking`.

```rust
for (operator, expected) in [
    ("compare_eq_ne", reason(RankingReasonCode::HighValueControl, 100)),
    ("collection_any_all", reason(RankingReasonCode::Behavioral, 80)),
    ("bitwise_and_or", reason(RankingReasonCode::Arithmetic, 70)),
    ("exception_type_pair", reason(RankingReasonCode::ExceptionHandling, 90)),
    ("type_nullable_remove", reason(RankingReasonCode::TypeAnnotation, 50)),
    // Include every remaining canonical ID explicitly.
] {
    let ranked = rank_candidates(&Selection::default(), &[], vec![candidate(
        "candidate", "src/calc.py", 1, 0, operator, None,
    )]);
    assert_eq!(ranked[0].ranking_reasons, [expected], "{operator}");
    validate_ranking(&ranked).unwrap_or_else(|error| panic!("{operator}: {error}"));
}
```

Also collect `MutationOperatorSelection::valid_names().into_iter().filter_map(MutationOperator::from_name)` and assert that the table has the same 43 distinct operators, so the literal table cannot silently omit a variant.

- [ ] **Step 2: Run the ranking test and verify RED**

Run: `cargo test -p hoimin-cli --lib plan::ranking_tests::ranking_assigns_every_operator_to_its_fixed_category -- --exact`

Expected: FAIL to compile because the new reason variants do not exist, or fail because newer operators have no reason.

- [ ] **Step 3: Implement reason codes, scores, version, and exhaustive mapping**

Add the new enum variants after `HighValueControl` so reason ordering remains selector reasons followed by descending operator-category score. Update `fixed_score` and `is_operator_reason`. Change the operator match to return:

```rust
let code = match operator {
    // existing control variants => HighValueControl
    // all exception variants => ExceptionHandling
    // all collection and structure variants => Behavioral
    // arithmetic and bitwise variants => Arithmetic
    // all type variants => TypeAnnotation
};
Some(code)
```

Do not retain a wildcard arm. Unknown strings still return `None` at `MutationOperator::from_name`.

- [ ] **Step 4: Run ranking tests and verify GREEN**

Run: `cargo test -p hoimin-cli --lib plan::ranking_tests -- --show-output`

Expected: all ranking tests pass, including tamper and deterministic-order validation.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/plan/ranking.rs crates/hoimin-cli/src/plan/ranking_tests.rs
git commit -m "fix: rank every mutation operator"
```

---

### Task 2: Validate generated plans and prove plan-to-verify interoperability

**Files:**
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- `create(config: RunConfig) -> Result<PlanOutput, PlanError>` calls `validate_header(&manifest)?` before returning.
- Plan JSON continues to use schema version 2 and now reports ranking rule version 3.

- [ ] **Step 1: Write failing integration tests**

Update the existing plan JSON assertion to expect ranking rule version 3. Add `plans_for_new_operator_families_pass_verify`, generating one plan for each selector/family fixture:

- collection: `collection_ops` on a file containing `any(items)`;
- structural: `structure_ops` on a file containing `items.append(value)`;
- bitwise: `bitwise_ops` on a file containing `left | right`;
- exception: `exception_ops` on a file containing a simple `except ValueError` handler.

Persist each `PlanOutput.manifest`, choose its first candidate ID, and pass it to `prepare_verify`. Assert verification succeeds and preserves that candidate.

- [ ] **Step 2: Run the integration test and verify RED**

Run: `cargo test -p hoimin-cli --test plan plans_for_new_operator_families_pass_verify -- --exact`

Expected before Task 1 is present: FAIL because affected candidates have no operator reason. With Task 1 already present, verify the test fails first by temporarily removing one affected classification arm, run the test, then restore the arm before implementation continues.

- [ ] **Step 3: Validate manifest at the create boundary**

Immediately after constructing `PlanManifest`, add:

```rust
validate_header(&manifest)?;
```

This must occur before `Ok(PlanOutput { ... })`. Reuse the existing validator rather than duplicating ranking checks.

- [ ] **Step 4: Run plan tests and verify GREEN**

Run: `cargo test -p hoimin-cli --test plan -- --show-output`

Run: `cargo test -p hoimin-cli --lib plan::ranking_tests -- --show-output`

Expected: all plan integration and ranking unit tests pass with rule version 3.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/tests/plan.rs
git commit -m "fix: validate generated plan rankings"
```

---

### Task 3: Document ranking semantics and run merge verification

**Files:**
- Modify: `docs/development.md`

**Interfaces:**
- Documents rule version 3, fixed category scores, exhaustive classification, and create-boundary validation.

- [ ] **Step 1: Update development documentation**

In the plan/ranking section, list the five operator categories and scores, state that all canonical operators are exhaustively classified, and explain that `create` validates its own completed manifest before returning it. Keep schema version and rule version distinct.

- [ ] **Step 2: Run formatting and static analysis**

Run: `cargo fmt --all -- --check`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: exit 0 with no warnings.

- [ ] **Step 3: Run full tests**

Run: `cargo test --workspace`

Run: `.venv/bin/python -m unittest discover -s tests -p 'test_*.py'`

Expected: all non-platform-skipped tests pass.

- [ ] **Step 4: Verify the final diff and worktree**

Run: `git diff --check origin/main...HEAD`

Run: `git status --short`

Expected: no whitespace errors and only `docs/development.md` plus the test-environment `.venv` symlink are uncommitted; `.venv` is never staged.

- [ ] **Step 5: Commit documentation**

```bash
git add docs/development.md
git commit -m "docs: explain complete plan ranking [skip ci]"
```

The PR closes #259 and includes the design and implementation plan. Merge only
after the repository CI passes, with the hard-cgroup job allowed to remain at
its configured skip state.
