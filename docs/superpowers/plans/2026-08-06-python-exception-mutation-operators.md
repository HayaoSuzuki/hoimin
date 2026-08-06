# Python Exception Mutation Operators Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe default Python `except`-type mutations plus explicit-only risky bare, BaseException-boundary, and tuple mutations, with complete CLI contracts, tests, and documentation.

**Architecture:** Extend the existing Rust AST candidate collector with an `ExceptHandler` visitor. A new conservative exception-binding fact set suppresses candidates for shadowed names; safe name replacements use curated pairs and risky structural operators use token-aware source rewrites. Core configuration owns canonical IDs, defaults, and selector families; all candidates continue through the existing filtering, ordering, cancellation, and JSON paths.

**Tech Stack:** Rust workspace (`hoimin-core`, `hoimin-cli`), Ruff Python AST/parser 0.6.2, Rust unit/integration tests, Python unittest contract tests, Markdown documentation.

## Global Constraints

- The implementation mutates `except` clauses only; `raise` mutation is a later issue.
- `exception_type_pair` is the only exception operator in the runtime default.
- `exception_risky` is explicit-only and contains bare, BaseException-boundary, and tuple operators.
- `SystemExit`, `KeyboardInterrupt`, and `GeneratorExit` are never generated as individual replacement targets.
- Safe candidates support only simple unqualified names; `except*`, qualified names, dynamic expressions, and unsupported tuple members are skipped.
- Candidate replacements must preserve source trivia and reparse as Python in tests.
- Production traversal must not add whole-source reparsing per candidate.
- Behavior-changing commits must not use `[skip ci]`; only a pure documentation-only commit may use it.
- Every task ends with a focused test command and a separate commit.

## File Map

- `crates/hoimin-core/src/config.rs`: canonical exception operator IDs, defaults, and selector families.
- `crates/hoimin-core/tests/operator_selection.rs`: selection/default/selector contract tests.
- `crates/hoimin-cli/src/analyzer/rust.rs`: exception binding facts, AST visitor, safe pair candidates, risky source rewrites.
- `crates/hoimin-cli/src/analyzer/rust_tests.rs`: candidate spans, exclusions, parseability, filtering, and cancellation tests.
- `crates/hoimin-cli/tests/cli_config.rs`: CLI help, canonical-name, default, and explicit selector contracts.
- `README.md`: public operator catalog, defaults, risky opt-ins, examples, and exclusions.
- `docs/development.md`: analyzer architecture, curated pair policy, shadowing policy, and test workflow.
- `docs/superpowers/specs/2026-08-06-python-exception-mutation-operators-design.md`: approved design (already committed).

---

### Task 1: Add exception operator IDs, defaults, and selectors

**Files:**
- Modify: `crates/hoimin-core/src/config.rs:MutationOperator`, `MutationOperatorSelection::valid_names`, `all_legacy`, and `parse_selector`
- Test: `crates/hoimin-core/tests/operator_selection.rs`
- Test: `crates/hoimin-cli/tests/cli_config.rs` for the runtime default count/catalog assertions

**Interfaces:**
- Produces canonical IDs: `exception_type_pair`, `exception_bare_to_exception`, `exception_exception_to_bare`, `exception_base_boundary`, `exception_tuple_add_pair`, and `exception_tuple_remove_member`.
- Produces selector families `exception_ops` (safe default operator only) and `exception_risky` (the five explicit-only operators).
- Keeps `all_legacy()` runtime defaults backward-compatible and adds only `ExceptionTypePair`.

- [ ] **Step 1: Write failing core selection tests**

Add assertions that `valid_names()` contains all six IDs and both selectors; `parse_selector("exception_ops")` returns only `ExceptionTypePair`; `parse_selector("exception_risky")` returns exactly the five risky IDs; and `MutationOperatorSelection::default()` contains `ExceptionTypePair` but none of the risky IDs.

```rust
assert_eq!(MutationOperatorSelection::parse_selector("exception_ops").unwrap(), vec![
    MutationOperator::ExceptionTypePair,
]);
assert!(!MutationOperatorSelection::default().contains(MutationOperator::ExceptionBaseBoundary));
```

- [ ] **Step 2: Run the focused tests and verify they fail**

Run: `cargo test -p hoimin-core --test operator_selection exception -q`

Expected: FAIL to compile because the exception enum variants and selectors do not exist.

- [ ] **Step 3: Implement the core IDs and selection behavior**

Add the six enum variants, `as_str` names, and `all()` entries. Extend `valid_names()` with `exception_ops` and `exception_risky`. Add selector branches:

```rust
"exception_ops" => Ok(vec![MutationOperator::ExceptionTypePair]),
"exception_risky" => Ok(vec![
    MutationOperator::ExceptionBareToException,
    MutationOperator::ExceptionExceptionToBare,
    MutationOperator::ExceptionBaseBoundary,
    MutationOperator::ExceptionTupleAddPair,
    MutationOperator::ExceptionTupleRemoveMember,
]),
```

