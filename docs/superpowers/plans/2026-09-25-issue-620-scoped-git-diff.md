# Scoped tracked Git diff implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline. Root arranges independent review and owns CI/merge. Publish through gh stack.

**Goal:** Avoid unrelated tracked patch bodies for explicitly selected ordinary changed targets while preserving Git rename semantics.

**Architecture:** Global NUL name/status inventory chooses legacy rename fallback or bounded literal patch/numstat batches. Existing parsers, binary exclusions, physical-line translation and untracked collectors remain authoritative.

**Tech Stack:** Rust/Tokio, Git subprocesses, public CLI integration tests, retained release performance artifacts.

**Spec:** docs/superpowers/specs/2026-09-25-issue-620-scoped-git-diff-design.md

## Constraints and review focus

One issue/worktree based on main99c27dc. Batch-analyzer Cargo cache, one job, no dev/test debug or incremental builds. No production changes before this commit. No new Lean execution planned; ask root before any Lean command if that changes.

Cover source/destination rename competition; literal pathspec metacharacters; Windows path spelling and argv bytes; empty scope with invalid repository/base; selected binary/deleted files and changed-context ranges.

## Task 1: baseline and public RED

- [x] Build retained baseline release and run issue's original 36-plan 4/16-MiB script; preserve all raw manifests and RSS/timing observations under /tmp/hoimin-batch-604-632/620-perf.
- [x] Add bounded public CLI subprocess test with a real-Git forwarding wrapper recording actual diff stdout bytes. Use fixed two-file fixtures at two unrelated CSV sizes and require identical candidate identity with small patch/numstat output.
- [x] Run this test before production edits and record observed oversized output as meaningful RED. Keep plain-plan/actual Git fixture premises separate from the resource assertion.

## Task 2: scoped command planning and compatibility

- [x] Add private inventory parser/planner adjacent to target/git.rs: strict NUL framing/statuses, raw path decoding through existing scope, selected rename/copy fallback, sorted deduplicated non-rename paths, batches bounded to 128 paths and8192 bytes.
- [x] Add scoped tracked dispatcher using original base/merge-base/metadata options. Use --literal-pathspecs, --no-renames and -- separators for each patch/numstat batch; unchanged legacy flow for unscoped/selected rename or oversized path. Empty metadata selection still validates Git/base.
- [x] Preserve parse_diff/numstat and read/physical-row/intersection behavior. Do not optimize discovery or change portable path/IO error contracts.
- [x] Add public resolver differential cases for into/out-of-scope and unrelated renames, binary/deletion, context, diff-base, unborn/untracked, literal names and batching; pure parser tests cover malformed inventory and bounds.
- [x] Run RED test to GREEN and all related target/oracle tests. Perform three actual implementation reviews: global pairing invariants; path/argv/error boundaries; downstream selection/coordinate semantics.

## Task 3: empirical evidence and completion

- [x] Build retained optimized release and rerun original measurement script; compare all candidate identity fields across matched manifests and report actual patch bytes, sampled Hoimin RSS and time medians, including noise/cost limitations.
- [x] Perform three test reviews: independent real Git premises/byte counts; semantic cross-product and legacy fallback; fixture isolation/timeouts and performance claim limits.
- [x] Run full workspace and exact workspace/vendor Clippy and fmt gates, diff whitespace; request independent root review. Existing Lean-generated changed-selection suites run as Rust public tests; no duplicate formal model or Python production mutation campaign.
- [ ] Record review/evidence report, commit implementation/tests separately, integrate newer main only as needed with related checks, publish using gh stack and hand off PR/head. Root owns CI/merge/cleanup.

## Plan reviews before code

1. Resource regression must observe production Git output, not just assert constructed argv. The forwarding wrapper runs real Git and records byte lengths; the original unwrapped release probe supplies independent empirical memory/time evidence.
2. Compatibility requires both fallback and optimized branches. Compare public scoped selection to the unchanged unscoped resolver, preserve adversarial rename fixtures and binary/delete/context behavior, and test literal names plus batch limits separately.
3. Operational review: retain baseline before editing, bound child commands and artifacts, use assigned cache, and avoid unnecessary new Lean modeling of external Git. Commit design/plan first, record three reviews at each stage, then obtain independent review before publication.
