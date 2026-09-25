# Issue 606 implementation plan

**Goal:** prevent verify metrics from replacing the input manifest before baseline.
**Architecture:** retain manifest entry/referent paths and add them to existing preflight entry comparison through shared verified shell execution.
**Tech stack:** Rust, Tokio, real CLI integration tests and temporary filesystem fixtures.
**Spec:** docs/superpowers/specs/2026-09-25-issue-606-manifest-metrics-design.md

## Global constraints

No plan schema changes. Preserve safe final symlink and hardlink replacement. Preserve uncertain-destination warnings, failed-baseline metrics, and existing source/session protection. Use only the assigned target directory, one build job, and no Lean.

## Review focus

- Relative/dot/parent aliases: subprocess CWD is explicit; rejection preserves bytes and marker absence.
- Input symlink: supplied entry and canonical referent are both protected.
- Native case aliases: test only when native lookup resolves the alternative spelling.
- Safe output aliases: replace a separate symlink/hardlink, verify original bytes and parse metrics.
- Failing baseline and public selectors: both top and candidate reject before running; separate output still records failure.

## Task 1: tests and implementation

Files: `crates/hoimin-cli/tests/plan.rs`, `src/plan.rs`, `src/shell.rs`, `src/metrics_destination.rs`, `src/lib.rs`, and `README.md` within the CLI crate except README at repository root.

Interfaces: `VerifiedPlan` produces `pub(crate) manifest_inputs: Vec<PathBuf>`; `validate_metrics_destination(config, targets, protected_inputs: &[PathBuf], id)` consumes the paths; shared `run_verified` sets `ShellContext.metrics_protected_inputs` and retains the existing selection and fingerprint behavior. Existing public selected-loop APIs stay compatible.

- [x] Add real CLI collision tests with fixtures made by `write_plan_manifest_with_marker`, using both selectors and normal/failing commands. For each case:
  ```rust
  assert_eq!(output.status.code(), Some(2));
  assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.destination.collision"));
  assert_eq!(std::fs::read(&path).unwrap(), original);
  assert!(!marker.exists());
  ```
  Cover absolute, relative, dot, dot-dot, parent symlink, native case alias, and supplied symlink/target destinations. Add positive tests for new/existing metrics and separate hardlink/symlink aliases, including failed-baseline sidecars.
- [x] Run `cargo test -p hoimin-cli --test plan verify_metrics_manifest -- --test-threads=1`. Expected: collision tests fail because metrics is accepted and baseline runs.
- [x] Store the absolute and canonical manifest paths during plan preparation. Extend inspector protected paths with `protected.extend_from_slice(protected_inputs)`. Add paths to shell context/preflight task and use a shared verified runner for owned and borrowed output dispatch. Update README protection list.
- [x] Repeat targeted command. Expected: all manifest cases pass; safe aliases still replace only the alias.
- [x] Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace -- --test-threads=1` under the assigned resource environment. Expected: pass; record any environmental exceptions precisely.
- [x] Perform three implementation and three test self-review passes; record findings and fixes with actual RED/GREEN and suite evidence. Commit implementation/tests/review evidence.

## Plan self-reviews

1. Requirement coverage: a top-only API test would omit real CLI and candidate dispatch. Added both selectors to subprocess cases and retained borrowed-writer regression coverage.
2. Negative/positive balance: rejecting all shared inodes could satisfy negative tests. Added safe symlink/hardlink replacement and existing metrics update assertions.
3. Verification/resources: filesystem case behavior must be detected, not assumed; explicit CWD is required for relative paths. Added native lookup condition and resource-constrained full workspace commands. Shared interface contracts are consistent; no placeholders remain.

Execution is inline in the assigned worktree. User authorization covers design, plan, implementation, and commits; parent agent handles independent review and publication.
