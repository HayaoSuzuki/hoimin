# Issue 263 PEP 695 Type-Position Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Classify every parser-supported PEP 695 bound, default, and type-alias value as a type position, suppress runtime mutations there, and emit existing type operators for supported expressions.

**Architecture:** Add one allocation-free helper that visits expression-valued positions in Ruff `TypeParams`. Reuse it from `AstFacts` for runtime suppression and from `AnnotationCollector` for source-ordered type-candidate collection, keeping import snapshots and qualified symbols in the existing collector.

**Tech Stack:** Rust 2024, `littrs-ruff-python-ast` 0.6.2, `littrs-ruff-python-parser` 0.6.2, Cargo tests, Clippy.

## Global Constraints

- Keep mutation operator IDs, profiles, defaults, selectors, report schemas, candidate ordering, limits, fingerprints, and exact source spans unchanged.
- Keep ordinary parameter, return, and annotated-assignment behavior unchanged.
- Keep unsupported-syntax behavior on the existing invalid-syntax diagnostic.
- Do not add dependencies or public APIs.
- Use independent expected candidate tuples and reparse every emitted replacement in the regression tests.
- Keep the design and implementation plan in the same Issue #263 worktree and PR.

---

### Task 1: Pin the Missing PEP 695 Behavior

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: existing `analyze`, `analyze_types`, and `apply_candidate_and_reparse` test helpers.
- Produces: `pep_695_type_positions_suppress_runtime_mutations` and `pep_695_type_positions_emit_type_candidates` regressions.

- [ ] **Step 1: Add a failing runtime-classification regression**

Add a fixture containing function and class bounds/defaults plus nearby runtime expressions. Assert that runtime `bitwise_and_or` or `binary_add_sub` candidates remain only on the ordinary assignments:

```rust
#[test]
fn pep_695_type_positions_suppress_runtime_mutations() {
    let source = concat!(
        "def convert[T: Left | Right = list[str]](value: T):\n",
        "    return left | right\n",
        "class Box[U: Base | None = tuple[int]]:\n",
        "    runtime = first + second\n",
    );
    let observed = analyze(source)
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "bitwise_and_or" | "binary_add_sub"
            )
        })
        .map(|candidate| (candidate.original.as_str(), candidate.line))
        .collect::<Vec<_>>();
    assert_eq!(observed, [("|", 2), ("+", 4)]);
}
```

- [ ] **Step 2: Add a failing type-candidate and symbol regression**

Use type aliases and all three Ruff type-parameter variants. Assert exact source text, replacement, operator, line, and qualified symbol without consulting a production type-position helper:

```rust
#[test]
fn pep_695_type_positions_emit_type_candidates() {
    let source = concat!(
        "from typing import Sequence\n",
        "type Values[T: list[str] = list[bytes], *Ts = list[float], **P = list[bool]] = list[int]\n",
        "class Outer:\n",
        "    type Nested[U: list[int]] = list[bytes]\n",
    );
    let output = analyze_types(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        [
            ("list[str]", "Sequence[str]", 2, Some("Values")),
            ("list[bytes]", "Sequence[bytes]", 2, Some("Values")),
            ("list[float]", "Sequence[float]", 2, Some("Values")),
            ("list[bool]", "Sequence[bool]", 2, Some("Values")),
            ("list[int]", "Sequence[int]", 2, Some("Values")),
            ("list[int]", "Sequence[int]", 4, Some("Outer.Nested")),
            ("list[bytes]", "Sequence[bytes]", 4, Some("Outer.Nested")),
        ]
    );
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}
```

If Ruff rejects a particular default combination because of Python's required
default ordering, split it into separate declarations while retaining coverage
for `TypeVar`, `TypeVarTuple`, and `ParamSpec` defaults.

Add a separate regression where an imported `typing.Sequence` is shadowed by a
PEP 695 type parameter named `Sequence` in a generic function, class, and type
alias. Assert that `type_list_sequence` is absent in those annotation scopes
but remains present immediately after them in the enclosing module.

- [ ] **Step 3: Run the focused tests and capture the red result**

Run:

```bash
cargo test -p hoimin-cli --lib pep_695_type_positions -- --nocapture
```

Expected: the runtime test observes extra candidates in bound/default ranges,
and the type-candidate test omits alias/bound/default candidates.

- [ ] **Step 4: Commit the regression tests**

```bash
git add crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "test: expose inconsistent PEP 695 type positions"
```

---

### Task 2: Share PEP 695 Type-Position Enumeration

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: Ruff `TypeParam`, `TypeParams`, and `Expr`; existing `AstFacts::record_annotation_range` and `AnnotationCollector::record`.
- Produces: `visit_type_param_expressions(type_params: &TypeParams, visit: impl FnMut(&Expr))`, a `KnownImports` type-parameter overlay, and consistent consumer integration.

- [ ] **Step 1: Import the Ruff type-parameter types**

