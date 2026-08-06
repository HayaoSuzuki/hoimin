# Collection and Structural Mutation Operators Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the approved collection, structural, bitwise, and boundary mutation operators to the Python analyzer and enable all of them by default in one tested, documented PR.

**Architecture:** Keep the existing token scanner for token-local operators and add an AST visitor pass in `crates/hoimin-cli/src/analyzer/rust.rs` for calls, literals, methods, subscripts, and slices. Both passes emit `AnalyzerCandidate` values and continue through the existing selection, profile, deduplication, ordering, truncation, and candidate validation pipeline. Configuration exposes stable operator IDs plus `collection_ops`, `structure_ops`, and `bitwise_ops` selector families.

**Tech Stack:** Rust 2024, `ruff_python_ast` 0.6.2, `ruff_python_parser`, `proptest`, `cargo test`, README/development Markdown contracts.

## Global Constraints

- All 17 new runtime operators are enabled by default; type-annotation operators remain opt-in.
- `append`/`pop` and set-literal-to-`frozenset` wrapping are excluded from this release.
- Builtin call candidates are suppressed conservatively when the target name is bound anywhere in the source file.
- Method candidates are syntax-directed and do not claim receiver type inference.
- Comprehensions, assignment/delete targets, unsupported keyword/star argument forms, arbitrary index expressions, and zero-step slice mutations are excluded.
- Every structural replacement must be one contiguous source span and must parse after replacement.
- The existing `max_mutants` and `max_candidates` bounds remain unchanged.
- This is a behavior-changing PR; commits and the PR must not use `[skip ci]`.
- All specification, plan, source, tests, README/development updates, and review fixes stay in this feature worktree and one PR.

---

## File Map

- Modify `crates/hoimin-core/src/config.rs`: add the 17 operator variants, canonical names, default selection membership, and selector-family expansion.
- Modify `crates/hoimin-core/tests/operator_selection.rs`: validate family expansion and default/exclusion behavior at the core configuration boundary.
- Modify `crates/hoimin-cli/src/analyzer/rust.rs`: collect AST-based candidates, track conservative builtin bindings, add bitwise tokens, and preserve the shared candidate pipeline.
- Modify `crates/hoimin-cli/src/analyzer/rust_tests.rs`: exact analyzer candidate, source-rewrite, filtering, shadowing, and boundary tests.
- Modify `crates/hoimin-cli/tests/cli_config.rs`: CLI parsing, default operator exposure, family selectors, and exclusion contracts.
- Modify `crates/hoimin-cli/tests/run_e2e.rs`: representative real CLI mutations and default candidate/report assertions.
- Modify `crates/hoimin-cli/src/analyzer/protocol.rs` only if a protocol validation assertion needs an explicit new operator case; the existing enum lookup should remain the source of truth.
- Modify `README.md`: operator table, default count, IDs, selector families, supported forms, and exclusions.
- Modify `docs/development.md`: analyzer extension guidance and parse-preservation expectations.
- Keep `docs/superpowers/specs/2026-08-06-collection-and-structural-mutation-operators-design.md` and this plan in the same PR.

## Interfaces

The analyzer keeps the existing `AnalyzerCandidate` shape:

```rust
pub(crate) struct AnalyzerCandidate {
    pub path: Utf8PathBuf,
    pub span: ByteSpan,
    pub original: String,
    pub replacement: String,
    pub operator: String,
    pub line: u32,
    pub column: u32,
    pub symbol: Option<String>,
}
```

The AST pass consumes the existing `AnalyzeRequest<'_>` and returns
`Vec<AnalyzerCandidate>`. It must call the existing `selected(request, line,
symbol)` helper and use `MutationOperator::as_str()` for every operator ID.

---

### Task 1: Add operator IDs, defaults, and selector families