Insert only `ExceptionTypePair` into `all_legacy()` so normal defaults remain safe.

- [ ] **Step 4: Run core and CLI selection tests**

Run: `cargo test -p hoimin-core --test operator_selection exception -q`

Run: `cargo test -p hoimin-cli --test cli_config collection_operator -q`

Expected: all focused tests pass, including default catalog assertions updated from 30 to 31 runtime operators.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-core/src/config.rs crates/hoimin-core/tests/operator_selection.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: add exception mutation operator selection"
```

---

### Task 2: Track shadowed exception names in AST facts

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs:AstFacts`, `record_builtin_name`, `record_import_alias`, and `MUTABLE_BUILTINS`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Adds `AstFacts::bound_exception_names: HashSet<String>` and `AstFacts::is_exception_bound(&self, name: &str) -> bool`.
- Adds `EXCEPTION_NAMES` containing every curated name plus `Exception`, `BaseException`, `SystemExit`, `KeyboardInterrupt`, and `GeneratorExit`.
- Existing binding traversal calls record both mutable collection names and exception names, including assignments, parameters, imports, comprehensions, match captures, aliases, and `except ... as name` targets.

- [ ] **Step 1: Write failing binding tests**

Add a fixture containing an assignment, parameter, import, star import, match capture, and exception alias for exception names. Assert `AstFacts::is_exception_bound` is true for each affected name and false for a clean source:

```rust
let parsed = parse_module("ValueError = Custom\n").unwrap();
let facts = super::AstFacts::from_module(parsed.syntax(), parsed.tokens());
assert!(facts.is_exception_bound("ValueError"));
```

- [ ] **Step 2: Run the binding tests and verify failure**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_binding -q`

Expected: FAIL because `is_exception_bound` and the exception fact set are absent.

- [ ] **Step 3: Implement conservative exception binding facts**

Add the set and method, and make the existing `record_builtin_name` call `record_exception_name` as well. Extend star-import handling with `EXCEPTION_NAMES`, preserving the existing conservative all-builtins behavior. Do not add scope-sensitive inference; any file-wide possible binding suppresses that exception name.

- [ ] **Step 4: Run binding and existing analyzer tests**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_binding -q`

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::shadowed_collection_builtins_are_not_mutated_as_calls -q`

Expected: all selected tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "feat: track shadowed exception names"
```

---

### Task 3: Implement safe curated exception pair candidates

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs:AstCandidateCollector` visitor and helper functions
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Adds `AstCandidateCollector::collect_exception_handler(&mut self, handler: &ruff_python_ast::ExceptHandlerExceptHandler)`.
- Adds `exception_pair_replacements(name: &str) -> &'static [&'static str]`, returning the curated counterpart list for a simple name. `KeyError` returns both `IndexError` and `AttributeError`; other names return one counterpart or an empty slice.
- Adds `visit_except_handler` to the AST visitor, calling the collector and then `visitor::walk_except_handler` so nested bodies remain traversed.

- [ ] **Step 1: Write failing safe-candidate tests**

Add a source fixture with each curated name in an `except` clause, including a `KeyError` case that must emit both curated replacements. Assert exact `(original, replacement, operator)` records and source order. Include `except (ValueError, TypeError)`, `except module.Error`, `except make_error()`, and `except* ValueError` and assert none produce `exception_type_pair` candidates.

```rust
let output = analyze("try:\n    work()\nexcept ValueError:\n    pass\n");
assert!(output.candidates.iter().any(|candidate| {
    candidate.original == "ValueError"
        && candidate.replacement == "TypeError"
        && candidate.operator == "exception_type_pair"
}));
```

- [ ] **Step 2: Run the safe-candidate tests and verify failure**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_type_pair -q`

Expected: FAIL because no exception visitor or operator candidate exists.

- [ ] **Step 3: Implement syntax-directed safe candidates**

In `collect_exception_handler`, require `handler.type_` to be `Expr::Name`, reject `facts.is_exception_bound(name)`, and call `exception_pair_replacements`. For each replacement, call the existing `add_candidate(name.range(), replacement, MutationOperator::ExceptionTypePair)`. The collector must not inspect tuple members in this task. Register the visitor method and retain body traversal.

- [ ] **Step 4: Run reparse and filtering tests**

Extend the fixture loop to apply every `exception_type_pair` candidate with the existing `apply_candidate_and_reparse` helper. Add line, symbol, and candidate-limit assertions using the existing analyzer request helpers.

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_type_pair -q`

Expected: all safe candidates reparse and all selection filters remain correct.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "feat: add curated exception type mutations"
```

---

### Task 4: Add explicit-only bare, BaseException, and tuple mutations

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs:AstCandidateCollector` exception helpers
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Adds `collect_risky_exception_handler` gated by the five risky operator selections.
- Adds token-aware helpers with concrete roles:
  - `bare_exception_replacement(handler) -> Option<(TextRange, String)>` for bare handlers;
  - `base_exception_boundary_replacement(type_name) -> Option<&'static str>`;
  - `tuple_exception_replacements(source, tuple, tokens, operator) -> Vec<(TextRange, String)>` for add/remove.
