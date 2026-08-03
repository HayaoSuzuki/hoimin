# Real-Signal Cancellation Coverage Implementation Plan

**Goal:** Cover Unix first- and second-interrupt behavior through the real `hoimin` binary without adding a production test API or changing runtime behavior.

**Architecture:** Reuse the existing real-process cancellation fixture and parameterize only its report format. The first-SIGINT test waits for the atomically published descendant readiness marker and the live incomplete SQLite run, sends `SIGINT`, records whether the descendant stopped before teardown, and validates the completed JSON report after robust process cleanup. The retained second-SIGINT test continues to use JSONL readiness and a held SQLite write lock to prove escalation bypasses blocked finalization.

**Tech Stack:** Rust, Tokio process APIs, `rusqlite`, Python fixture commands, Unix `libc::kill`.

## Constraints

- Spawn `env!("CARGO_BIN_EXE_hoimin")`; do not call a shell entry point directly.
- Keep the first-SIGINT stdout as one parseable JSON report and the second-SIGINT stdout as JSONL events.
- Synchronize on readiness files and the incomplete session row, never on a test-side fixed sleep.
- Observe descendant termination before teardown, always reap the CLI child and fixture process tree, and assert the recorded observation only after cleanup.
- Assert exit code `130`, `summary.complete == false`, SQLite `complete == 0`, and descendant termination after the first signal.
- Retain the second-signal `130` escalation and absence of `run_finished` while finalization is blocked.
- Prove the first test is load-bearing by temporarily no-oping its test-only signal send, observing the expected timeout/failure, and restoring the real signal.
- Keep Windows console-control coverage as a narrowed follow-up; do not emulate `GenerateConsoleCtrlEvent` on Unix.

## Verification

1. Run the new exact first-SIGINT test.
2. Temporarily replace its `send_sigint(child.id())?` call with a no-op, rerun, and record the expected timeout failure; restore it immediately.
3. Run both focused signal tests and the complete `run_e2e` target.
4. Run relevant workspace tests, `cargo fmt --all -- --check`, Clippy with all targets/features and denied warnings, and `git diff --check`.
5. Review the final diff, confirm no production files changed, preserve `.venv`, and commit with `test: exercise cancellation through real signals`.

## Windows Follow-up

Open follow-up [#215](https://github.com/tokyogas-tech/hoimin/issues/215) covers both the first- and second-console-control audit rows with a real process group and `GenerateConsoleCtrlEvent`; Unix signal tests do not cover those rows. It is linked from the [#153 closure comment](https://github.com/tokyogas-tech/hoimin/issues/153#issuecomment-5162757276).
