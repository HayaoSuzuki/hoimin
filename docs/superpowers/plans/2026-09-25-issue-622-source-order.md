# Issue 622 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to execute task by task.

**Goal:** Prevent stale conclusive verdict reuse after source-root precedence changes.
**Architecture:** Add an ordered, framed source-root field to fingerprint schema 9; preserve worker behavior.
**Tech Stack:** Rust, SQLite, Lean 4, public CLI tests.
**Spec:** docs/superpowers/specs/2026-09-25-issue-622-source-order-design.md

## Global Constraints

Dedicated issue worktree. Commit design and plan before implementation. Three actual review rounds per stage with evidence. Serialize Lean runs with 20-second/2048-MiB guard and 10000-heartbeat proof limits. Root owns publication.

## Review Focus

Ordered versus unordered data, field framing, schema compatibility, same-order reuse, fresh-run equivalence and real SQLite/public CLI execution.

## Tasks

- Add core `resume_policy` tests through `FingerprintInput::from_config` for AB/BA, unchanged order and path framing; observe RED before product changes.
- Add `source_roots` in `core/src/resume.rs`, encode as field 12, bump schema to 9, update structural fixtures and version expectations, run core tests GREEN.
- Promote `/tmp/hoimin-round9-source-order/SourceOrder.lean` into named formal model/generator with eight generated cases and a broken unordered-source control. Register library, executable and guarded CI checks.
- Add Rust public-run adapter with two identical modules, max-mutants 1 and isolated session DB per case. Assert Lean expectations for status, reuse, execution, termination, baseline and incomplete exit; compare changed-order resume with a fresh run.
- Update README/development fingerprint contract, run workspace tests, exact CI Clippy, fmt and diff checks; record three implementation and test reviews and independent review, commit, then publish PR.

## Plan self-review

1. Sequence review: core tests can exercise the old public constructor without referencing the new field, allowing a behavioral RED rather than a compilation failure.
2. Coverage review: use both order-flip directions and unchanged controls; import-root cases ensure existing precedence protection remains intact. Keep SQLite and real test subprocesses in the adapter.
3. Resource review: reuse a lane-specific Cargo target; each Lean command is separately bounded and runs alone. The model enumerates eight fixed cases rather than arbitrary trace search. Generated expectations are never hand-edited.
