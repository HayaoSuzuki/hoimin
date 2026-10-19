### Spec Compliance

- ✅ Spec compliant for Task 1 at `e5ce0c6d`, against base `7110f2ef`. The requested owner and public API are present (`crates/hoimin-budget-math/src/lib.rs:6`, `src/duration.rs:10`, `src/projection.rs:28`); the projection is a 100% rename, and auto-timeout arithmetic matches the removed body (`src/duration.rs:46`). No adapter, duplicate production arithmetic, proof-only implementation, or new arithmetic restriction appears in this task.
- ✅ Optional serde adds implementations and attributes only (`crates/hoimin-budget-math/Cargo.toml:14`, `src/duration.rs:6`); core enables it and preserves named conversion errors (`crates/hoimin-core/Cargo.toml:17`, `src/config.rs:632`, `src/config.rs:791`). Serialization tests pin the existing Duration and enum representations and reject zero (`crates/hoimin-budget-math/tests/serialization.rs:6`).
- ✅ Independent bounded arithmetic properties, individually named finite cases, full-width inputs, saturation, nanosecond carry, and manually constructed shortfall inputs are covered (`crates/hoimin-budget-math/tests/arithmetic.rs:10`, `tests/arithmetic.rs:49`, `tests/arithmetic.rs:143`). The corrected smallest-covering-wave property and division-guarded capacity oracle meet the brief's correction; their u128 intermediate bounds are sound for the required 64-bit targets.
- ✅ Workspace/consumer dependencies, both locks, release version rewriting, required SBOM identity and generator cleanup, and actual sdist member checks are updated (`Cargo.toml:2`, `Cargo.lock:627`, `fuzz/Cargo.lock:254`, `tools/release.py:203`, `tools/sbom.py:118`, `tools/capture_sbom.py:25`, `tests/test_sdist.py:40`). The integration-conditional files need no artificial changes; actual sdist evidence supports leaving `pyproject.toml` unchanged.
- ✅ Five implementation and five testing reviews, including findings and fixes, are durable (`docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md:7`, `:15`). Baseline, structural API RED, Python behavioral RED, final tests, lint, and commit evidence are recorded in the task report.

### Strengths

- `crates/hoimin-budget-math/src/duration.rs:7`: the private tuple field and public Option constructor preserve the positive-duration invariant while keeping configuration-specific errors in core.
- `crates/hoimin-budget-math/src/projection.rs:37`: extracting the unchanged production calculation preserves overflow behavior and warning-only projection semantics; `is_shortfall` still reads only the two documented fields (`:22`).
- `crates/hoimin-budget-math/tests/arithmetic.rs:1`: mathematical expectations avoid repeating production ceiling arithmetic or overflowing their own product; boundary cases execute separately and serde coverage is feature-gated.
- `tools/capture_sbom.py:25` and `tests/test_sbom.py:350`: integration includes generated-sidecar cleanup and negative identity validation, rather than only adding the crate to manifests.
- `.superpowers/sdd/2026-10-19-issue-999-creusot-adoption/task-1-evidence/oracle-followup2.log:42`, `:54`: final follow-up evidence shows 29 arithmetic and 6 serialization tests passing. Existing logs independently total 133 baseline tests, 197 focused core tests, 29 no-serde tests, and 2916 workspace tests with zero failures; packaging evidence reports 124 passing tests. Final check/lint/hook/test logs inspected have no warning diagnostics. No passing tests were rerun during this review.

### Issues

#### Critical (Must Fix)

- None.

#### Important (Should Fix)

- None.

#### Minor (Nice to Have)

- None.

### Cannot verify

- `deny.toml:23`: the new exception matches the existing narrow internal-crate policy, but an actual cargo-deny run remains unavailable in Task 1 evidence. Task 5's final dependency audit must run the real policy checks; this is not a Task 1 blocker under the assigned ownership.
- The exact compiler version used by the recorded stable workspace check is not printed in `task-1-evidence/check.log:141`. Final acceptance should retain explicit Rust 1.99.0 version evidence. Creusot installation, contracts/proofs, cross-target generation, CI limits, and final acceptance belong to Tasks 2–5 and are not approved by this review.
- The diff package elided the entire unchanged renamed projection body. Inspected `crates/hoimin-budget-math/src/projection.rs:1` directly once because that body was necessary to assess the public API and arithmetic; tool-output truncation also required reading only the missing middle portion of the supplied diff. No other changed file was reread.
- Concrete external-integration risk checked: stale references to the removed core module/API in tools, formal inputs, Python tests, CI, fuzz, and the Python crate. One focused `rg` search found no old module path or qualified old API references there; no broader repository crawl was performed.

### Assessment

**Task quality:** Approved.

**Reasoning:** The extraction keeps production arithmetic intact, preserves configuration and serialization behavior, and supplies independent mathematical tests plus the necessary crate/distribution integration. Recorded verification supports Task 1; the explicitly assigned later-task proof and dependency acceptance gates remain open.
