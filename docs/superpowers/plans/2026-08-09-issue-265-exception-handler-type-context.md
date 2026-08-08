# Issue 265 Exception Handler Type Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent `collection_list_tuple` from emitting invalid list
replacements for `except` and `except*` handler type tuples while preserving
ordinary collection and dedicated exception mutations.

**Architecture:** `AstCandidateCollector` maintains a private, balanced
exception-type traversal depth. Ordinary and starred exception handlers visit
their optional type through one helper, and only the list/tuple literal
candidate collectors consult that context. Traversal, dedicated exception
candidate collection, handler bodies, ordering, and report behavior remain
unchanged.

**Tech Stack:** Rust 1.88+, Ruff Python AST visitor, existing analyzer unit
tests, and the production CLI E2E harness.

## Global Constraints

- Suppress only `collection_list_tuple` candidates inside the type expression
  of `except` and `except*` handlers.
- Continue walking handler type expressions and handler bodies.
- Preserve ordinary list/tuple mutations, including literals in handler
  bodies.
- Preserve `exception_type_pair` and explicitly selected risky exception
  operators, including exact spans, replacements, order, and profile behavior.
- Preserve candidate IDs, deduplication, limits, diagnostics, and cancellation.
- Add no dependency, CLI option, schema field, or operator ID.
- Behavior commits do not use `[skip ci]`; pure documentation commits do.

---

### Task 1: Add exception-type traversal context to the analyzer

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**

- Adds private `AstCandidateCollector::exception_type_depth: usize`.
- Adds private
  `AstCandidateCollector::visit_exception_type(&'ast Expr)`.
- Changes `visit_except_handler` and the `Stmt::Try` starred-handler branch to
  use the helper for optional handler type expressions.
- Changes `collect_list_literal` and `collect_tuple_literal` to decline
  collection-shape candidates while `exception_type_depth > 0`.

- [ ] **Step 1: Write failing analyzer regressions**

  Add hand-written candidate assertions for source containing ordinary,
  nested, parenthesized, and starred handlers. Assert that no
  `collection_list_tuple` candidate has an original equal to
  `(ValueError, TypeError)` or another handler-type tuple, while literal tuples
  and lists before, after, and in the handler body retain their exact
  replacements and source spans.

  Extend the dedicated exception tests with adjacent collection literals and
  assert the existing safe and risky exception candidate vectors are
  unchanged. Reparse every retained replacement with the existing parser test
  helper.

- [ ] **Step 2: Run the focused tests and record RED**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_handler_type -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_type_pair_candidates_are_curated_and_syntax_directed -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_risky_candidates_require_explicit_selection_and_reparse -- --nocapture
  ```

  Expected: the new handler-type tests fail because tuple-to-list candidates
  are currently emitted for `except` and `except*`; the existing dedicated
  exception tests pass.

- [ ] **Step 3: Implement the balanced collector context**

  Initialize `exception_type_depth` to zero. Implement:

  ```rust
  fn visit_exception_type(&mut self, expression: &'ast Expr) {
      self.exception_type_depth += 1;
      self.visit_expr(expression);
      self.exception_type_depth -= 1;
  }
  ```

  In `visit_except_handler`, call `collect_exception_handler` first, visit the
  optional type through the helper, and then visit each body statement. Do not
  call `visitor::walk_except_handler`, because it would visit the type outside
  the role context. In the existing `except*` branch, replace the direct
  `visit_expr` call for the type with the same helper without enabling
  dedicated risky exception collection there.

  Add `self.exception_type_depth == 0` to the existing load-context and
  annotation exclusions in both literal collectors. Do not change other
  collector gates.

- [ ] **Step 4: Verify analyzer behavior**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_handler_type -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::collection -- --nocapture
  cargo fmt --all -- --check
  cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
  git diff --check
  ```

  Expected: all focused regressions pass, including unchanged dedicated
  exception candidates and ordinary collection candidates.

- [ ] **Step 5: Commit Task 1**

  ```bash
  git add crates/hoimin-cli/src/analyzer/rust.rs \
    crates/hoimin-cli/src/analyzer/rust_tests.rs
  git commit -m "fix: exclude exception handler types from collection mutations"
  ```

---

### Task 2: Exercise the exclusion through the production CLI

**Files:**

- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**

- Adds an issue-specific temporary Python project fixture with a tuple handler
  type and an ordinary tuple in its handler body.
- Uses the existing `run_project_options` production CLI path with only
  `collection_list_tuple` selected.

- [ ] **Step 1: Write the failing production CLI regression**

  Add a fixture equivalent to:

  ```python
  def classify():
      try:
          raise ValueError
      except (ValueError, TypeError):
          return (1, 2)
  ```

  Run it with a command that imports the module, calls `classify()`, and
  accepts either tuple or list shape by asserting
  `list(classify()) == [1, 2]`. This guarantees that the baseline and retained
  ordinary mutant both execute the exception handler path.

  Assert the final JSON report is parseable and complete, contains the exact
  ordinary body tuple candidate with original `(1, 2)` and replacement
  `[1, 2]`, and contains no candidate whose original is
  `(ValueError, TypeError)`. Assert the observed candidate count and outcome so
  an omitted discovery phase cannot make the test pass vacuously.

- [ ] **Step 2: Run the E2E test and record RED**

  Run:

  ```bash
  cargo test -p hoimin-cli --test run_e2e exception_handler_type_collection_candidates_are_excluded -- --nocapture
  ```

  Expected before Task 1's production change: the report contains the invalid
  handler tuple candidate or has a larger candidate count. When Task 1 is
  already present, validate the test against the parent of Task 1 or document
  that its test-first failure is proven by temporarily reverting only the
  context gate, then restore the implementation before committing.

- [ ] **Step 3: Verify the production path and neighboring E2E tests**

  Run:

  ```bash
  cargo test -p hoimin-cli --test run_e2e exception_handler_type_collection_candidates_are_excluded -- --nocapture
  cargo test -p hoimin-cli --test run_e2e collection_default_run_reports_canonical_ids_with_stable_spans -- --nocapture
  cargo test -p hoimin-cli --test run_e2e exception_default_run_reports_canonical_json_candidate -- --nocapture
  cargo fmt --all -- --check
  git diff --check
  ```

  Expected: the real CLI runs the handler path, reports only the ordinary
  tuple-to-list mutant for the fixture, and neighboring collection/exception
  report contracts remain unchanged.

- [ ] **Step 4: Commit Task 2**

  ```bash
  git add crates/hoimin-cli/tests/run_e2e.rs
  git commit -m "test: cover exception handler collection exclusion"
  ```

---

### Task 3: Document and verify the syntax-role invariant

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`

**Interfaces:**

- Documents the user-visible exclusion in the collection operator catalog.
- Documents the collector traversal invariant for future analyzer changes.

- [ ] **Step 1: Update user and developer documentation**

  In `README.md`, state beside `collection_list_tuple` that ordinary list and
  tuple literals remain eligible but exception handler type positions are
  excluded because Python requires an exception class or tuple of exception
  classes.

  In `docs/development.md`, state that `AstCandidateCollector` must enter its
  exception-type context for both `except` and `except*`, keep the context
  balanced, collect dedicated exception candidates outside the generic gate,
  and visit handler bodies normally.

- [ ] **Step 2: Commit the pure documentation change**

  ```bash
  git add README.md docs/development.md
  git commit -m "docs: document exception handler type exclusions [skip ci]"
  ```

- [ ] **Step 3: Run the complete verification suite**

  Ensure the worktree `.venv` resolves to the repository test environment,
  then run:

  ```bash
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace
  ./.venv/bin/python -m unittest discover -s tests -p 'test_*.py'
  git diff --check
  git status --short
  ```

  Expected: all Rust and Python tests pass; formatting, Clippy, and diff checks
  are clean; only intentional issue files differ from `origin/main`.

- [ ] **Step 4: Add a non-skip validation commit**

  Because the documentation commit skips CI, create an empty final commit so
  GitHub Actions validates the complete branch:

  ```bash
  git commit --allow-empty -m "ci: validate exception handler type exclusions"
  ```

- [ ] **Step 5: Request final review and deliver**

  Review the complete range from the issue branch base through `HEAD`, address
  findings through the responsible implementation task, rerun the relevant
  focused tests and complete verification, then push and create one PR with
  `Closes #265`. Merge only after every required CI check succeeds, using
  squash merge, and verify that Issue #265 closes.
