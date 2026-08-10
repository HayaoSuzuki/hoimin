# Issue #268 Raise Exception Pairs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the existing safe `exception_type_pair` operator to simple built-in exception names used as the primary expression of Python `raise` statements.

**Architecture:** Add one AST-context helper to `AstCandidateCollector` and invoke it from the existing statement visitor before normal child traversal. Reuse the curated pair map, scope-aware `resolves_builtin_pair`, candidate pipeline, operator ID, selectors, profiles, and report schema.

**Tech Stack:** Rust 1.88+, `littrs-ruff-python-ast`/parser 0.6.2, Tokio integration tests, Cargo, Python 3.14 via uv.

## Global Constraints

- Work only in `.worktrees/issue-268-raise-exception-pairs` on `feat/issue-268-raise-exception-pairs`.
- Keep `exception_type_pair` as the only safe exception operator and do not add a new selector, profile rule, or report field.
- Mutate only `raise Name` and `raise Name(...)`; replace exactly the primary exception name.
- In `raise ... from cause`, never classify the cause as an exception type.
- Require both source and destination names to resolve definitely through Python builtins at the primary name's position.
- Skip bare, qualified, dynamic, shadowed, ambiguous, termination, and control-flow exception forms.
- Preserve existing non-exception candidates inside primary constructor arguments and cause expressions.
- Add no dependency and no new Lean model; exact AST and scope correspondence belongs in Rust tests.
- Use `[skip ci]` for commits that contain only documentation.
- Merge the final PR with squash after all required checks pass.

## File structure

- `crates/hoimin-cli/src/analyzer/rust.rs`: extract the supported primary raised-exception name and emit curated candidates.
- `crates/hoimin-cli/src/analyzer/rust_tests.rs`: exact spans, supported and excluded forms, scope safety, reparsing, and shared-pipeline behavior.
- `crates/hoimin-cli/tests/plan.rs`: persisted plan/verify coverage for a raised-exception candidate.
- `crates/hoimin-cli/tests/run_e2e.rs`: final JSON report coverage for both raised and handled exception candidates.
- `README.md`: user-facing supported raise forms and exclusions.
- `docs/development.md`: contributor traversal, resolution, and regression-test guidance.

---

### Task 1: Collect safe raised-exception candidates

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: `exception_pair_replacements(&str)`, `AstFacts::resolves_builtin_pair(TextRange, &str, &str)`, and `AstCandidateCollector::add_candidate`.
- Produces: `AstCandidateCollector::collect_raised_exception(&ruff_python_ast::StmtRaise)`; candidates use `MutationOperator::ExceptionTypePair`.

- [ ] **Step 1: Write failing exact-shape and span tests**

Add `raise_exception_type_pair_candidates_preserve_primary_expression` with a fixture containing `raise ValueError`, `raise TypeError(message, code=code)`, `raise KeyError(key) from cause`, and every remaining curated source name. Filter `exception_type_pair` candidates and assert exact `(original, replacement, ByteSpan, line, symbol)` tuples. Assert the `KeyError` name produces both `IndexError` and `AttributeError`, apply every candidate with `apply_candidate_and_reparse`, and assert the resulting constructor arguments and `from cause` text are unchanged.

Add `raise_exception_type_pairs_skip_unsupported_and_non_primary_forms`:

```rust
let source = concat!(
    "def reraised():\n    raise\n",
    "def qualified():\n    raise errors.ValueError\n",
    "def dynamic():\n    raise factory()\n",
    "def subscripted():\n    raise errors[kind]()\n",
    "def cause_only():\n    raise CustomError from ValueError\n",
    "def termination():\n    raise SystemExit(1)\n",
);
assert!(analyze(source).candidates.iter().all(|candidate| {
    candidate.operator != "exception_type_pair"
}));
```

- [ ] **Step 2: Write failing scope and pipeline tests**

Add a test with clean sibling scopes plus source and destination shadowing in module, function, nested function, class, import, parameter, comprehension, wildcard-import, and post-`exec` positions. Assert only occurrences where both names definitely resolve to builtins emit candidates.

Add assertions that a clean raised exception remains selected under both `MutationProfile::Full` and `MutationProfile::Focused`, exact line and symbol filters select it, `max_candidates` truncates through the existing prefix, and excluding `ExceptionTypePair` removes it. Use existing `analyze_with_profile`, `analyze_with`, and `AnalyzeRequest` helpers rather than a new pipeline.

In `plans_for_new_operator_families_pass_verify`, change the `exception_ops`
source to include a direct `raise ValueError(...)` candidate and assert that the
persisted candidate has operator `exception_type_pair`, original `ValueError`,
and replacement `TypeError`. In
`exception_default_run_reports_canonical_json_candidate`, retain the fixture
containing both `raise ValueError` and `except ValueError`, then expect exactly
two distinct `exception_type_pair` mutants with span length 10.

- [ ] **Step 3: Run focused tests and observe RED**

