# Negative neighbor implementation plan

> Execute inline with superpowers:executing-plans; the assignment prohibits further agents.

**Goal:** Generate correct negative index/slice neighbors and detect a tail off-by-one through verify.
**Architecture:** Retain existing collector/span/suppression logic; extend decimal extraction and bounded arithmetic only.
**Tech stack:** Rust, Ruff AST, CPython 3.14, public CLI integration tests.
**Spec:** ../specs/2026-09-14-issue-471-negative-neighbors-design.md

## Global constraints

Preserve unsigned candidates/order and u64 magnitude bound. Exclude zero steps in both directions. CARGO_BUILD_JOBS=2; no concurrent Lean invocation.

## Task 1: Analyzer contract

Files: crates/hoimin-cli/src/analyzer/rust.rs and rust_tests.rs.

- [x] Update positive regression expectations for the negative lines already present; add table-driven negative positions, excluded contexts, spellings, limits and multiline cases. Expected -1 index: [("-1", "0"), ("-1", "-2")]; -1 step: [("-1", "-2")].
- [x] Run `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib structure_` and observe missing-negative-candidate assertion failures.
- [x] Change extraction to `Option<(i128, bool)>`: match `Expr::UnaryOp(unary) if unary.op == UnaryOp::USub`, parse the number operand text as u64, and negate its i128 magnitude. Compute neighbors `[value + 1, value - 1]`, retain values within the magnitude range (unsigned lower bound zero), and filter zero when `excludes_zero`.
- [x] Run targeted tests and full analyzer tests.

## Task 2: Public plan/verify regression and documentation

Files: crates/hoimin-cli/tests/negative_neighbors.rs, README.md, affected OKF indexes and analyzer concept.

- [x] Create a temporary project, generate plan with only boundary operators, apply every saved candidate to original bytes, and invoke CPython `compile(source, '<mutant>', 'exec')`. Require CPython 3.14, using HOIMIN_OPERATOR_TEST_PYTHON or the repository .venv; missing/wrong interpreters fail explicitly.
- [x] Generate separate boundary and unary-sign plans for `def tail(x): return x[-1]`; test `tail([7, 7, 9, 7]) == 7`. Verify all candidates: boundary report has one killed and one survived; unary sign has one survived. Assert baseline success and original source preservation.
- [x] Run the public integration test, formatting, clippy, and CLI test suite appropriate to the change.
- [x] Document precise accepted spelling, bounds, signed-zero policy and excluded zero step in README; finalize report, source hashes and OKF indexes; validate YAML, links, source identities and index completeness.
- [x] Perform three actual implementation and test review passes and record findings and fixes.
Publishing: commit/push and create the PR using the repository template after the checks above.