**Files:**
- Modify: `crates/hoimin-core/src/config.rs`
- Test: `crates/hoimin-core/tests/operator_selection.rs`
- Test: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**
- Produces `MutationOperator::{CollectionAnyAll, CollectionListTuple, CollectionSetFrozenset, CollectionAppendInsert, CollectionMinMax, CollectionSetAddDiscard, CollectionSetRemoveDiscard, CollectionStringStartsEnds, CollectionStringSplitRsplit, BitwiseAndOr, BitwiseShift, StructureAppendExtend, StructureMappingGetSubscript, StructureSortReverse, StructureSortedReversed, StructureIndexNeighbor, StructureSliceNeighbor}`.
- Produces canonical IDs exactly matching the design: `collection_any_all`, `collection_list_tuple`, `collection_set_frozenset`, `collection_append_insert`, `collection_min_max`, `collection_set_add_discard`, `collection_set_remove_discard`, `collection_string_starts_ends`, `collection_string_split_rsplit`, `bitwise_and_or`, `bitwise_shift`, `structure_append_extend`, `structure_mapping_get_subscript`, `structure_sort_reverse`, `structure_sorted_reversed`, `structure_index_neighbor`, `structure_slice_neighbor`.
- Produces selector families `collection_ops`, `structure_ops`, and `bitwise_ops`.

- [ ] **Step 1: Write failing core selection tests.**

Add tests that assert `MutationOperatorSelection::default()` contains every new runtime operator and no `type_*` operator, that each new ID is in `valid_names()`, and that the three families expand to the exact expected sets. Add an exclusion test proving a family exclusion removes only that family.

```rust
#[test]
fn default_runtime_selection_contains_collection_structure_and_bitwise_operators() {
    let selected = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::CollectionAnyAll,
        MutationOperator::CollectionListTuple,
        MutationOperator::StructureMappingGetSubscript,
        MutationOperator::BitwiseShift,
    ] {
        assert!(selected.contains(operator));
    }
    assert!(!selected.contains(MutationOperator::TypeNullableAdd));
}
```

- [ ] **Step 2: Run the focused tests and verify they fail for the missing variants.**

Run: `cargo test -p hoimin-core --test operator_selection default_runtime_selection_contains_collection_structure_and_bitwise_operators -- --exact`

Expected: FAIL to compile because the new enum variants and selector names do not exist.

- [ ] **Step 3: Implement the enum and selector changes.**

Add the 17 variants to `MutationOperator`, `as_str()`, and `all()`. Add them to `all_legacy()` so default runtime selection includes all 30 runtime operators (13 existing plus 17 new). Extend `parse_selector()` with the three families and ensure `valid_names()` returns both the IDs and family names in sorted order.

- [ ] **Step 4: Run core and CLI configuration tests.**

Run: `cargo test -p hoimin-core --test operator_selection`

Run: `cargo test -p hoimin-cli --test cli_config operator_flags`

Expected: PASS, including exact default/exclusion behavior.

- [ ] **Step 5: Commit the configuration boundary.**

```bash
git add crates/hoimin-core/src/config.rs crates/hoimin-core/tests/operator_selection.rs crates/hoimin-cli/tests/cli_config.rs
git commit -m "feat: add default collection mutation operators"
```

---

### Task 2: Build AST candidate helpers and conservative builtin binding facts

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Extends `AstFacts` with file-level bound-name tracking for builtin call names.
- Adds an AST candidate collector invoked by `analyze_source_cancellable` after token candidates and before type-annotation candidates.
- Reuses `LineIndex`, `selected`, scope lookup, cancellation checks, candidate ordering, deduplication, and truncation already owned by `rust.rs`.

- [ ] **Step 1: Add failing shadowing and parse-preservation tests.**

Add tests for a clean builtin call and for each binding form that suppresses the corresponding bare call: module assignment, import, function parameter, function definition, class definition, and local assignment. Add a helper in the test module that applies a candidate span/replacement and reparses the resulting source with `ruff_python_parser::parse_module`.

```rust
#[test]
fn shadowed_collection_builtins_are_not_mutated() {
    let source = "any = custom_any\ndef f(list):\n    return list(items)\n";
    let output = analyze(source);
    assert!(output.candidates.iter().all(|candidate| {
        !candidate.operator.starts_with("collection_")
    }));
}
```