- All structural replacements pass through `add_candidate` and preserve parseable delimiters, comments, and line endings.

- [ ] **Step 1: Write failing risky-selection and parseability tests**

Add tests proving default analysis emits no risky candidates, while an explicit `MutationOperatorSelection` containing each risky operator emits the intended candidates for:

```python
try:
    work()
except:
    pass
except Exception:
    pass
except (ValueError, TypeError):
    pass
```

Assert both bare directions, both BaseException boundary directions, tuple add/remove candidates, exact source spans, and successful `parse_module` after applying every candidate. Add comments and trailing commas inside tuple fixtures.

- [ ] **Step 2: Run risky tests and verify failure**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_risky -q`

Expected: FAIL because risky operators and rewrites are not implemented.

- [ ] **Step 3: Implement bare and BaseException boundary rewrites**

For bare handlers, replace only the `except` keyword span with `except Exception` for `ExceptionBareToException`. For `except Exception`, replace only the handler type name with an empty type using a token-aware span for `ExceptionExceptionToBare`, preserving the colon and comments. For `ExceptionBaseBoundary`, replace only a simple unshadowed `Exception` or `BaseException` name with its counterpart.

- [ ] **Step 4: Implement tuple add/remove rewrites**

Accept only `Expr::Tuple` handlers whose elements are all simple, unshadowed `Expr::Name` values from `EXCEPTION_NAMES` and whose tuple has at least two elements for removal. For add, generate one candidate per missing curated counterpart; for remove, generate one candidate per removable member while keeping at least one member. Use parser token ranges to place commas outside nested parentheses and preserve existing comments/trivia. Skip dynamic or qualified members even when the risky operator is selected.

- [ ] **Step 5: Run the complete analyzer test module**

Run: `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests -q`

Expected: all existing and new analyzer tests pass, including cancellation and candidate ordering.

- [ ] **Step 6: Commit**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "feat: add explicit risky exception mutations"
```

---

### Task 5: Complete CLI contracts and documentation

**Files:**
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `README.md`
- Modify: `docs/development.md`

**Interfaces:**
- CLI help lists all six canonical exception IDs and both exception selectors.
- README documents 31 runtime defaults, safe `exception_type_pair`, explicit `exception_risky`, curated pairs, unsupported shapes, and BaseException warnings.
- Development documentation explains the `ExceptHandler` AST pass, conservative shadowing, token-aware tuple rewrites, and focused test commands.

- [ ] **Step 1: Add failing CLI/documentation contract assertions**

Update the existing catalog tests to require all six IDs and both selectors, require the default to contain `exception_type_pair` but no risky ID, and require the README/development text to mention `exception_risky`, `BaseException`, `except*`, and the curated-pair policy.

- [ ] **Step 2: Run focused contract tests and verify failure**

Run: `cargo test -p hoimin-cli --test cli_config exception -q`

Expected: FAIL until help, README, and development documentation are updated.

- [ ] **Step 3: Update help/catalog documentation**

Update the existing operator catalog and examples without changing unrelated operator descriptions. Explicitly state that risky operators require `--operators exception_risky` or individual IDs and are not enabled by default.

- [ ] **Step 4: Run focused contract tests**

Run: `cargo test -p hoimin-cli --test cli_config exception -q`

Expected: PASS with canonical names, selectors, defaults, and documentation contracts.

- [ ] **Step 5: Commit documentation and contracts**

```bash
git add crates/hoimin-cli/tests/cli_config.rs crates/hoimin-core/tests/operator_selection.rs README.md docs/development.md
git commit -m "docs: document Python exception mutations"
```

---

### Task 6: Final verification, review, and integration

**Files:**
- Modify: `.superpowers/sdd/2026-08-06-python-exception-mutation-operators/progress.md` (ignored implementation ledger)
- Create: `.superpowers/sdd/2026-08-06-python-exception-mutation-operators/final-review.diff` (ignored review artifact)

- [ ] **Step 1: Run formatting and static checks**

Run: `cargo fmt --all -- --check`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: both commands exit 0.

- [ ] **Step 2: Run the complete local test suites**

Run: `cargo test --workspace`

Run: `uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v`

Expected: zero failures. Record exact pass/ignore counts in the ledger.

- [ ] **Step 3: Request independent code review**

Generate the review package from `origin/main` to the feature HEAD and verify that the reviewer reports no Critical or Important findings. Fix and retest any parseability, selection, shadowing, or documentation issue before proceeding.

- [ ] **Step 4: Push and create one PR**

Push `feat/exception-mutation-operators`, create a single PR against `main`, and include the design/plan/docs in that PR. Do not use `[skip ci]` on behavior-changing commits.

- [ ] **Step 5: Monitor CI and merge**

Wait for all available required checks. The self-hosted-only hard cgroup job may remain skipped; any actual failure must be investigated. Merge the PR after required checks pass, then verify `origin/main` contains the merge commit and preserve the worktree for post-merge inspection.
