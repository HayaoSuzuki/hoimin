# Skip Interpolated-String Content Tokens Implementation Plan

> **For Codex:** Use Superpowers subagent-driven development and test-driven development to execute this plan task by task.

**Goal:** Prevent the Python analyzer from emitting operator and literal mutants for the literal-text portions of f-strings and t-strings, while preserving mutations inside interpolation expressions.

**Architecture:** Keep the existing source-ordered token traversal and candidate construction. Before raw source-text matching, reject only Ruff `TokenKind::FStringMiddle` and `TokenKind::TStringMiddle`. Start/end string tokens do not match the mutation table today, and expression tokens inside braces retain their ordinary kinds, so the narrow guard fixes the defect without changing expression behavior.

**Tech Stack:** Rust, `littrs-ruff-python-ast` 0.6.x, `littrs-ruff-python-parser` 0.6.x, Cargo tests.

---

## Task 1: Add the regression contract and filter middle-string tokens

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

### Step 1: Add a failing literal-content test

Add a focused analyzer unit test covering f-string literal segments whose complete token text is mutable:

```python
label = f"{start}-{end}"
path = f"{left}/{right}"
flag = f"True"
relation = f"in"
```

Assert that no candidate span points at the literal `-`, `/`, `True`, or `in` content. Prefer an exact empty-candidate assertion for this source.

Run:

```console
cargo test -p hoimin-cli analyzer::rust::rust_tests::skips_mutable_fstring_literal_content -- --exact
```

Expected: FAIL because the current raw-text matcher emits candidates from `FStringMiddle`.

### Step 2: Add an interpolation-expression boundary test

Add a companion test such as:

```python
value = f"literal-{left + right}-{enabled is not None}"
```

Assert that literal `-` content is ignored while the `+` and `is not` expression tokens still produce their expected candidates. This prevents an overly broad “skip all tokens inside f-strings” fix.

Run the focused test and record its behavior before implementation.

### Step 3: Implement the minimal token-kind guard

Import Ruff's public token kind:

```rust
use ruff_python_ast::token::TokenKind;
```

At the start of the token loop, after cancellation polling and before source slicing/raw-text matching, skip:

```rust
if matches!(
    token.kind(),
    TokenKind::FStringMiddle | TokenKind::TStringMiddle
) {
    continue;
}
```

Do not skip expression tokens between interpolation braces.

### Step 4: Prove RED becomes GREEN

Run both new focused tests. Expected: PASS.

Then run the analyzer unit module:

```console
cargo test -p hoimin-cli analyzer::rust::rust_tests
```

Expected: all analyzer tests pass.

### Step 5: Run full verification

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --features contracts
git diff --check
```

Expected: every command succeeds.

### Step 6: Commit

Stage only the analyzer implementation, regression tests, and this plan:

```console
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs docs/superpowers/plans/2026-07-31-skip-string-content-tokens.md
git commit -m "fix: skip interpolated string content tokens"
```

## Acceptance Checklist

- F-string literal text matching a mutation token emits no candidate.
- T-string literal text is guarded by the same token-kind rule.
- Operators and keywords inside interpolation expressions remain mutable.
- Candidate order, selection, profiles, and limits are otherwise unchanged.
- The complete workspace and contracts-enabled CLI suites pass.
