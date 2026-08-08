# Issue 264 Typing Import Rebinding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate type-annotation mutations only with typing import spellings
that still resolve to the intended symbol at the annotation site.

**Architecture:** The annotation collector walks statement suites in source
execution order and attaches a cloned `KnownImports` environment to every
annotation site. Binding transfers invalidate or establish names, function
scopes predeclare their Python-local names, and compound statements intersect
reachable exit environments. Candidate generation consumes the site snapshot
instead of one module-wide map.

**Tech Stack:** Rust 1.88+, Ruff Python AST visitor/types, existing analyzer
unit and integration tests.

## Global Constraints

- Direct typing names and module aliases are usable only while they resolve to
  the intended `typing` or `collections.abc` symbol at that annotation site.
- Rebinding after an annotation does not retroactively suppress its candidate;
  rebinding before it does.
- An unconditional supported import may establish or re-establish a safe
  spelling from that point onward.
- Function-local bindings hide matching outer aliases according to Python's
  static local-name rules; function signatures use the enclosing environment.
- A control-flow join retains a name only when every reachable exit maps it to
  the identical known symbol.
- Unsafe or ambiguous spelling causes the import-dependent candidate to be
  skipped, never guessed.
- Preserve unshadowed preferred spellings, operators, ordering, profiles,
  limits, diagnostics, symbols, and cancellation.
- Do not change the separate builtin/exception shadowing policy.
- Add no dependency, CLI option, schema field, or operator ID.
- Behavior commits do not use `[skip ci]`; pure documentation commits do.

---

### Task 1: Attach source-order import snapshots to annotation sites

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**

- Produces internal `AnnotationSite<'ast> { annotation, symbol, imports }`.
- Makes `KnownImports` cloneable and comparable for snapshots and joins.
- Adds transfer operations for supported imports and unknown bindings.
- Changes `type_annotation_candidates` to call `annotation_replacements` with
  each site's `KnownImports` snapshot rather than `AstFacts.imports`.

- [ ] **Step 1: Write failing linear source-order tests**

  Add exact tests for the module fixture:

  ```python
  from typing import Sequence
  before: list[str]
  Sequence = local_sequence
  after: list[str]
  from typing import Sequence as Sequence
  restored: list[str]
  ```

  Select type operators and assert `type_list_sequence` candidates exist at
  annotation byte spans `36..45` and `139..148`, both replacing with
  `Sequence[str]`, while no candidate exists at `79..88`. Include the expected
  one-based lines and `symbol: None`.

  Add corresponding direct-alias, `import typing as t`, competing import,
  function-definition, and class-definition cases. Assert a definition named
  `Sequence` changes only annotations after the definition. Reparse every
  emitted replacement.

- [ ] **Step 2: Run the new tests and record RED**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_import_rebinding_linear -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_module_alias_rebinding_linear -- --nocapture
  ```

  Expected: candidates are incorrectly emitted after assignment, definition,
  or competing import because the current module-wide map never invalidates
  them.

- [ ] **Step 3: Implement linear environment transfers**

  Add `KnownImports` methods that process an import alias and invalidate a
  local name from `direct`, `modules`, and `type_vars`. A supported direct or
  module import overwrites prior state with its known resolved symbol. A star
  import or unsupported competing import invalidates the local spelling it can
  bind; unsupported wildcard effects invalidate all known direct spellings.

  Introduce `AnnotationSite` and make the collector maintain a current
  environment while walking a suite. For statements whose annotations or
  expressions are evaluated before their binding takes effect, collect the
  annotation first and transfer the binding afterward. Function signature
  annotations are recorded in the current environment before the function
  name is invalidated.

  Replace `AstFacts.imports` consumption with the site snapshot. Preserve
  existing `KnownImports::resolved_name` and `spelling_for` preference and
  deterministic lexicographic fallback.

- [ ] **Step 4: Verify linear behavior and existing type operators**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_import_rebinding_linear -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_module_alias_rebinding_linear -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::type_annotations -- --nocapture
  cargo fmt --all -- --check
  cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
  git diff --check
  ```

  Expected: linear rebinding tests pass and the existing seven
  `type_annotations` tests retain their exact preferred spellings.

- [ ] **Step 5: Commit Task 1**

  ```bash
  git add crates/hoimin-cli/src/analyzer/rust.rs \
    crates/hoimin-cli/src/analyzer/rust_tests.rs
  git commit -m "fix: snapshot typing imports at annotation sites"
  ```

---

### Task 2: Model lexical scopes and conservative control flow

**Files:**

- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**

- Produces a function-local binding prepass that does not descend into nested
  function or class bodies.
- Produces scope entry/exit for function and class suites without leaking
  ordinary bindings to their parents.
- Produces `KnownImports` intersection for reachable control-flow exits.
- Preserves enclosing-scope signature lookup separately from function-body
  free-name lookup.

- [ ] **Step 1: Write failing scope and control-flow tests**

  Add hand-written vectors for:

  ```python
  from typing import Sequence

  def outer(value: Sequence[str]):
      before: list[str]
      Sequence = local_sequence

  untouched: list[str]
  ```

  Assert the function signature may resolve the outer import, the body
  `before` annotation has no import-dependent candidate because `Sequence` is
  statically local, and `untouched` retains `Sequence[str]`. Assert exact
  spans and symbols (`outer` for the local annotation, `None` at module
  scope).

  Add parameter, nested function, nested class, function-local typing import,
  module-alias rebinding, `if` with and without `else`, loop zero-iteration,
  `try`/handler, `with`, `except as`, match-pattern, and named-expression
  cases. Each test must retain at least one unaffected candidate and reject
  only ambiguous/shadowed sites.

