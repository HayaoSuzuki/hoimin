# Real-Binary Stream Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Verify that the compiled `hoimin` binary keeps successful command output and JSON reports on stdout while routing diagnostics exclusively to stderr.

**Architecture:** Add process-boundary integration tests that invoke `env!("CARGO_BIN_EXE_hoimin")` directly. Keep help/version coverage beside CLI parsing tests and place the invalid-source run beside the existing end-to-end fixture coverage, parsing each stream independently so a report/diagnostic mix-up cannot pass.

**Tech Stack:** Rust, `std::process::Command`, Tokio process tests, `serde_json`, the checked-in basic Python fixture.

## Global Constraints

- Change only `crates/hoimin-cli/tests/cli_config.rs`, `crates/hoimin-cli/tests/run_e2e.rs`, and this plan.
- Do not add or change a production API.
- Spawn `env!("CARGO_BIN_EXE_hoimin")`; do not substitute `run_with_io` for the process boundary.
- Require help and version to succeed with non-empty stdout and empty stderr.
- Require the invalid-syntax JSON run to exit `4`, emit exactly one newline-terminated report with `summary.complete == false` on stdout, and emit the `analyzer.invalid_syntax` JSON diagnostic on stderr.
- Assert that diagnostic text is absent from stdout and that no report object containing `summary` appears on stderr.

---

### Task 1: Verify Real Help and Version Stream Routing

**Files:**
- Modify: `crates/hoimin-cli/tests/cli_config.rs`

**Interfaces:**
- Consumes: `env!("CARGO_BIN_EXE_hoimin")`, `--help`, and `--version`.
- Produces: assertions over the real process exit status, stdout, and stderr.

- [ ] **Step 1: Add the real-binary test**

Add `real_binary_help_and_version_use_stdout` and invoke the compiled binary once for each argument. Assert success, non-empty stdout, and empty stderr for both invocations.

- [ ] **Step 2: Prove the stream assertion is load-bearing**

Temporarily mutate the real CLI successful-display routing from stdout to stderr, run `cargo test -p hoimin-cli --test cli_config real_binary`, and observe this test fail because stdout is empty or stderr is non-empty. Restore the production file immediately.

- [ ] **Step 3: Verify the restored behavior**

Run `cargo test -p hoimin-cli --test cli_config real_binary` and require a pass.

### Task 2: Verify Real JSON Report and Diagnostic Stream Separation

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: `env!("CARGO_BIN_EXE_hoimin")`, the basic fixture root, its controlled Python interpreter, and an invalid `src/calc.py` written into a temporary copy.
- Produces: independently parsed stdout report and stderr diagnostic assertions.

- [ ] **Step 1: Add the real invalid-syntax run test**

Add `real_binary_json_report_and_diagnostic_use_separate_streams`. Prepare a temporary fixture project whose selected Python source is syntactically invalid, then invoke `hoimin run --format json` through the compiled binary with the controlled fixture test command.

- [ ] **Step 2: Assert the stdout report contract**

Assert exit code `4`, exactly one trailing newline, no additional JSON document, a parseable report whose `summary.complete` is `false`, and absence of `analyzer.invalid_syntax` from stdout.

- [ ] **Step 3: Assert the stderr diagnostic contract**

Parse every non-empty stderr line as JSON, locate a record with `code == "analyzer.invalid_syntax"`, and assert that no stderr record contains the `summary` report field.

- [ ] **Step 4: Prove stream separation is load-bearing**

Temporarily route the analyzer diagnostic to stdout in production, run `cargo test -p hoimin-cli --test run_e2e real_binary_json_report`, and observe failure due to mixed or invalid stdout. Restore the production file immediately.

- [ ] **Step 5: Verify the restored behavior**

Run `cargo test -p hoimin-cli --test run_e2e real_binary_json_report` and require a pass.

### Task 3: Verify and Commit the Test-Only Change

**Files:**
- Verify: `crates/hoimin-cli/tests/cli_config.rs`
- Verify: `crates/hoimin-cli/tests/run_e2e.rs`
- Verify: `docs/superpowers/plans/2026-08-03-issue-158-real-binary-streams.md`

**Interfaces:**
- Consumes: the two new process-boundary tests.
- Produces: a formatted, lint-clean, test-only commit for issue #158.

- [ ] **Step 1: Run both focused tests**

Run `cargo test -p hoimin-cli --test cli_config real_binary` and `cargo test -p hoimin-cli --test run_e2e real_binary_json_report`.

- [ ] **Step 2: Run appropriate CLI suites and static checks**

Run the complete `cli_config` and `run_e2e` test targets, `cargo fmt --all -- --check`, Clippy for `hoimin-cli` tests with denied warnings, and `git diff --check`.

- [ ] **Step 3: Review scope and commit**

Confirm only the two named test files and this plan changed, preserve the untracked `.venv` symlink, and commit with `test: verify real binary stream routing`.
