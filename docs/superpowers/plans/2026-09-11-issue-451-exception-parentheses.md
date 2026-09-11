# Issue #451 Implementation Plan

> Execute task-by-task using the executing-plans workflow; autonomous implementation and PR creation are authorized by the user.

**Goal:** Remove parenthesized exception expressions without changing the intended mutation.
**Architecture:** Resolve the syntactic expression span through existing Ruff token ranges before deletion.
**Tech Stack:** Rust 2024, Rust 1.88 minimum, Ruff parser, CPython.
**Spec:** [Exception parentheses design](../specs/2026-09-11-issue-451-exception-parentheses-design.md)

## Constraints

No added dependencies; preserve builtin resolution, final-handler restriction and except-star exclusion. Keep source outside each candidate span intact. Use a dedicated worktree and branch; create a PR targeting main.

## Task 1: Regression and implementation

Files: `crates/hoimin-cli/src/analyzer/rust_tests.rs`, `crates/hoimin-cli/src/analyzer/rust.rs`.

- [x] Add literal cases: `except ((Exception))` deletes `((Exception))`; `((ValueError), (TypeError))` becomes `( (TypeError))` or `((ValueError) )`. Add multiline comments and trailing commas, reparse all candidates.
- [x] Run `cargo test -p hoimin-cli --lib parenthesized_exception` and confirm the old deletion spans fail.
- [x] Replace the name range used by bare-handler deletion and the element range used by tuple removal with:

```rust
ruff_python_ast::token::parenthesized_range(
    expression.into(), parent.into(), facts.tokens.expect("parser tokens are set"),
).unwrap_or_else(|| expression.range())
```

- [x] Run `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests`.

## Task 2: Observable behavior and delivery

Files: `crates/hoimin-cli/tests/operator_function_contracts.rs`, `crates/hoimin-cli/tests/run_e2e.rs`, the analyzer OKF concept and design index.

- [x] Execute generated candidates with CPython: bare handlers catch ValueError and KeyboardInterrupt; removing ValueError retains TypeError and removing TypeError retains ValueError. Use the existing plan and Python harness helpers.
- [x] Add a real run with only `exception_exception_to_bare`: a ValueError-catching test must report one survived mutant and zero killed mutants.
- [x] Run focused contracts/E2E, workspace tests, contract-feature tests, formatting and Clippy; record failures and platform limits accurately.
- [x] Validate OKF YAML, source footnotes, links and source hashes; record three reviews for implementation, tests and PR in this plan.
- [ ] Commit, push the Issue branch, and create a PR with `Fixes #451`, validation evidence and OKF references.

## Plan self-review

1. Spec coverage: both deletion sites have exact-span tests; runtime expectations cover the silent catch-set defect.
2. Execution details: existing CPython harness avoids a new test framework; tests precede production edits.
3. Boundaries: baseline analyzer tests passed (154 passed, 2 ignored); production parsing and unrelated operators stay unchanged. IDE MCP has no hoimin project open, so local Cargo checks provide diagnostics.

## OKF self-review

1. Claims: kept the new rule explicitly a design pending execution; historical audit results remain scoped to their original revisions.
2. Provenance: linked the actual design and recorded its current SHA-256 as untracked; no fabricated verification metadata.
3. Navigation and prose: updated the existing analyzer concept and design index, preserving existing metadata and separating deleted internal comments from retained external comments. YAML and link execution checks follow after final edits.

## Implementation self-review

1. Span correctness: checked Ruff's `parentheses_iterator` against both parent nodes. Tuple commas stop optional-parenthesis matching; supported tuples have at least two simple names, so the tuple delimiter is not removed.
2. Rust integration: followed existing expression-range call sites, reused token facts, kept fallbacks and all eligibility checks. No new allocations except the existing replacement string.
3. Preservation: traced preceding/following-comma selection, last-member behavior, comments and nested grouping. Added an internal-member-comment fixture after independent review found that coverage gap. Independent reviewer reported no actionable implementation findings.

## Test self-review

1. Sensitivity: the new unit test failed on the old code with actual `Exception` versus expected `((Exception))`; the implementation change made it pass. Exact replacement assertions catch the malformed tuple case.
2. Oracle independence: CPython results explicitly check retained and excluded exception classes. Initial test assumed plan order matched discovery order; replaced that assumption with the exact set of observed catch outcomes, retaining the assertion of two distinct candidates.
3. End-to-end scope: the real CLI pipeline reports one survived mutant, zero killed, complete=true, exit 1. Python 3.14.7; focused semantic and E2E tests both passed. Internal comments and unchanged source ranges are covered by exact text and reparsing.

## Final validation

- `cargo test --workspace --all-features`: PASSED (1612 passed, 13 ignored), including both packages' contracts features and existing Lean corpus adapters.
- `cargo fmt --all -- --check`: PASSED.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PASSED.
- Standalone CLI binary, original Issue fixture: baseline Exit(0), one survived mutant, complete=true, exit 1.
- OKF: 16 pages passed parsed YAML/reserved-file checks, source-footnote matching, 612 local-link checks and new design source hashes. Historical source hashes were not re-attested.
- No Lean modules changed or rebuilt. Executed on macOS arm64 with CPython 3.14.7; Windows/Linux execution left to CI.

## PR self-review

1. Scope: diff contains the two deletion fixes, behavior regressions and requested documents only. Dedicated branch is based on origin/main 4adf809; local Python environment symlink is excluded from staging.
2. Evidence: compared every validation claim with the command output; recorded the initially failing regression, successful CLI binary result, ignored tests and platform limits.
3. Reviewer navigation: PR names the triggering grouped expressions and resulting catch behavior, links Issue #451, records OKF scope and uses the repository template. No merge is requested or performed.