Run:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::raise_exception_type_pair -- --nocapture
cargo test -p hoimin-cli --test plan plans_for_new_operator_families_pass_verify -- --exact
cargo test -p hoimin-cli --test run_e2e exception_default_run_reports_canonical_json_candidate -- --exact
```

Expected: FAIL because no `exception_type_pair` candidate is collected from a `Stmt::Raise` primary expression.

- [ ] **Step 4: Implement the minimal AST collector**

Add the helper beside `collect_exception_handler`:

```rust
fn collect_raised_exception(&mut self, statement: &ruff_python_ast::StmtRaise) {
    let Some(primary) = statement.exc.as_deref() else {
        return;
    };
    let name = match primary {
        Expr::Name(name) => name,
        Expr::Call(call) => match call.func.as_ref() {
            Expr::Name(name) => name,
            _ => return,
        },
        _ => return,
    };
    for replacement in exception_pair_replacements(name.id.as_str()) {
        if self
            .facts
            .resolves_builtin_pair(name.range(), name.id.as_str(), replacement)
        {
            self.add_candidate(
                name.range(),
                (*replacement).to_owned(),
                MutationOperator::ExceptionTypePair,
            );
        }
    }
}
```

Extend `AstCandidateCollector::visit_stmt` without returning early for raises:

```rust
if let Stmt::Raise(statement_raise) = statement {
    self.collect_raised_exception(statement_raise);
}
visitor::walk_stmt(self, statement);
```

Retain the specialized `Stmt::Try` traversal and cancellation guards. Do not manually visit `exc` or `cause`; `visitor::walk_stmt` must visit each exactly once.

- [ ] **Step 5: Run focused tests and observe GREEN**

Run:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::raise_exception_type_pair -- --nocapture
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests
cargo test -p hoimin-cli --test plan plans_for_new_operator_families_pass_verify -- --exact
cargo test -p hoimin-cli --test run_e2e exception_default_run_reports_canonical_json_candidate -- --exact
```

Expected: all matching and analyzer tests PASS.

- [ ] **Step 6: Commit analyzer behavior**

```console
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs crates/hoimin-cli/tests/plan.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "feat(analyzer): mutate raised exception pairs"
```

---

### Task 2: Document the raised-exception safety contract

**Files:**
- Modify: `README.md`
- Modify: `docs/development.md`

**Interfaces:**
- Consumes: supported shapes and exclusions from the approved design and Task 1.
- Produces: user and contributor documentation consistent with observable analyzer behavior.

- [ ] **Step 1: Record the stale documentation before editing**

Confirm README and the development guide still state that `raise` expressions
are not changed. These are the exact stale claims the documentation edit must
remove; do not add string-matching assertions for human prose.

- [ ] **Step 2: Update README and development documentation**

Change the catalog row from handler-only wording to curated exception types in `except` and supported `raise` contexts. Replace the future-extension text with exact accepted forms and exclusions. In `docs/development.md`, explain that `visit_stmt` inspects only `StmtRaise.exc`, replaces the simple primary name or simple call callee, then walks `exc` and `cause` normally once.

- [ ] **Step 3: Verify documentation consistency and focused behavior**

Run:

```console
cargo test -p hoimin-cli --test cli_config development_docs_explain_exception_mutation_policy -- --exact
cargo test -p hoimin-cli --test cli_config readme_documents_all_mutation_operator_ids_and_selector_families -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::raise_exception_type_pair -- --nocapture
git diff --check
```

Expected: PASS.

- [ ] **Step 4: Commit documentation**

This commit contains only documentation and therefore uses `[skip ci]`.

```console
git add README.md docs/development.md
git commit -m "docs: describe raised exception mutations [skip ci]"
```

---

### Task 3: Verify, review, and integrate

**Files:**
- Verify all changed files and the complete workspace.

**Interfaces:**
- Consumes: Tasks 1–2 and the approved design.
- Produces: a reviewed, green PR that closes #268 through squash merge.

- [ ] **Step 1: Format and lint**

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check main...HEAD
```

- [ ] **Step 2: Run complete verification**

Run:

```console
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
uv run --frozen python -m unittest tests/test_skills.py
```

- [ ] **Step 3: Review the complete diff against the design**

Compare `main...HEAD` line by line with the scope, exclusions, traversal rule,
shared-pipeline requirements, documentation, and non-goals. Fix every Critical
or Important finding and rerun the relevant focused test plus Step 1 and Step 2.

- [ ] **Step 4: Push and create the PR**

Push `feat/issue-268-raise-exception-pairs`, create a PR against `main` with
`Closes #268`, summarize red-green evidence and exclusions, and wait for all
required GitHub Actions checks.

- [ ] **Step 5: Squash merge and clean up**

After the PR is clean, mergeable, and green, squash merge it. Verify #268 is
closed, fast-forward local `main` to `origin/main`, remove only
`.worktrees/issue-268-raise-exception-pairs`, prune worktree metadata, and delete
the merged local and remote feature branches. Preserve every unrelated untracked
file in the main worktree.
