# Typed session mode implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Normalize session intent into a single valid state.
**Architecture:** Keep raw CLI inputs; normalize to SessionMode once and match the mode at execution.
**Tech Stack:** Rust, serde, proptest, Python fixture checks.
**Spec:** ../specs/2026-10-15-issue-896-typed-session-design.md

## Global constraints

Separate branch for #896; no backward compatibility adapters. AAA tests, one concern per function. Five self-review passes per stage. Cargo jobs1 and dedicated target; periodically clean after builds finish.

## Review focus

Raw resume without session rejects; plan conversion disables session; normalized legacy inputs reject; path normalization retains mode; metrics collision sees both Fresh and Resume paths.

## Tasks

- [x] Baseline: `cargo test -p hoimin-core --test config_json --test plan_config`.
- [x] Red: add normalized-input legacy resume rejection regression, run it and observe failure.
- [x] Add SessionMode and normalized RunConfig changes in core/config.rs; migrate core/machine.rs to mode matching.
- [x] Update shell, metrics_destination and normalized-config consumers; retain raw CLI fields.
- [x] Add property tests for mode/path roundtrip and raw normalization; split independent assertions/tests.
- [x] Migrate current-schema JSON fixtures, update format documentation and explicit legacy rejection tests.
- [x] Run core tests, workspace tests, all-feature clippy, fmt and affected Python/doc checks; record results and five implementation/verification review passes.
- [x] Prepare the implementation, documentation and PR content for publication.
- [ ] Publish branch/create PR closing #896 and watch CI with spaced snapshots; operational follow-up is recorded on the PR and parent thread.

Plan reviews: ①Every Issue completion condition maps to a task. ②Raw and normalized configuration signatures differ intentionally. ③No rewrite of resume outcome or session DB schema is planned. ④Property tests generate safe nonempty paths and test one invariant each. ⑤Full workspace and fixture verification catches normalized-config changes outside core.
