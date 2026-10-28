# Inheritance operators implementation plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship default-on override_method_delete and super_call_move with opt-out.
**Architecture:** A shared conservative same-module class/method index and pure
source builders, integrated into the existing statement collector.
**Tech stack:** Rust, Ruff AST/tokens, existing name resolution and CPython tests.
**Spec:** ../specs/2026-10-29-inheritance-operators-design.md and the issue-1123 report.

## Global constraints

One shared index only when selected; no target imports/runtime MRO discovery.
Same-module top-level ordinary single inheritance; exclude dynamic identity and
descriptor/signature hazards. Preserve source trivia and historical receipts.
Default-on IDs, individual exclusion, 104 total IDs / 85 defaults.

## Review focus

Rebound/aliased classes and method writes must suppress uncertain candidates.
Shadowed super, extra delegation and nested/suspending code must be excluded.
Docstrings and multiline/semicolon/Unicode/newline source must remain valid.
Selectors precede replacement work; caps/cancellation/payload leases must hold.
Saved plans must preserve exact IDs, restore source and expose real assertion gaps.

## Task 1: Shared index and override deletion

- [x] Write failing tests for registration, positive deletion and boundary cases.
- [x] Observe missing-feature failures, then add index/collector and registration.
- [x] Verify source preservation, rebinding/escape/method/signature exclusions,
  selectors, caps, cancellation and bounded shared-index work.

## Task 2: Delegation movement and delivery

- [x] Add failing movement tests, implement source-slot movement and builtin proof.
- [x] Add public default/opt-out, CPython and saved-plan behavioral tests for both.
- [x] Update fixed catalogs/ranking, usage/reference and implementation evidence.
- [x] Run appropriate full workspace/quality checks, independent review, commit
  and update existing PR #1135. Record limitations and actual results.

## Execution ledger

The user explicitly requested both remaining evaluated-useful mutations after
clarifying the prior incomplete scope. Execute inline in the existing PR worktree;
the existing research design supplies the approved restrictions. No new approval loop.

Verification: 15 focused analyzer tests; public CLI5 on CPython3.14.9/3.15.1;
workspace3898 passed/32 ignored; all-target/all-feature Clippy, formatting and
generated CLI reference check pass. Independent review findings were fixed with
regressions; historical receipts remain unchanged.
