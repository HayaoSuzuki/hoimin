# Issue #324: Annotation token gate implementation plan

**Goal:** Prevent default raw-token mutation operators from producing
candidates inside recorded Python annotation spans while preserving executable
tokens and opt-in type-annotation operators.

**Architecture:** Reuse the finalized `AstFacts` annotation containment index
as a spelling-independent gate in the token scanner. Keep replacement dispatch
and all later candidate processing unchanged.

**Tech stack:** Rust, Ruff Python AST/parser, Cargo tests, Python unittest
contracts, cargo-mutants focused verification.

---

### Task 1: Add the behavior regression

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

1. Add a test containing unary-sign, boolean-literal, and arithmetic tokens in
   annotations and the same operator families in executable code.
2. Assert exact operator, original text, and one-based line for the executable
   candidates.
3. Run the exact test and confirm that annotation candidates make it fail.

Command:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::annotations_suppress_all_default_token_mutations -- --exact
```

### Task 2: Generalize the token gate

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

1. Replace the four-spelling annotation condition with an unconditional
   containment check after the AST operator-token allowlist gate.
2. Rename and update the lookup-stat regression so addition, multiplication,
   and bitwise tokens are expected to query the annotation index equally.
3. Run both focused tests and the complete analyzer library suite.

Commands:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::annotations_suppress_all_default_token_mutations -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::all_operator_tokens_query_annotation_index -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests
```

### Task 3: Document the invariant

**Files:**

- Modify: `docs/development.md`

1. State that every AST-approved raw-token operator is checked against the
   annotation containment index.
2. Explain that opt-in `type_*` candidates remain a separate producer.
3. Run documentation contracts through the Python unittest suite.

### Task 4: Verify and review

1. Run formatting and lint checks.
2. Run the Rust workspace and Python contract suites.
3. Build and smoke-test the wheel.
4. Run focused Rust mutation testing for the changed gate and regression.
5. Request independent code review and address findings.
6. Confirm `git diff --check` and a clean worktree.

Commands:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --quiet
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uv run maturin build --release
uv run --frozen python tests/wheel_smoke.py
git diff --check origin/main...HEAD
```

### Task 5: Deliver and clean up

1. Push the issue branch and create a PR that closes #324.
2. Monitor every CI job and diagnose any failure.
3. Squash merge after required checks succeed.
4. Fast-forward local `main`, rerun the focused regression, and confirm #324 is
   closed.
5. Remove only the Issue #324 worktree and its merged local branch.
