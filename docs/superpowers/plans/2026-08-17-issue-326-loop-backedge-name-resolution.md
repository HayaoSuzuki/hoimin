# Issue #326: Loop back-edge name resolution implementation plan

**Goal:** Suppress builtin-pair candidates when a loop back edge can expose a
later binding at an earlier tracked load.

**Architecture:** Track repeated-region occurrences and same-scope binding
names in `NameResolutionBuilder`, store affected offsets in a sparse resolution
index, and demote only otherwise-definite builtin resolution to `Unknown`.

**Tech stack:** Rust, Ruff Python AST visitor, Cargo tests, Python unittest
contracts, focused cargo-mutants verification.

---

### Task 1: Add failing loop regressions

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

1. Reproduce the reported module-level `for` candidate.
2. Cover a later replacement-name binding, class scope, `while` test, and
   nested loop.
3. Assert that a builtin in the one-time `for` iterable remains eligible.
4. Assert that a binding inside a nested function does not affect the outer
   repeated region.
5. Run the exact regressions and confirm unsafe candidates or definite-builtin
   snapshots fail before implementation.

Command:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::loop_back_edge_bindings_make_builtin_resolution_uncertain -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::loop_back_edge_bindings_suppress_builtin_pair_candidates -- --exact
```

### Task 2: Track active repeated regions

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

1. Add a sparse offset-to-`back_edge_bindings` map to `NameResolutionIndex`.
2. Add an active-loop context stack containing owner scope, occurrence offsets,
   and binding names.
3. Register tracked loads with each visible active loop context.
4. Route definite, unknown, and wildcard binding paths through active-loop
   recording.
5. On loop close, extend every recorded occurrence with all context binding
   names.

### Task 3: Give `for` and `while` precise repeated boundaries

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

1. Keep `for` iterable evaluation outside the repeated context.
2. Keep `for` target binding, target evaluation, and body inside the context;
   keep its `else` suite outside.
3. Visit `while` test and body inside one repeated context, with `else` outside.
4. Preserve conditional-depth accounting and AST evaluation order.
5. Demote only `DefinitelyBuiltin` resolutions whose queried name appears in
   the occurrence's back-edge set.
6. Run the exact regressions and existing scope-aware name-resolution tests.

Commands:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::loop_back_edge_bindings_make_builtin_resolution_uncertain -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::loop_back_edge_bindings_suppress_builtin_pair_candidates -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -- name_resolution
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -- shadowed
```

### Task 4: Verify and review

1. Run formatting and lint checks.
2. Run the analyzer, Rust workspace, and Python contract suites.
3. Build and smoke-test the release wheel.
4. Run focused mutation testing for the demotion guard and loop boundaries.
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

1. Push the issue branch and create a PR that closes #326.
2. Monitor every CI job and diagnose any failure.
3. Squash merge after required checks succeed.
4. Fast-forward local `main`, rerun the focused loop regression, and confirm
   #326 is closed.
5. Remove only the Issue #326 worktree and its merged local branch.
