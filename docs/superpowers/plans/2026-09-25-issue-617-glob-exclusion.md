# Issue 617 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans task by task.

**Goal:** Keep files that discovery correctly retained under glob exclusion.
**Architecture:** Discovery owns filtering; core consumes its inventory without interpreting glob strings.
**Tech Stack:** Rust, ignore/globset existing dependency, Lean 4.
**Spec:** docs/superpowers/specs/2026-09-25-issue-617-glob-exclusion-design.md

## Global Constraints

Dedicated worktree; commit design/plan first; three actual reviews for design, plan, implementation and tests. Share only the root-owned Cargo cache, never active build processes. Serialize Lean with 20-second/2048-MiB guard.

## Review Focus

Core API responsibility, source/file/line composition, exclude/include precedence, escaped glob controls, original public run behavior and oracle sensitivity.

## Tasks

- Add a core regression for a discovered bracket-named file under source/file/line selectors; observe RED.
- Remove raw exclude equality from core target resolver and document prefiltered discovery. Update direct exclusion fixture and preserve discovery tests; run GREEN.
- Integrate GlobSelectionModel and generator, generated 20-case corpus, library/lake/CI/Python registry, and public discovery/core/plan adapter. Add public run assertion for the issue's false-empty result.
- Verify proofs/freshness/sensitivity with resource guards, run relevant target/plan/core tests, full workspace, exact CI Clippy/fmt and workflow tests.
- Record implementation/test review rounds and independent review, commit and create PR; merge after CI, then remove worktree.

## Plan self-review

1. RED test uses existing public types and resolver, so it fails on the lost file rather than a missing API field.
2. Positive and negative cases cover glob character classes, escaped brackets, literal exclusion and excluded explicit paths; stage observations distinguish discovery errors from core errors.
3. The existing finite audit supplies expectations; Rust does not implement a duplicate glob oracle. Keep CI executable order synchronized with lakefile and its closed Python registry, based on the earlier CI registration finding.
