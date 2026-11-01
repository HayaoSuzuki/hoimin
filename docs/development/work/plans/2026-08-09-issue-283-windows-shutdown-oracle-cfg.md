# Issue #283 Windows Shutdown Oracle CFG Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the shutdown oracle test target clippy-clean on Windows without reducing cross-platform corpus validation.

**Architecture:** Gate only the real Unix process/signal adapter and its private dependencies with `cfg(unix)`. Leave data-model parsing and platform-independent tests available on every target.

**Tech Stack:** Rust 2024, Cargo clippy, Tokio integration tests, GitHub Actions Windows runner.

## Global Constraints

- Do not use blanket `allow(dead_code)` or `allow(unused_imports)` attributes.
- Do not enable the Unix process fixture on Windows.
- Preserve cross-platform corpus parsing and validation tests.

---

### Task 1: Gate Unix-only adapter dependencies

**Files:**

- Modify: `crates/hoimin-cli/tests/lean_shutdown_oracle.rs`

**Interfaces:**

- Consumes: existing `cfg(unix)` strict oracle entry point.
- Produces: a Windows test target containing only cross-platform helpers and tests.

- [ ] **Step 1: Record RED**

Use Windows quality job 93180427490 from run 31288084479. Expected failure:
unused imports and dead code in `lean_shutdown_oracle.rs` under `-D warnings`.

- [ ] **Step 2: Add narrow cfg attributes**

Split Unix-only standard-library and Tokio imports behind `#[cfg(unix)]`.
Apply the same attribute to `REVIEWED_MISMATCHES`, `CaseClass`, `CaseResult`,
`FixturePaths` and its impl, `FixtureProcesses`, `write_parallel_project`,
`repo_root`, `python_executable`, `spawn_scenario`, `observe_report`, and
`observe_session`. Keep `OracleCase`, `CliObservation`, parsing, validation,
projection, and their tests unconditional.

- [ ] **Step 3: Verify Unix behavior**

```bash
cargo fmt --check
cargo clippy -p hoimin-cli --test lean_shutdown_oracle --all-features -- -D warnings
cargo test -p hoimin-cli --test lean_shutdown_oracle
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 4: Verify Windows and integrate**

Push the branch, create a PR with `Fixes #283`, and require the Windows quality
job to pass before squash-merging. Then update PR #282 onto the repaired main.