Extend the grouped import with `TypeParam` and `TypeParams`:

```rust
use ruff_python_ast::{
    CmpOp, Expr, ExprCall, ExprContext, ExprList, ExprSlice, ExprSubscript,
    ExprTuple, ModModule, Number, Operator, Pattern, Stmt, TypeParam,
    TypeParams, UnaryOp, visitor,
};
```

- [ ] **Step 2: Implement the shared enumerator**

Add a private helper near the analyzer fact types:

```rust
fn visit_type_param_expressions<'ast>(
    type_params: &'ast TypeParams,
    mut visit: impl FnMut(&'ast Expr),
) {
    for parameter in type_params.iter() {
        match parameter {
            TypeParam::TypeVar(parameter) => {
                if let Some(bound) = parameter.bound.as_deref() {
                    visit(bound);
                }
                if let Some(default) = parameter.default.as_deref() {
                    visit(default);
                }
            }
            TypeParam::TypeVarTuple(parameter) => {
                if let Some(default) = parameter.default.as_deref() {
                    visit(default);
                }
            }
            TypeParam::ParamSpec(parameter) => {
                if let Some(default) = parameter.default.as_deref() {
                    visit(default);
                }
            }
        }
    }
}
```

- [ ] **Step 3: Route runtime suppression through the helper**

Add an `AstFacts` method:

```rust
fn record_type_param_ranges(&mut self, type_params: &TypeParams) {
    visit_type_param_expressions(type_params, |expression| {
        self.record_annotation_range(expression.range());
    });
}
```

Call it for `Stmt::FunctionDef`, `Stmt::ClassDef`, and `Stmt::TypeAlias` type
parameters. Keep `alias.value.range()` recorded exactly once. Do not record the
entire bracket range because type-parameter identifiers are not expression
mutation sites.

- [ ] **Step 4: Collect declaration-qualified type sites**

Add `KnownImports::enter_type_params` to invalidate any same-named imported
spelling and mark each declared name as a type variable. Add an
`AnnotationCollector` helper that temporarily applies this overlay, pushes the
declaration name, records every expression with the scoped import snapshot,
and restores both the imports and qualification stack:

```rust
fn in_type_param_scope(
    &mut self,
    name: &str,
    type_params: Option<&'ast TypeParams>,
    visit: impl FnOnce(&mut Self),
) {
    let outer = self.imports.clone();
    if let Some(type_params) = type_params {
        self.imports.enter_type_params(type_params);
    }
    self.qualname.push(name.to_owned());
    visit(self);
    self.qualname.pop();
    self.imports = outer;
}
```

Use it for function annotations, class header type positions, and type-alias
bounds/default/value before the declaration name is bound. Apply the same type
parameter overlay while collecting generic function and class bodies, then
restore the enclosing import environment. Keep decorators and ordinary
function defaults outside the overlay. Walk or invalidate permitted binding
expressions in their existing source order so `KnownImports` remains
conservative.

- [ ] **Step 5: Run focused tests and resolve only expectation mistakes**

Run:

```bash
cargo test -p hoimin-cli --lib pep_695_type_positions -- --nocapture
cargo test -p hoimin-cli --lib type_annotations -- --nocapture
```

Expected: all new and existing type-annotation tests pass. If an exact byte
offset or Python-valid fixture differs, derive the literal expectation from the
checked-in fixture text once; do not call a production helper from the test.

- [ ] **Step 6: Commit the implementation**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "fix: classify PEP 695 type positions consistently"
```

---

### Task 3: Verify Compatibility and PR Readiness

**Files:**
- Verify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Verify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Verify: `docs/superpowers/specs/2026-08-09-issue-263-pep695-type-positions-design.md`
- Verify: `docs/superpowers/plans/2026-08-09-issue-263-pep695-type-positions.md`

**Interfaces:**
- Consumes: completed Issue #263 implementation and tests.
- Produces: a clean branch ready for PR, CI, and squash merge.

- [ ] **Step 1: Run formatting and static analysis**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: both commands exit zero without warnings.

- [ ] **Step 2: Run the complete Rust and Python contract suites**

```bash
cargo test --workspace
uv run --frozen python -m unittest tests/test_skills.py
```

Expected: all tests pass; only repository-documented ignored fixtures may be
reported as ignored.

- [ ] **Step 3: Check the final diff and branch state**

```bash
git diff --check main...HEAD
git status --short
git log --oneline main..HEAD
```

Expected: no whitespace errors, no uncommitted files, and only the Issue #263
design, plan, tests, and implementation commits.

- [ ] **Step 4: Push, open the PR, and merge only after CI succeeds**

The PR body must include `Fixes #263`, the PEP 695 behavior matrix, explicit
Lean non-applicability rationale, and exact local verification commands. Push
the branch, create the PR, monitor every required check, then squash merge and
delete the remote branch. Finally fast-forward `main`, remove this worktree,
delete the local branch, and preserve unrelated untracked files in the main
checkout.
