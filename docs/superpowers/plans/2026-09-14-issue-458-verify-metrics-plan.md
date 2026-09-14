# Verify metrics implementation plan

> Execute inline with the executing-plans workflow; the user authorizes autonomous completion and requires three self-review passes at every stage.

**Goal:** Export the existing operational metrics for verify batches.
**Architecture:** Normalize the CLI destination using the run helper, attach it after plan validation in both dispatch routes, and reuse shell finalization.
**Tech Stack:** Rust, clap, Tokio, existing metrics schema.
**Spec:** `docs/superpowers/specs/2026-09-14-issue-458-verify-metrics-design.md`

## Global constraints

No plan/result/metrics schema change. Relative paths use invocation cwd. No execution/resource override. No sidecar on preparation failure. Preserve existing run collision and warning behavior.

## Task 1: CLI metrics export

Files: `crates/hoimin-cli/src/cli.rs`, `crates/hoimin-cli/src/lib.rs`, `crates/hoimin-cli/tests/plan.rs`.

- [x] Add a regression invoking `hoimin_cli::run_with_io(["hoimin", "verify", plan, "--top", "2", "--metrics", destination], ...)`; assert successful exit, `RunMetrics::validate()`, discovered/executed = 2 and report ID parity. Current parser must fail with unexpected `--metrics`.
- [x] Run `cargo test -p hoimin-cli --test plan verify_metrics -- --nocapture` and inspect the missing-feature failure.
- [x] Add `metrics: Option<PathBuf>` to raw args and `metrics: Option<Utf8PathBuf>` to public verify args. Extract run normalization as `fn metrics_path(path: PathBuf) -> Result<Utf8PathBuf, CliError>`; call `.map(metrics_path).transpose()?` in both parse paths.
- [x] In both verify dispatch matches use `Ok(mut verified)` and assign `verified.config.output.metrics = args.metrics` before shell execution.
- [x] Extend regressions for explicit IDs, strict/diverse, baseline failure, prevalidation failure, write warning and protected-source rejection. Use existing `write_plan_manifest` fixtures and independent report candidate assertions.
- [x] Run plan and argument tests plus existing run metrics tests; run fmt and Clippy for affected targets.

## Task 2: Documentation and release review

Files: README operational metrics; related OKF selection contract; design/audit reference indexes; issue-specific review report.

- [x] Add verify usage with separate report and metrics paths and explain prevalidation failure/stage timing.
- [x] Record source hashes only after final edits; validate OKF frontmatter through YAML parser and links separately.
- [x] Review OKF, design, plan, implementation, tests and PR three times each; record findings and fixes with evidence in the review report.
- [ ] Commit only issue files, push dedicated branch, create PR from repository template; inspect published diff/body and checks.
