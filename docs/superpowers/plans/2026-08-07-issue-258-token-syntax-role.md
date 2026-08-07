# Issue 258 Token Syntax-Role Gating Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent raw-token operators from mutating Python grammar tokens while preserving every currently supported expression and statement operator mutation.

**Architecture:** Extend `AstFacts` with a set of parser-token start offsets proven to occupy supported AST operator roles. Populate it during the existing AST visitor, then require membership in that set before the raw-token pass constructs a candidate; keep existing span logic for composite comparisons, unary `not`, and source trivia.

**Tech Stack:** Rust workspace, Ruff Python AST/parser 0.6.2, Rust unit and integration tests, Python unittest contract tests.

## Global Constraints

- Candidate operator IDs, byte spans, ordering, filters, profiles, and report schemas remain unchanged for valid expression operators.
- Production analysis does not reparse source once per candidate.
- Unsupported or ambiguous grammar roles are skipped conservatively without a new diagnostic.
- Loop/comprehension membership syntax, imports/unpacking, and match OR patterns must not emit unrelated token mutations.
- Existing annotation bitwise exclusions remain effective.
- Behavior-changing commits must not use `[skip ci]`; only pure documentation commits may use it.

## File Map

- `crates/hoimin-cli/src/analyzer/rust.rs`: collect AST-proven operator token starts and gate the raw-token mutation pass.
- `crates/hoimin-cli/src/analyzer/rust_tests.rs`: regression coverage for grammar roles, valid operators, exact spans, and parseability.
- `docs/development.md`: document the AST eligibility invariant for token operators.
- `docs/superpowers/specs/2026-08-07-issue-258-token-syntax-role-design.md`: approved design committed before this plan.

---

### Task 1: Exclude unsupported Python grammar roles

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

**Interfaces:**
- Adds `AstFacts::operator_token_starts: HashSet<usize>`.
- Adds `AstFacts::is_operator_token(start: usize) -> bool`.
- Adds a private range helper that records parser-token starts only when their text is in an explicit spelling set for an AST node category.

- [ ] **Step 1: Write the failing grammar-role regression test**

Add `grammar_tokens_do_not_emit_expression_operator_mutations`. Use one parseable fixture containing a normal loop, a comprehension, star import, positional and keyword unpacking, and a match OR pattern. Include nearby valid membership, multiply, and bitwise expressions. Assert the invalid-role `(original, operator)` pairs are absent at their known lines while valid-role candidates remain.

```rust
let source = concat!(
    "from package import *\n",
    "for item in items:\n    pass\n",
    "values = [item for item in items]\n",
    "result = call(*args, **kwargs)\n",
    "match value:\n    case left | right:\n        pass\n",
    "member = item in items\n",
    "product = left * right\n",
    "union = left | right\n",
);
let output = analyze(source);
assert!(!output.candidates.iter().any(|candidate| {
    candidate.line <= 8
        && matches!(candidate.operator.as_str(), "membership" | "binary_mul_div" | "bitwise_and_or")
}));
assert!(output.candidates.iter().any(|candidate| candidate.line == 9 && candidate.operator == "membership"));
```

- [ ] **Step 2: Run the regression test and verify RED**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::grammar_tokens_do_not_emit_expression_operator_mutations -- --exact`

Expected: FAIL because the current token pass emits at least the loop/comprehension `membership`, star `binary_mul_div`, and pattern `bitwise_and_or` candidates.

- [ ] **Step 3: Implement AST-derived token eligibility**

Populate `operator_token_starts` in `Visitor::visit_stmt` for `Stmt::AugAssign`, `Stmt::Break`, and `Stmt::Continue`, and in `Visitor::visit_expr` for `Expr::Compare`, `Expr::BoolOp`, `Expr::BinOp`, `Expr::UnaryOp`, and `Expr::BooleanLiteral`. Use explicit spelling slices per node category and parser tokens bounded by each node range.

Before the token pass evaluates composite or simple replacements, add:

```rust
if !facts.is_operator_token(start) {
    continue;
}
```

The range recorder must inspect the original token text and must never infer eligibility from punctuation alone outside the AST node category.

- [ ] **Step 4: Run the focused regression test and verify GREEN**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::grammar_tokens_do_not_emit_expression_operator_mutations -- --exact`

