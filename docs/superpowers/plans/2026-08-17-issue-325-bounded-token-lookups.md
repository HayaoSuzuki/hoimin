# Issue #325: Bounded candidate token lookups implementation plan

**Goal:** Replace per-candidate whole-module token scans with borrowed,
range-bounded token slices while preserving analyzer output.

**Architecture:** Route candidate-local punctuation queries through an
`AstFacts` accessor backed by Ruff `Tokens::in_range`. Add test-only scan
accounting to enforce a deterministic complexity bound.

**Tech stack:** Rust, Ruff Python AST/parser, Cargo tests, Python unittest
contracts, cargo-mutants focused verification.

---

### Task 1: Add deterministic scan accounting and a failing regression

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

1. Add test-only candidate token scan statistics to `AstFacts` and
   `AnalyzerOutput`.
2. Route the existing whole-stream scans through accounting without changing
   their search behavior.
3. Generate repeated `.get`, list-literal, and exception-tuple inputs.
4. Assert that each path produces candidates and examines at most a small
   constant number of tokens per lookup.
5. Run the exact regression and confirm the whole-stream implementation fails
   the complexity bound.

Command:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::candidate_punctuation_queries_are_range_bounded -- --exact
```

### Task 2: Bound every reported lookup

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

1. Add `AstFacts::candidate_tokens_in_range(TextRange)` using
   `Tokens::in_range` without allocation.
2. Use `call.arguments.inner_range()` for trailing argument commas.
3. Use the element-to-list-end range for single-element list commas.
4. Use `tuple.range()` for exception tuple additions and removals.
5. Remove unrestricted `Tokens` parameters from the affected helpers.
6. Run the exact regression and the existing focused semantic tests.

Commands:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::candidate_punctuation_queries_are_range_bounded -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -- tuple
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -- trailing
```

### Task 3: Document the invariant

**Files:**

- Modify: `docs/development.md`

1. State that candidate-local punctuation searches use the shared bounded
   token accessor.
2. Explain the test-only lookup and examined-token counters.
3. Run documentation contracts through the Python unittest suite.

### Task 4: Verify and review

1. Run formatting and lint checks.
2. Run the analyzer, Rust workspace, and Python contract suites.
3. Build and smoke-test the release wheel.
4. Run focused Rust mutation testing for the new range boundaries.
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

1. Push the issue branch and create a PR that closes #325.
2. Monitor every CI job and diagnose any failure.
3. Squash merge after required checks succeed.
4. Fast-forward local `main`, rerun the focused complexity regression, and
   confirm #325 is closed.
5. Remove only the Issue #325 worktree and its merged local branch.
