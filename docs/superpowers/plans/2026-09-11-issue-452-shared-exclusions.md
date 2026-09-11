# Issue #452 Implementation Plan

> Execute task-by-task with the executing-plans workflow; autonomous work through PR is authorized.

**Goal:** Every automatically selected target is eligible for worker copying under built-in exclusions.
**Architecture:** Shared crate-private copy policy used by filesystem discovery, selector validation and manifest walks.
**Tech Stack:** Rust 2024, MSRV 1.88, ignore walker, existing Cargo integration tests.
**Spec:** [Shared exclusions](../specs/2026-09-11-issue-452-shared-exclusions-design.md)

## Constraints

No new dependencies or cache copying. Preserve user excludes over includes and target hidden defaults. Use the normalized relative path for direct-selector checks. Separate worktree and main-based PR.

## Task 1: Shared policy

Files: new `crates/hoimin-cli/src/copy_policy.rs`; modify `lib.rs`, `target/fs.rs`, `target/mod.rs`, `workspace/manifest.rs`; tests in `tests/target_handler.rs`.

- [x] Create real files `calc.py`, `venv/dep.py`, nested env and cache entries. Assert exact discovery of calc.py with and without includes; assert file/line selectors fail with the excluded path and remediation. Test non-Git .gitignore handling and root named venv.
- [x] Run target regressions and observe old behavior failing.
- [x] Extract the existing predicate and connect all four walks:

```rust
pub(crate) fn default_excluded(entry: &ignore::DirEntry) -> bool {
    entry.depth() != 0 && excluded_name(entry.file_name())
}
```

Use one `excluded_name(&OsStr)` comparison over the existing names (ASCII case-insensitive on Windows, exact elsewhere); `excluded_path(&Path)` checks components with that same predicate. Normalize explicit paths using `hoimin_core::normalize_logical_path` before checking.

- [x] Run target and workspace integration tests.

## Task 2: Public consistency and documentation

Files: `crates/hoimin-cli/tests/plan.rs`, README, selection OKF and design index.

- [x] Create plan with `--source .` for calc.py and venv/dep.py. Assert only calc.py is recorded. Verify its candidate and run directly with a no-op test command; expect complete=true, one survived, baseline Exit(0). Explicit excluded file fails before baseline.
- [x] Correct README include semantics and describe direct-selector remediation.
- [x] Run workspace tests with contracts, Clippy, formatting, and OKF YAML/link/source checks.
- [x] Complete three implementation, test and PR self-reviews.
- [ ] Commit, push and create PR fixing #452.

## Plan self-review

1. Coverage: tests exercise both discovery walks, selector errors, non-Git ignore rules and public execution paths.
2. Interfaces: policy is crate-private, uses OsStr/Path and existing ignore types; normalization stays in the existing core helper.
3. Cost and sequencing: failed regression first, shared target cache reused sequentially to limit disk use, no environment changes committed.

## OKF self-review

1. Updated existing selection concept, keeping old plan-version observations scoped to their sources.
2. Added the new design as an untracked source with its actual hash; no verification metadata invented.
3. Design remains reachable through the index and concept; final YAML, links and source checks will run after edits.

## Implementation self-review

1. Policy ownership: all four walks import the same entry predicate; direct selectors use the same excluded-name comparison after core path normalization. No duplicated directory list remains.
2. Boundaries: preserves the root depth exception, ignores excluded descendants at any depth and prevents includes from restoring them. Non-Git .gitignore now agrees with copying; ordinary hidden targets require includes.
3. Portability: independent review found that Windows selector spelling could disagree with actual directory spelling. Confirmed core resolves Windows paths without case sensitivity; made the shared policy ignore ASCII case on Windows, added both spelling directions to regression coverage, and documented the deliberate tightening for uppercase cache names. Removed an unused import after extraction.

## Test self-review

1. Sensitivity: old code failed two regressions, admitting venv/cache sources and ignored.py in a non-Git root. The updated discovery and workspace suites passed.
2. Public behavior: plan/run/verify test compares exact candidate path/count, baseline Exit(0), one survived result and complete=true; excluded direct-file test asserts no plan stdout and no baseline side effect.
3. Boundary coverage: root named venv, nested env, include restoration, exclude precedence, hidden files, normalized absolute file paths and line selectors have real filesystem tests. Windows-only mixed-spelling branches are not executed locally and remain a CI verification limit.

## Final validation

- `cargo test --workspace --all-features`: 1615 passed, 0 failed, 13 ignored after the Windows policy revision. Both packages' contract features and existing Lean corpus adapters included.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`, `git diff --check`: passed.
- OKF: 16 pages, 309 source-footnote pairs, 611 local links checked; all pages reachable, all designs indexed and new design source hashes matched.
- macOS arm64 with CPython 3.14.7; Windows-specific case branches not executed. No Lean modules changed or rebuilt. IDE MCP has no hoimin project open.

## PR self-review

1. Scope: compared the final diff to Issue acceptance conditions; source scans, includes, direct selectors and public plan/run/verify are covered. Shared predicate retains the root exception and documents the deliberate Windows case rule.
2. Evidence: final checks include the revised Windows policy; distinguish locally executed Unix behavior from Windows test coverage awaiting CI. Independent reviewer confirmed the previous finding resolved.
3. Delivery: branch forks from main 4adf809, includes source/tests/README/OKF/design/plan only; exclude the Python symlink from staging. PR body uses the repository template and fixes #452.