- [ ] **Step 2: Run the new tests and record RED**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_import_rebinding_scope -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_import_rebinding_control_flow -- --nocapture
  ```

  Expected: outer aliases leak into function-local sites or branch-local
  transfers incorrectly determine the continuation state.

- [ ] **Step 3: Implement function-local name discovery**

  Collect every name Python treats as local to a function: parameters,
  imports, assignment-family targets, definitions, loop/with/except targets,
  named expressions, and pattern captures. Do not descend into nested function
  or class bodies, and do not leak comprehension-local targets. Respect
  `global` and `nonlocal` declarations conservatively.

  On function-body entry, clone the proper non-class enclosing environment and
  invalidate predeclared locals before processing the first statement. Process
  the defining function's signature in its enclosing environment. For a
  method, evaluate the signature in class state but make its body free-name
  fallback bypass the class namespace.

- [ ] **Step 4: Implement branch cloning and intersection**

  Add an intersection operation that retains only identical `direct`,
  `modules`, and safe type-variable entries across all reachable exits.
  Explicitly process suites for `if`, loops, `try`/handlers/`else`/`finally`,
  `match`, and other compound statements that can contain annotations or
  bindings. Include the unchanged path for an absent `else`, loop zero
  iterations, and unmatched non-irrefutable `match`.

  A branch-local supported import may remain known only if every exit produces
  the same mapping. Bindings in a nested class/function must not alter the
  enclosing continuation except for binding the definition name after the
  statement.

- [ ] **Step 5: Verify scopes, joins, and regression coverage**

  Run:

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_import_rebinding -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::type_annotations -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::annotations -- --nocapture
  cargo fmt --all -- --check
  cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
  git diff --check
  ```

  Expected: exact scope/source-order vectors pass, every retained replacement
  reparses, and existing annotation and profile behavior remains unchanged.

- [ ] **Step 6: Commit Task 2**

  ```bash
  git add crates/hoimin-cli/src/analyzer/rust.rs \
    crates/hoimin-cli/src/analyzer/rust_tests.rs
  git commit -m "fix: model typing imports across lexical scopes"
  ```

---

### Task 3: Prove site-aware resolution through the analyzer integration path

**Files:**

- Modify: `crates/hoimin-cli/tests/rust_analyzer.rs`

**Interfaces:**

- Uses the existing integration crate's real `analyze_source` path.
- Selects only `MutationOperator::TypeListSequence` for a canonical fixture.
- Observes complete `AnalyzerCandidate` descriptors, not internal environment
  helpers.

- [ ] **Step 1: Write the analyzer integration regression**

  Add a fixture with one `list[str]` annotation before and one after direct
  `Sequence` rebinding, plus a qualified `typing as t` alias that is rebound
  later. Assert the inventory contains only the two pre-rebinding candidates,
  with exact path, byte spans, originals, replacements, operators, line,
  column, and symbol. Assert no post-rebinding range appears and reparse every
  candidate through `apply_candidate_and_reparse`.

- [ ] **Step 2: Demonstrate the regression's sensitivity**

  Run the test against the Task 2 tree, then temporarily disable the binding
  transfer or query the parent of the implementation commit and record the
  resulting extra candidates. Restore the production code before final
  verification. The expected pre-fix failure is an inventory larger than the
  literal expected vector.

- [ ] **Step 3: Run integration and neighboring regressions**

  Run:

  ```bash
  cargo test -p hoimin-cli --test rust_analyzer typing_import_rebinding_inventory_is_site_aware -- --nocapture
  cargo test -p hoimin-cli --test rust_analyzer type_annotations -- --nocapture
  cargo test -p hoimin-cli --test analyzer_handler
  cargo fmt --all -- --check
  git diff --check
  ```

  Expected: the exact non-vacuous inventory passes through the production
  analyzer path and protocol regressions remain green.

- [ ] **Step 4: Commit Task 3**

  ```bash
  git add crates/hoimin-cli/tests/rust_analyzer.rs
  git commit -m "test: cover site-aware typing import resolution"
  ```

---

### Task 4: Document and verify the resolution contract

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`

**Interfaces:**

- Documents the user-visible safe-spelling rule beside type operators.
- Documents source-order snapshots, function-local predeclaration, and
  conservative control-flow intersection for future analyzer changes.

- [ ] **Step 1: Update user and developer documentation**

  In `README.md`, state that import-dependent type replacements are emitted
  only while a direct name or module alias remains unshadowed at the
  annotation site; otherwise the candidate is skipped.

  In `docs/development.md`, record that signature annotations use their
  enclosing state, function bodies predeclare Python-local names, nested scopes
  do not leak, and control-flow exits retain only identical known imports.

- [ ] **Step 2: Commit the pure documentation change**

  ```bash
  git add README.md docs/development.md
  git commit -m "docs: document typing import resolution [skip ci]"
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
  are clean; only intentional Issue #264 files differ from `origin/main`.

- [ ] **Step 4: Add a non-skip validation commit**

  ```bash
  git commit --allow-empty -m "ci: validate typing import rebinding"
  ```

- [ ] **Step 5: Request final review and deliver**

  Review the complete branch from `origin/main` through `HEAD`, address all
  Critical and Important findings in one reviewed fix wave, rerun the complete
  verification, then push and create one PR with `Closes #264`. Merge only
  after every required CI check succeeds, using squash merge, and verify that
  Issue #264 closes.
