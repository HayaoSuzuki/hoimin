# Session companion admission implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Reject shared SQLite companion files before any session connection can modify their referents.

**Architecture:** Add a descriptor-based preflight to session/schema.rs, reusing SqliteCompanions, open_regular_file, file_link_count and SchemaError. Preserve existing schema admission and normal resume.

**Tech Stack:** Rust, rusqlite, existing native file-link helpers.

**Spec:** [Approved task scope and design](../specs/2026-10-23-issue-1094-session-companions-design.md).

## Global Constraints

No dependency/schema/CLI additions or compatibility readers. Preserve bytes and permissions on rejection. Use two Cargo jobs and private temporary roots. Include the existing audit in the PR; do not merge.

## Review Focus

Missing database with a shared companion; empty/current DB; foreign/unsupported DB checked before SQLite side effects; ordinary single-link companions/resume; FIFO/directory or inaccessible companion must not hang or silently bypass inspection.

### Task 1: Admission and source-protection regression

- [x] Add native fixtures in session/schema.rs covering three companion suffixes and missing/empty/current/foreign/unsupported DBs; assert byte/permission/artifact preservation and typed rejection.
- [x] Add public CLI source-alias cases in tests/session_hardlinks.rs for inside/outside project, including normal controls.
- [x] Run focused RED before implementation; record actual source modification or premature side effects.
- [x] Add minimal pre-connection regular-file/link-count validation; preserve ordinary session tests.
- [x] Run focused GREEN, review error paths, then full workspace and contracts checks as appropriate.
- [x] Update session contract, compatibility, source catalogs, maintenance evidence and audit follow-up; validate documentation.
- [x] Obtain fresh read-only branch review.

Delivery: use standard commit hooks, explicit same-name remote push and a PR closing #1094. The actual commit/PR and CI status belong in the delivery report.
