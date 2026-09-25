# Excluded Git path validation implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline. Root provides independent review and owns CI/merge; publish using gh stack.

**Goal:** Keep excluded Git filenames from failing changed-only selection without weakening selected-path validation.

**Architecture:** Centralize optional resolved-path membership ahead of portable validation across patch, binary numstat and current-worktree names. Keep parser framing and unscoped validation intact.

**Tech Stack:** Rust/Tokio, real Git, public CLI plan fixtures, existing Lean-generated changed-selection adapters.

**Spec:** docs/superpowers/specs/2026-09-25-issue-621-excluded-git-paths-design.md

## Constraints and review focus

One issue per branch, second level on 612 head 3b456ff. Shared batch-analyzer Cargo cache with one job and no debug/incremental data. No Git command/pathspec performance changes (620), no portable path relaxation, no non-UTF-8 contract expansion. No new Lean commands needed.

Review Windows separator collisions; rename direction and binary records; empty versus absent eligible scope; selected invalid names versus excluded invalid names; patch body/header isolation and existing CR/context translation.

## Task 1: meaningful RED

- [ ] Reuse the freshly validated parent baseline; add public plan tests in a dedicated excluded Git paths integration file using isolated repositories and controlled Git config.
- [ ] Cover tracked/untracked/unborn-indexed states and excluded .txt/.py names. Compare plain-plan candidate identity with changed-plan results and require one selected boolean candidate.
- [ ] Add selected invalid-name negative controls, excluded binary names, rename from excluded unsupported name to selected valid destination, literal-backslash/slash collision, and empty resolved scope. Observe the issue regression failing before production edits, retaining logs.

## Task 2: implementation and focused checks

- [ ] Add a private Git path scope helper in target/git.rs (or adjacent paths module if this keeps the file focused) from optional eligible target keys, with raw-backslash membership guard.
- [ ] Pass scope through patch header and binary numstat decoders, preserving quote decoding, prefixes, /dev/null and framing. Filter current-worktree names before portable validation; preserve existing reads/intersections.
- [ ] Test absent/empty scopes, scoped literal-backslash collisions, ordinary Windows case behavior, parser malformed-input behavior and rename direction. Run new public tests plus existing parser/target and all changed-selection oracle tests.
- [ ] Perform three implementation reviews: identity/platform boundaries; all ingress/caller flows; parser/rename/IO compatibility. Record findings and fixes.

## Task 3: validation and publication

- [ ] Perform three test reviews: RED premises and actual Git states; positive/negative coverage independent of helper implementation; bounded isolated subprocesses and candidate identity comparisons.
- [ ] Run full workspace, exact workspace and vendor Clippy gates, workspace/vendor fmt, and diff whitespace checks. Do not add Python tests or mutation campaigns because no Python production changes are planned.
- [ ] Obtain independent root review; write report including limitations and exact evidence. Commit implementation/tests/report separately from this design/plan commit.
- [ ] Publish via gh stack as the second meaningful level, or rebase onto main excluding merged parent if 612 merges first. Root owns CI polling/merge/cleanup.

## Plan reviews before code

1. Regression fidelity: direct parser unit tests alone cannot show discovery has excluded the bad name. Public plan tests must exercise real source/exclude configuration and compare candidate identity to the ordinary plan.
2. Scope completeness: add .py as well as reported .txt, plus binary and rename ingress; preserve standalone rejection tests. Keep malformed quote/UTF-8 expectations unchanged rather than silently broadening the task.
3. Operational and stack review: reuse parent validation for baseline, capture new RED before production, use assigned single Cargo lane. 612/632 integration tests protect row/context behavior; new Lean predicate duplication would not cover the filesystem/parser risk. Wait for parent merge before adding a third target stack level.
