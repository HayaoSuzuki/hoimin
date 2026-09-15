# Issue #548 Implementation Plan

> **For agentic workers:** Execute locally in the assigned worktree; user authorized autonomous execution and parent prohibits further delegation.

**Goal:** Collection annotation mutations require actual builtin provenance.
**Architecture:** Reuse lexical scope facts with annotation-aware deferred lookup and gate only concrete collection pairs.
**Tech Stack:** Rust, Lean 4, CPython 3.14.
**Spec:** ../specs/2026-09-15-issue-548-annotation-builtins-design.md

## Constraints

Only issue-548 worktree; no new dependencies; both mutation directions; five substantive self-review rounds at each stage. Parent performs push and PR creation.

- [x] Add Lean proof, broken witness and generated scope fixtures; verify corpus freshness.
- [x] Add public CLI adapter and CPython3.14 annotation evaluation; capture failing regression before production edits.
- [x] Register annotation resolution sites; resolve lexical/class/type parameter provenance; gate collection pairs.
- [x] Run focused debug/release regression, existing annotation/type-parameter tests, format and lint.
- [x] Update OKF concept and source catalogs, verify YAML/links/hashes, review PR body, commit and hand off.
