### Spec Compliance

- ✅ Spec compliant for Task 8 (#977), base `787fc4f31d4d8d115386303425f789750fbcfad5` → head `8d58c121a8121e2eec87f1b54d48cd9cb0140a63`. `crates/hoimin-cli/src/analyzer/rust.rs:3016` reuses `selected` with the expression's start line and scope; the condition guard at `:3031` and container guard at `:3995` precede replacement generation. The three changed files match the brief; no compatibility API, schema conversion, byte budget, or unrelated production change appears.
- ✅ Construction work is measured at completed string builders before admission (`crates/hoimin-cli/src/analyzer/rust.rs:3072`; `crates/hoimin-cli/src/analyzer/rust/container_elements.rs:78`), with a test-only thread-local counter (`crates/hoimin-cli/src/analyzer/rust/whole_expression_selection_tests.rs:17`). The dedicated tests separately register excluded line/symbol, multiline interior line, selected child, selected identity/order/dedup, count truncation, and cancellation scenarios; the two properties use 24 bounded cases each.
- ✅ The required distinct self-review passes are recorded in each stage (`task-8-report.md:5`, `:13`, `:62`, `:85`, `:115`), with five numbered entries per stage. AAA helpers and individually named finite cases keep each test scenario independently registered.
- ⚠️ Scope limitation: this task's diff cannot establish whole-branch native test argv, error precedence, or resource ownership across the other tasks. No relevant interface or ownership change is present here; the controller's integrated verification remains responsible for those global claims. No Windows-host or release memory/performance result is established by this review.

### Strengths

- The new helper uses the same span/scope anchor as admission (`crates/hoimin-cli/src/analyzer/rust.rs:2940`, `:2971`, `:623`, `:3016`) and preserves the existing line/symbol conjunction (`:737`). It centralizes geometry without adding a second selection policy.
- Exclusion guards only generation. The container branch falls through to the remaining mutation work and child walk (`crates/hoimin-cli/src/analyzer/rust.rs:4011`, `:4062`); condition exclusion returns from its emission helper while the statement visitor continues (`:3744`, `:3906`, `:3938`). Exact child construction counts distinguish preserved walking from silently pruned subtrees.
- Literal expected replacements supplement comparison to unrestricted analyzer output; full candidate equality covers identity fields, with explicit stable-ID, cap-one, and post-construction cancellation controls (`crates/hoimin-cli/src/analyzer/rust/whole_expression_selection_tests.rs:1`). Instrumentation is compiled only in tests, and counts completed payload bytes rather than admissions or RSS.

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- None found.

#### Minor (Nice to Have)

- None found.

### Checks and evidence

- Read the supplied diff once and compared it with `task-8-brief.md`, its supplement, the detailed issue `/tmp/hoimin-design-audit-5d3aee95/impl-977.json`, and `task-8-report.md`. No additional diff, codebase crawl, Git operation, Cargo invocation, or test rerun was performed.
- Named risk — preselection differs from admission: checked only `make_candidate`, `selected`, `add_candidate`, and `add_candidate_in_scope` (`crates/hoimin-cli/src/analyzer/rust.rs:608`, `:737`, `:2940`, `:2963`). Both changed operators submit their expression range as the scope anchor; the helper uses that same start. No mismatch found.
- Named risk — excluded parents prune children or alter cancellation control flow: the supplied hunks cut off their enclosing functions, so checked the relevant condition call site and visitor continuation (`crates/hoimin-cli/src/analyzer/rust.rs:3737`, `:3904`, `:3973`, `:4054`). Both child walks and existing cancellation checks remain outside the new generation guards. No pruning found.
- Checked RED evidence (`/tmp/task8-red.log:5`, `:220`): six controls passed and ten failures report actual completed-build/count mismatches, including zero-work exclusions and child-only work. Expected RED failures and proptest shrink output are intentional; the report documents removal of the generated regression artifact (`task-8-report.md:50`).
- Checked GREEN evidence (`/tmp/task8-green.log:23`): 16 passed. Checked focused results (`/tmp/task8-focused.log:11`, `:21`, `:32`, `:375`; `/tmp/task8-selection.log:8`, `:16`): 4 + 4 + 5 + 334 + 1 + 2 = 350 passed, with exactly three named existing benchmark ignores (`/tmp/task8-focused.log:148`). The alternate-root integration run includes all sixteen new cases (`:353`). Depth, semantic/scope, existing cancellation, and bounded discovery tests appear in the successful focused log. No unexpected warning or failure was found in successful-run logs.
- Checked mandatory hook evidence (`/tmp/task8-commit.log:1`): workspace fmt, vendored-parser fmt, workspace Clippy, and vendored-parser Clippy all passed; `:5` records the requested head commit. These are recorded executions, not independently rerun validations.

### Assessment

**Task quality:** Approved.

**Reasoning:** The scoped production change moves the existing selection decision ahead of the expensive builders while preserving admission and traversal. Builder-level regression tests and the recorded focused runs substantiate the intended work reduction and unchanged selected output; no blocking or minor defect was identified.
