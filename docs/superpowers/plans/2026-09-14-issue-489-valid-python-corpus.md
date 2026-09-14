# Valid Python corpus implementation plan

> Execute inline with executing-plans. No further agents.

**Goal:** Detect missing/excess/invalid candidates across four producer paths using valid CPython3.14 input.
**Architecture:** Lean-owned fixture/rule corpus → direct analyzer and public plan → shared validator, independent source positions and CPython compile/runtime.
**Tech Stack:** Lean 4.32.2, Rust, CPython3.14, existing bounded CI guard.
**Spec:** ../specs/2026-09-14-issue-489-valid-python-corpus-design.md

## Constraints

Do not add production analyzer code or hand-edit generated corpus. Mode claims require public observations. Enumerate uncovered combinations. Two Cargo jobs; Lean commands serialized with 30s/2GiB guard.

## Task 1: Model and generated fixtures

- [x] Add `ValidPythonModel.lean` reusing scope/span imports, generic visibility and finite semantic key equality. Start broken witnesses red; repair rule definitions and prove shadow suppression, key collision exclusion and one-span composition.
- [x] Add `ValidPythonAuditMain.lean` cases with site anchors, original/replacement, eligibility from model rules, producer/position/binding metadata and optional runtime outputs. Generate JSONL only through the executable.
- [x] Add registration for all55 canonical operators, with explicit deferred reasons for operators outside the initial representative subset. Emit pair-coverage missing entries separately from unsupported syntax.
- [x] Run serial guarded build, output, freshness and sensitivity. Wire generator and module order into CI and workflow contract test.

## Task 2: Real correspondence

- [x] Add a test-only direct analyzer module adapter using the existing rust_analyzer.rs pattern; no production export.
- [x] For each row: compile original with CPython3.14 first; find each unique declared anchor in source bytes; compare exact output sites/replacements; create CandidateDescriptor and call validate_candidate; independently count Python physical lines and Unicode columns; apply one replacement and compile.
- [x] Pass identical source/operators/profile/line/symbol/cap to public CLI plan. Compare validated candidate identities and truncation with the direct analyzer; exact expected site sets detect shared missing/excess behavior.
- [x] Render layout and selection variants deterministically from the fixture seed, retaining all expected values from the Lean row. Verify runtime output or declared exception for representative builtin/import/protocol/walrus cases.
- [x] Test adapter sensitivity by supplying missing, extra, damaged-span and stale-hash candidates and invalid source; make infrastructure failures distinct from semantic mismatches. Run the same original fixture against deliberately broken Lean eligibility/byte rules.

## Task 3: Evidence and delivery

- [x] Run bounded model gates, new Rust correspondence, related scope/span integrations, format/Clippy, CI workflow tests and OKF checks.
- [x] Record three self-reviews each for OKF, design, plan, model/implementation, tests and PR; record actual counts, modes, scope limits and resource stats.
- [x] Commit/push an independent branch and create a PR using the template (delivery step after final checks).

Publication verified: https://github.com/tokyogas-tech/hoimin/pull/542. Current applicable CI checks passed before this documentation-only delivery-state update.