- [ ] **Step 2: Run the focused tests to establish the failing baseline.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests::shadowed_collection_builtins_are_not_mutated -- --exact`

Expected: FAIL because the AST pass and binding facts do not exist yet.

- [ ] **Step 3: Implement binding collection.**

Extend the `AstFacts` visitor to record target names from imports, assignments,
annotated assignments, named expressions, loop targets, with/except targets,
function/class definitions, and all function/lambda parameters. Use a fixed
set of target builtin names (`any`, `all`, `list`, `tuple`, `set`,
`frozenset`, `min`, `max`, `sorted`, `reversed`) and suppress only the matching
bare builtin candidates when that name is present anywhere in the file.

- [ ] **Step 4: Add the shared candidate constructor.**

Create a helper that receives a source byte range, replacement text, operator,
and optional symbol; computes line/column; checks `selected`; checks operator
selection; and returns `AnalyzerCandidate`. Use checked `u64` conversions and
return `None` for invalid or empty ranges. Keep source slicing byte-based so
Unicode columns continue to use `LineIndex`.

- [ ] **Step 5: Run analyzer position, selection, and existing regression tests.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests::line_index_positions_token_and_type_annotation_candidates -- --exact`

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests::focused_profile_suppresses_main_print_assert_and_defaults -- --exact`

Expected: PASS with no changes to existing candidate positions or profile behavior.

- [ ] **Step 6: Commit the AST scaffolding.**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "refactor: prepare AST mutation candidate collection"
```

---

### Task 3: Implement collection calls, literals, and same-contract methods

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- The AST collector emits the nine `collection_*` IDs in the operator inventory.
- `collection_list_tuple` handles both constructor calls and load-context list/tuple literals.
- All candidates use one source span and preserve receiver/argument text.

- [ ] **Step 1: Write exact candidate tests for calls and literals.**

Add one table-driven test containing `any`, `all`, `list`, `tuple`, `set`,
`frozenset`, `min`, `max`, `append`, `insert`, set methods, string methods,
list literals, tuple literals, empty/singleton literals, trailing commas, and
starred literal elements. Assert `(original, replacement, operator)` tuples in
source order and assert every applied replacement reparses.

- [ ] **Step 2: Add negative-shape tests.**

Cover list/set comprehensions, tuple assignment targets, `any` with zero or
two arguments, constructors with keywords/star expansion, `insert` with a
nonzero or nonliteral index, `sort` with options, and set/string methods with
unsupported argument shapes. Assert no candidate is emitted for each excluded
form.

- [ ] **Step 3: Run the new tests and verify they fail.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests::collection -- --nocapture`

Expected: FAIL because the AST visitor does not yet emit collection candidates.

- [ ] **Step 4: Implement call and method collection.**

Visit `Expr::Call` nodes. Match bare builtin names only when the binding facts
allow them; match method attributes by exact attribute name and supported
argument shape. Build replacements by changing only the callable name for
same-contract calls, and by replacing the whole call range for
`append`/`insert`. Use `Expr::List`, `Expr::Tuple`, and their `ctx`/range data
for literal candidates, excluding comprehensions and non-load contexts.

- [ ] **Step 5: Implement list/tuple literal source rewriting.**

For list-to-tuple, replace the outer delimiters and add a comma for a singleton
element. For tuple-to-list, remove tuple parentheses when present, preserve
the element text and trailing comma, and wrap the elements in brackets. Empty
forms become `[]` and `()`. Do not reserialize nested expressions.

- [ ] **Step 6: Run the focused analyzer suite.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests`

Expected: PASS, including existing token/type tests and all new collection cases.

- [ ] **Step 7: Commit collection candidates.**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "feat: add collection call and literal mutations"
```

---

### Task 4: Implement structural call transformations

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- The AST collector emits `structure_append_extend`, `structure_mapping_get_subscript`, `structure_sort_reverse`, and `structure_sorted_reversed`.
- Structural replacements never evaluate the original receiver or argument twice.

- [ ] **Step 1: Write failing structural transformation tests.**

Add exact tests for:

```python
items.append(value)       # -> items.extend([value])
items.extend([value])     # -> items.append(value)
mapping.get(key)          # -> mapping[key]
mapping[key]              # -> mapping.get(key)
items.sort()              # -> items.reverse()
items.reverse()           # -> items.sort()
sorted(items)             # -> reversed(items)
reversed(items)           # -> sorted(items)
```

Assert unsupported defaults, multi-element `extend`, keyword forms, slices,
stores/deletes, and multi-argument `sorted/reversed` are skipped.

- [ ] **Step 2: Run the focused structural tests and verify they fail.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests::structure -- --nocapture`

