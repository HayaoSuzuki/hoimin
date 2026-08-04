# Windows Console Interrupt Coverage Design

## Goal

Close issue #215 with deterministic Windows tests that deliver the first and second interrupts to the real `CARGO_BIN_EXE_hoimin` through `GenerateConsoleCtrlEvent(CTRL_C_EVENT, ...)`, without a production test hook.

## Existing gap

The Unix `run_e2e` tests already prove both lifecycle rows with real `SIGINT`. On Windows the same target has durable process-handle support, but the signal tests and session/cleanup helpers are Unix-only. Tokio's unit tests prove interrupt counting but do not cross a Windows console boundary.

The baseline workspace run on this host completed 175 library tests and failed only at `workspace::root::tests::rejects_non_normal_and_linked_parent_components` because the account lacks Windows symbolic-link privilege (OS error 1314). That pre-existing environment failure is unrelated to console control.

## Approaches considered

1. Send `CTRL_BREAK_EVENT` to a `CREATE_NEW_PROCESS_GROUP` child. It can be targeted by group ID, but it does not satisfy the explicit `CTRL_C_EVENT` boundary.
2. Add a production command, environment variable, or public injection API. This would avoid console attachment, but the issue rejects production fault injection and it would not test Windows delivery.
3. Spawn hoimin in a dedicated console, then re-enter the integration-test binary as an ignored helper. The helper attaches to that console, ignores Ctrl+C for itself, calls `GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)`, and detaches. This crosses the real OS boundary while keeping control code in the test target. This design selects approach 3.

## Architecture

The existing Unix scenario bodies become platform-neutral private async helpers. Thin platform-named tests keep the current Unix names and add Windows names. The real hoimin fixture remains the same except that Windows supplies `CREATE_NEW_CONSOLE` when spawning it. `CREATE_NEW_PROCESS_GROUP` is not combined with it because Windows ignores that flag when a new console is requested.

The Windows sender launches the current integration-test executable with `--ignored --exact console_ctrl_sender_helper` and passes the hoimin PID through an environment variable. The helper calls `AttachConsole(pid)`, `SetConsoleCtrlHandler(None, TRUE)`, `GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)`, and `FreeConsole()`. Group ID zero is required for `CTRL_C_EVENT`; the dedicated console limits recipients to the fixture tree and sender helper. The parent waits for the helper's successful exit before observing hoimin.

The first scenario waits for the atomically renamed descendant marker and one live incomplete session row, sends one event, then requires exit 130, one parseable JSON report with `complete:false`, persisted `complete=0`, and a stopped descendant handle.

The second waits for `mutant_started`, retains a SQLite `BEGIN IMMEDIATE` lock, sends one event and observes descendant reaping, then sends another event. It requires exit 130 within one second and no `run_finished` JSONL event while finalization remains locked.

## Error handling and cleanup

Every console API failure includes the API name and `last_os_error`. The sender helper always attempts `FreeConsole` after successful attachment. Both Python fixture processes ignore `SIGINT` before publishing readiness, so their observed termination proves hoimin's Job cleanup rather than direct console-event handling. Once readiness publishes the active-mutant and descendant PIDs, Windows opens process handles with query and terminate rights and retains them through teardown. Windows teardown terminates only those retained handles and never reopens recorded PIDs; Unix retains its existing PID-based `SIGKILL` cleanup.

All readiness is file-, SQLite-, stdout-, or process-handle-based. Fixed sleeps are short polling intervals, never proof that a phase occurred.

## Files and compatibility

- `crates/hoimin-cli/tests/run_e2e.rs`: shared interrupt scenarios, Windows console sender helper, fixture creation and cleanup.
- `crates/hoimin-cli/Cargo.toml`: enable the existing `windows-sys` dependency's `Win32_System_Console` feature.

No production module, public interface, CLI option, schema, or runtime behavior changes. Unix tests retain their names and behavior.

## Verification

On Windows, run both exact new tests and the complete `run_e2e` target. Run formatting, Clippy, and the workspace suite while recording the unrelated symlink-privilege baseline if it remains. Run focused cargo-mutants against `crates/hoimin-cli/src/interrupt.rs`; the new E2E tests must kill behavioral mutants reachable through real first/second interrupt handling, or each non-executed outcome must be explained from the tool report.