Expected: PASS with invalid roles absent and valid neighboring expressions present.

- [ ] **Step 5: Commit the behavioral fix**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "fix: gate token mutations by syntax role"
```

---

### Task 2: Prove valid operator coverage and parseability

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs` only if a valid AST role exposed by the test is missing

**Interfaces:**
- Uses the existing `apply_candidate_and_reparse(source, candidate) -> String` test helper.
- Adds no public production interface.

- [ ] **Step 1: Write the valid-role completeness test**

Add `token_operator_candidates_cover_supported_ast_roles_and_reparse`. The fixture must include equality/order/chained comparisons, `in`, `not in`, `is`, `is not`, `and`, `or`, binary and unary signs, multiply/divide/floor/modulo, bitwise and/OR/shifts, `+=`, `-=`, `not`, boolean literals, `break`, and `continue`. Assert a hand-written set of expected `(original, replacement, operator)` triples, then apply and reparse every token-family candidate.

```rust
for candidate in output.candidates.iter().filter(|candidate| {
    TOKEN_OPERATOR_NAMES.contains(&candidate.operator.as_str())
}) {
    apply_candidate_and_reparse(source, candidate);
}
```

Keep the expected list literal in the test rather than deriving it from the production replacement table.

- [ ] **Step 2: Run the completeness test and verify RED if a role is missing**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::token_operator_candidates_cover_supported_ast_roles_and_reparse -- --exact`

Expected before any necessary completion: FAIL naming the omitted valid role. If the gating implementation already covers all listed roles, temporarily remove one AST visitor branch, verify that the test fails for that role, restore it, and rerun; retain only the final production code.

- [ ] **Step 3: Complete the minimal AST role mapping**

Add only the missing explicit AST category or spelling revealed by Step 2. Do not accept tokens through a source-wide fallback. Composite `not in` and `is not` remain represented by their first token start so the existing multi-token replacement spans are unchanged.

- [ ] **Step 4: Run analyzer tests and verify GREEN**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -- --show-output`

Expected: all analyzer unit tests pass, including annotation exclusions, source ordering, filters, cancellation, and the new parseability invariant.

- [ ] **Step 5: Commit completeness tests and any minimal mapping fix**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "test: cover syntax-directed token mutations"
```

---

### Task 3: Document and verify the analyzer invariant

**Files:**
- Modify: `docs/development.md`

**Interfaces:**
- Documents that raw-token replacements require an AST-proven operator role and that parseability is enforced by regression tests rather than per-candidate production parsing.

- [ ] **Step 1: Update development documentation**

In the Rust analyzer section, describe the `AstFacts` token-start allowlist, the conservative behavior for ambiguous grammar tokens, and the test-only apply-and-reparse invariant. State that annotation exclusions and existing selection/order behavior are unchanged.

- [ ] **Step 2: Run repository formatting and static checks**

Run: `cargo fmt --all -- --check`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: both commands exit 0 with no warnings.

- [ ] **Step 3: Run the full verification suite**

Run: `cargo test --workspace`

Run: `python3 -m unittest discover -s tests -p 'test_*.py'`

Expected: all non-platform-skipped Rust and Python tests pass.

- [ ] **Step 4: Check the final diff**

Run: `git diff --check origin/main...HEAD`

Run: `git status --short`

Expected: no whitespace errors and only the intended documentation edit remains uncommitted.

- [ ] **Step 5: Commit development documentation**

```bash
git add docs/development.md
git commit -m "docs: explain syntax-directed token mutations [skip ci]"
```

The final PR references and closes #258, includes the already committed design
and plan, and must pass the repository CI before squash merge.