Expected: FAIL because the structural visitor branches do not yet exist.

- [ ] **Step 3: Implement append/extend and sort/reverse.**

Construct `extend([value])` from the original receiver and value source for
`append(value)`. Recognize the inverse only when the argument is exactly a
singleton list literal. Restrict sort/reverse to no-argument calls and replace
the full call range while retaining the receiver.

- [ ] **Step 4: Implement mapping get/subscript.**

Recognize one-key `.get(key)` calls without defaults or star/keyword arguments
and simple receiver expressions. Recognize load-context simple-receiver
subscripts without slices. Replace only the call/subscript expression so key
source is evaluated once and the intentional `None` versus `KeyError`
semantic difference remains visible to the mutant.

- [ ] **Step 5: Implement sorted/reversed.**

Recognize exactly one positional argument with no keywords, replace the bare
call name, and retain the original argument source.

- [ ] **Step 6: Apply each replacement and reparse in tests.**

Use the shared test helper for nested receivers, parenthesized keys, strings,
comments, and calls embedded in larger expressions. Reject any replacement
that does not parse before asserting candidate equality.

- [ ] **Step 7: Run the full analyzer suite and commit.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests`

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "feat: add structural collection mutations"
```

---

### Task 5: Add bitwise and boundary mutations

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- `bitwise_and_or` and `bitwise_shift` use the existing token replacement path.
- `structure_index_neighbor` and `structure_slice_neighbor` use AST ranges and emit checked decimal-literal replacements.

- [ ] **Step 1: Write failing tests for bitwise tokens and boundary candidates.**

Assert `&`/`|` and `<<`/`>>` candidates in source order. Add subscripts with
`0`, `1`, and large decimal literals; assert `+1` and valid `-1` candidates,
no negative-literal or arbitrary-expression candidates, and no overflow.
Add slices with start/stop/step literals, empty bounds, and a step of `1` to
prove zero-step replacements are excluded.

- [ ] **Step 2: Run focused tests and verify failure.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests::bitwise_and_or -- --exact`

Expected: FAIL because the token replacement table and AST boundary pass do not yet contain these operators.

- [ ] **Step 3: Add bitwise token replacements.**

Extend `replacement()` for `&`, `|`, `<<`, and `>>`, using the two new IDs. Keep
existing string/f-string filtering and source-order behavior unchanged.

- [ ] **Step 4: Add checked index/slice literal rewriting.**

Visit load-context subscripts and slice nodes. Parse only plain decimal
integer source text, use checked arithmetic, and emit at most two candidates
per eligible bound. Preserve all non-mutated receiver, colon, and step source.

- [ ] **Step 5: Run analyzer tests and commit.**

Run: `cargo test -p hoimin-cli --lib analyzer::rust_tests`

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "feat: add bitwise and boundary mutations"
```

---

### Task 6: Wire configuration, protocol, CLI, and real-run contracts

**Files:**
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Modify: `crates/hoimin-cli/src/analyzer/protocol.rs` only if required by a failing validation test
- Test fixtures: use the existing temporary-project helpers in `run_e2e.rs`; do not add a second fixture framework.

**Interfaces:**
- CLI defaults expose all 30 runtime IDs while preserving type-operator opt-in.
- JSON candidate records carry the new canonical operator IDs and stable candidate IDs.
- Existing plan/run fingerprint behavior sees the expanded default operator list.

- [ ] **Step 1: Add failing CLI contract tests.**

Test that `--operators collection_ops`, `--operators structure_ops`, and
`--operators bitwise_ops` parse, that each family expands to the expected IDs,
that `--exclude-operators structure_ops` removes only structural IDs, and that
the help/validation output lists every new ID.

- [ ] **Step 2: Add representative real-run tests.**

Create one temporary Python project containing one candidate from each family,
run the real CLI with default operators and `--max-mutants` high enough to
retain them, parse the JSON report, and assert the expected operator IDs and
stable source spans. Add a focused run proving collection/structure operators
remain eligible under `--profile focused` outside arid spans.

