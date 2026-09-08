# Python Operator Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Discover mutations for Python 3.13 operator syntax and trusted standard-library operator functions.

**Architecture:** Extend existing token allowlists and the bounded AST producer. Keep conservative operator import resolution and callable replacement mapping in a focused analyzer module, separate from the existing builtin resolver.

**Tech Stack:** Rust, Ruff Python AST, CPython executable fixtures, Cargo.

**Spec:** `docs/superpowers/specs/2026-09-08-python-operator-coverage.md`

## Global Constraints

- Work only in `.worktrees/python-operator-coverage`; preserve dirty main files.
- Commit implementation, tests, and design/verification documentation together.
- Do not launch Windows Actions or add automatically triggered Windows jobs.
- Avoid repeated GitHub API polling; use a 180-second check interval after push.
- Use test-driven development, meaningful external Python behavior assertions,
  and independent code review before PR completion. Do not mutate test modules.
- Keep generated mutation candidates inside the existing bounded AST producer.
- Do not add a dependency or change Python's supported version floor.
- PR prose describes changes and verification only. Do not merge the PR.

---

### Task 1: Native operator syntax

**Files:**
- Modify: `crates/hoimin-core/src/config.rs`
- Modify: `crates/hoimin-core/tests/operator_selection.rs`
- Modify: `crates/hoimin-cli/src/plan/ranking.rs`
- Test: `crates/hoimin-cli/src/plan/ranking_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `docs/development.md`

**Interfaces:**
- Consumes: existing `replacement(&str, bool)` and AST token allowlists.
- Produces: nine syntax variants in `MutationOperator`, with exact names from the spec; default runtime count becomes 42 before Task 2.

- [ ] Add a table-driven analyzer test using each source below, selecting the exact operator ID from the spec and asserting exactly one candidate, original/replacement text and parseability:

```python
def calculate(a, b):
    return a ** b
```

Repeat for `@`, `^`, unary `~a`, and each augmented spelling in the spec. Check source slices against candidate spans. Add negative decorator, `**kwargs`, annotation, string and comment examples. Exercise line/symbol selection and a candidate cap of one using mixed new syntax.

- [ ] Run `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo test -p hoimin-cli --lib analyzer::rust::rust_tests` and record the expected missing-selector failures.
- [ ] Add the nine enum variants, serialization names, default/all catalogs and bitwise family members. Extend arithmetic ranking. Add token mappings, e.g.:

```rust
"**" => Some(("*", "binary_power")),
"@" => Some(("*", "binary_matmul")),
"~" => Some(("+", "bitwise_invert")),
```

Expand only the BinOp, UnaryOp and AugAssign AST allowlists with their corresponding spellings. Update explicit selector/count expectations and document new syntax mappings.
- [ ] Run analyzer tests, core operator-selection tests, CLI configuration tests, and `cargo fmt --all -- --check` with the dedicated target directory. Record actual results.
- [ ] Self-review and commit only Task 1 paths with `feat: cover missing Python operator syntax`.

### Task 2: Imported operator callables

**Files:**
- Create: `crates/hoimin-cli/src/analyzer/rust/operator_functions.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Modify: `crates/hoimin-core/src/config.rs`
- Modify: `crates/hoimin-cli/src/plan/ranking.rs`
- Test: `crates/hoimin-cli/src/plan/ranking_tests.rs`
- Modify: `crates/hoimin-core/tests/operator_selection.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `docs/development.md`

**Interfaces:**
- Consumes: Task 1 catalog and existing `AstCandidateCollector::add_candidate` filtering/bounding.
- Produces: `MutationOperator::OperatorFunction` (`operator_function`), bringing runtime count to 43, and a focused `OperatorImports` index with callable replacement lookup integrated into the AST collector. Final concrete signatures belong to this module and its sole caller.

- [ ] Write analyzer tests before implementation for every function mapping and dunder alias listed in the spec, qualified and from-import aliases, and higher-order references:

```python
import operator as op
from operator import add as plus
def calculate(a, b):
    return op.add(a, b), plus(a, b)
def combine(values):
    return map(op.add, values, values)
```

Assert exact source/replacement spans, parseability, operator ID and candidate count. Include negative tests for rebinding, function parameters, local/conditional/relative imports, wildcard imports, exception/match bindings, comprehensions, walrus, global/nonlocal, module attribute assignment/deletion and dynamic namespace writes. Assert that an imported module whose name is shadowed does not produce candidates even if an unrelated attribute is named `add`.
- [ ] Run the focused new tests and record missing functionality failures.
- [ ] Implement the catalog and conservative binding analysis in the new module. A unique unconditional module import is trusted only if no other binding anywhere in the module affects its bound name. Account for AST binding sites not represented as Store names (parameters, aliases, function/class names, exception targets, pattern captures and type parameters). Namespace mutation uncertainty invalidates identity conservatively. Pair replacements preserve callable argument evaluation by editing the callable reference only; lambda replacements use positional-only parameters. Suppress generated `__import__` use if that builtin is shadowed or namespace identity is uncertain.
- [ ] Integrate via the existing bounded AST producer, skip annotation spans and non-Load contexts, and preserve cancellation checks. Add `operator_function` to default/all catalogs and ranking. Do not refactor existing audited builtin resolution.
- [ ] Run analyzer/core/CLI selection tests, add bounded prefix, profile, line/symbol and annotation regression tests for function candidates, and document the mapping and conservative exclusions in `docs/development.md`.
- [ ] Self-review and commit Task 2 with `feat: mutate trusted operator module callables`.

### Task 3: Executable contracts and delivery documentation

**Files:**
- Modify: `README.md`
- Test: `crates/hoimin-cli/tests/cli_config.rs` (README inventory contract)
- Create: `crates/hoimin-cli/tests/operator_function_contracts.rs`
- Create: `docs/superpowers/reports/2026-09-08-python-operator-coverage.md`
- Modify: `docs/superpowers/plans/2026-09-08-python-operator-coverage.md`

**Interfaces:**
- Consumes: CLI analyzer output and all operator IDs from Tasks 1 and 2.
- Produces: executable regression coverage and verification record; no new production interface.

- [x] Add a separate Rust integration test file following the existing analyzer CLI fixture conventions; do not edit dirty main `run_e2e.rs`. Generate actual mutants from CLI analysis and run the mutated source with CPython. Verify non-equivalence with explicit expectations rather than merely parsing:

```python
events = []
def operand(value):
    events.append(value)
    return value
```

Check `pow(2, 3)` becomes multiplication (6), matrix-multiplication custom protocol dispatch changes to `__mul__`, bitwise/invert and augmented protocol effects, `contains` keeps container/item order, `setitem`/`delitem` no longer modify the target, `call` evaluates arguments but does not invoke the target, and from-import/dunder/higher-order replacements execute. Use positional-only argument failures and side-effect event lists to expose duplicated/reordered evaluation. Default to the controlled `.venv` interpreter; accept `HOIMIN_OPERATOR_TEST_PYTHON` only in this test helper so the controller can also execute the contracts on installed Python 3.13 without changing project dependencies.
- [x] Run the focused integration suite and full Rust workspace tests, format and clippy. Run relevant repository Python tests if available. Production Python is unchanged: do not mutation-test fixture/test modules.
- [x] Update README's runtime count to 43, syntax/function inventory and conservative import restrictions. Record exact commands, counts, local platform, skipped checks, conservative scope and final review outcome in the report. Commit docs and tests.
- [ ] Obtain independent whole-branch review, fix findings and rerun affected tests. Push only the feature branch, create a PR with changes/verification prose, and observe final-head checks at 180-second intervals without dispatching Windows workflows.
