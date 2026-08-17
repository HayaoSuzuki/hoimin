# Issue 323 Split/Rsplit Maxsplit Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Emit `collection_string_split_rsplit` candidates only when the call supplies `maxsplit`.

**Architecture:** Add one split-specific AST argument predicate that reuses the current same-contract safety check and then detects a second positional argument or a named `maxsplit` keyword. Keep candidate spans, replacements, operator identity, ordering, and the other same-contract methods unchanged.

**Tech Stack:** Rust 2024, littrs Ruff Python AST 0.6.2, Cargo tests, Python `unittest`, no new dependencies.

## Global Constraints

- Preserve `min`/`max` and `startswith`/`endswith` candidate behavior.
- Continue rejecting starred positional arguments and unnamed `**kwargs` for split-direction candidates.
- Keep the analyzer syntax-directed; do not evaluate `maxsplit` values or infer receiver types.
- Add no dependency.

---

### Task 1: Gate split-direction candidates on explicit maxsplit

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs:1780-1841,2260-2282`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs:1816-1990`
- Modify: `README.md:179`
- Modify: `docs/development.md:181-202`

**Interfaces:**
- Consumes: `ExprCall.arguments.args`, `ExprCall.arguments.keywords`, and `has_supported_same_contract_arguments(call)`.
- Produces: `has_supported_split_rsplit_arguments(call: &ExprCall) -> bool` and unchanged `MutationOperator::CollectionStringSplitRsplit` candidates for accepted calls.

- [ ] **Step 1: Write the failing regression test**

Add this test to `crates/hoimin-cli/src/analyzer/rust_tests.rs` after the broad collection candidate test:

```rust
#[test]
fn split_rsplit_candidates_require_explicit_maxsplit() {
    let source = concat!(
        "text.split()\n",
        "text.split(',')\n",
        "text.rsplit()\n",
        "text.rsplit(',')\n",
        "text.split(',', 1)\n",
        "text.rsplit(',', 1)\n",
        "text.split(',', maxsplit=1)\n",
        "text.rsplit(maxsplit=1)\n",
    );
    let output = analyze(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_string_split_rsplit")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            ("split", "rsplit"),
            ("rsplit", "split"),
            ("split", "rsplit"),
            ("rsplit", "split"),
        ]
    );
}
```

- [ ] **Step 2: Run the regression and verify RED**

Run:

```bash
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::split_rsplit_candidates_require_explicit_maxsplit -- --exact
```

Expected: FAIL because the analyzer returns eight split-direction candidates, including the four calls without `maxsplit`.

- [ ] **Step 3: Add the split-specific predicate and use it**

Add this helper beside `has_supported_same_contract_arguments`:

```rust
fn has_supported_split_rsplit_arguments(call: &ExprCall) -> bool {
    has_supported_same_contract_arguments(call)
        && (call.arguments.args.len() >= 2
            || call.arguments.keywords.iter().any(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|argument| argument.as_str() == "maxsplit")
            }))
}
```

Change the `split`/`rsplit` match guard to
`has_supported_split_rsplit_arguments(call)`. In the broad positive fixture,
replace `text.rsplit()` with `text.rsplit(None, 1)` so both replacement
directions retain explicit positive coverage.

- [ ] **Step 4: Verify GREEN and the existing positive fixture**

Run:

```bash
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::split_rsplit_candidates_require_explicit_maxsplit -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::collection_calls_and_literals_emit_exact_parseable_candidates -- --exact
```

Expected: both tests pass. The broad fixture reparses each replacement.

- [ ] **Step 5: Update the operator contract documentation**

Change the README operator table entry from `split`/`rsplit` without a
qualification to “`split`/`rsplit` when `maxsplit` is supplied.” Add this
sentence after the development guide's method-mutation paragraph:

```markdown
The `split`/`rsplit` swap requires an explicit second positional argument or
the named `maxsplit` keyword; calls that omit `maxsplit` produce identical
results and are skipped.
```

- [ ] **Step 6: Run the analyzer suite**

Run:

```bash
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests
```

Expected: all analyzer tests pass; benchmark-only tests remain ignored.

- [ ] **Step 7: Commit the behavior change**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs README.md docs/development.md
git commit -m "fix(analyzer): require maxsplit for split swaps"
```

### Task 2: Verify the complete branch

**Files:**
- Modify only if a check identifies a defect in Task 1.

**Interfaces:**
- Consumes: the committed Issue 323 implementation and repository quality configuration.
- Produces: fresh evidence that the branch meets the Rust and Python repository contracts.

- [ ] **Step 1: Run formatting, lint, and repository tests**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
git diff --check origin/main...HEAD
```

Expected: each command exits 0. Rust reports only intentional ignored tests;
Python reports 134 tests with four platform skips on macOS.

- [ ] **Step 2: Review the requirement diff**

Compare `origin/main...HEAD` with Issue 323 and the design. Confirm that the
diff contains the two worktree documents, one predicate, one match guard, the
focused regression, the positive fixture adjustment, and two documentation
clarifications. Remove any unrelated change before review.