- [ ] **Step 3: Run the focused integration tests and verify failure.**

Run: `cargo test -p hoimin-cli --test cli_config collection -- --nocapture`

Run: `cargo test -p hoimin-cli --test run_e2e collection -- --nocapture`

Expected: the new tests fail until defaults and end-to-end candidate plumbing are complete.

- [ ] **Step 4: Implement any protocol assertion updates.**

Only change protocol code if the focused tests identify a hard-coded operator
allowlist. Prefer `MutationOperator::from_name` so future IDs remain defined in
one configuration source.

- [ ] **Step 5: Run all CLI integration tests and commit.**

Run: `cargo test -p hoimin-cli --test cli_config`

Run: `cargo test -p hoimin-cli --test run_e2e`

```bash
git add crates/hoimin-cli/src/analyzer/protocol.rs crates/hoimin-cli/tests/cli_config.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "test: cover default collection mutation runs"
```

---

### Task 7: Update README and development documentation in the same PR

**Files:**
- Modify: `README.md`
- Modify: `docs/development.md`
- Test: `crates/hoimin-cli/tests/cli_config.rs` documentation contract assertions, if existing assertions need expanded expected strings

- [ ] **Step 1: Write documentation contract expectations.**

Extend the existing README contract test so it requires the 30 runtime IDs,
the three new families, the default count, the exclusion examples, and the
explicit exclusions for `append/pop`, comprehensions, and set literals.

- [ ] **Step 2: Update the README operator section.**

Replace the old 13-operator statement with the complete default set. Add a
compact table grouped by collection calls/literals, same-contract methods,
structural calls, bitwise operators, and boundary operators. State that
`--operators` is explicit, `--exclude-operators` removes IDs/families, and
`type_*` remains opt-in.

- [ ] **Step 3: Update `docs/development.md`.**

Document the AST candidate pass, conservative builtin shadowing, one-span
replacement rule, parse-preservation test helper, and the command sequence for
focused analyzer tests. Include the exact supported shapes for structural and
boundary operators.

- [ ] **Step 4: Run documentation and whitespace checks.**

Run: `cargo test -p hoimin-cli --test cli_config readme_documents_all_mutation_operator_ids_and_selector_families -- --exact`

Run: `git diff --check`

- [ ] **Step 5: Commit documentation with the feature branch.**

```bash
git add README.md docs/development.md crates/hoimin-cli/tests/cli_config.rs
git commit -m "docs: document collection and structural operators"
```

---

### Task 8: Full verification, review, and single PR handoff

**Files:**
- Verify all modified files in the worktree; do not modify unrelated user files.

- [ ] **Step 1: Run formatting and lint checks.**

Run: `cargo fmt --all -- --check`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: both commands exit 0.

- [ ] **Step 2: Run the complete Rust test suite.**

Run: `cargo test --workspace`

Expected: all non-ignored tests pass. The worktree `.venv` symlink may be used
for controlled Python fixtures but must not be staged.

- [ ] **Step 3: Run Python contract tests and diff checks.**

Run: `uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v`

Run: `git diff --check`

- [ ] **Step 4: Review the complete diff against `origin/main`.**

Run: `git diff --stat origin/main...HEAD`

Run: `git status -sb`

Confirm only the intended source, tests, documentation, specification, and
plan files are tracked; preserve `.venv` as an untracked symlink and preserve
the user's unrelated untracked files.

- [ ] **Step 5: Request code review and address findings.**

Use the repository review process on the complete branch. Re-run the focused
tests for every review fix, then repeat the full verification commands before
claiming completion.

- [ ] **Step 6: Push and create one PR.**

```bash
git push -u origin feat/collection-mutation-operators
gh pr create --base main --head feat/collection-mutation-operators \
  --title "feat: add default collection and structural mutation operators" \
  --body "Implements the approved collection, structural, bitwise, and boundary mutation operators. Includes the design specification, implementation plan, analyzer tests, CLI contracts, README, and development documentation in one PR."
```

The PR must report the exact verification commands and explain that all new
runtime operators are default-enabled while `append/pop` and set-literal
wrapping remain intentionally excluded.
