# Issue #222 Windows Root-Before-Descendant Test Design

## Context

Issue #222 requires Windows evidence for the normal-exit cleanup boundary where a runtime root exits while an assigned descendant remains active. Existing Windows coverage proves close-time descendant termination, but it does not observe an exited root and a live descendant through real process handles before `ProcessHandler` performs its normal completion cleanup.

The production Windows backend already assigns each suspended root to the run-wide Job Object and a nested root Job Object before resuming it. On a normal root exit, `ProcessHandler` classifies the completion-port notification, terminates the nested root Job Object, drains output, and returns the root result. `ProcessHandler::close` then terminates the run-wide Job Object. This issue adds deterministic coverage for that existing lifecycle; it does not change production behavior.

## Approaches Considered

### 1. Internal Windows backend test using private Job Object accounting

Add the regression to `resource::windows::tests`. Construct a real `WindowsBackend`, retain a clone for querying its production run Job Object, and run the root through the public `ProcessHandler`. Use real Win32 process handles for both published process identities.

This is the selected approach. It exercises the production assignment, completion-port classification, nested cleanup, and run close paths without exposing a test-only production API. The test can directly call the existing private `active_process_count` function against the real run Job Object.

### 2. Public integration test with a Job Object accounting hook

Add the scenario to `crates/hoimin-cli/tests/process_handler.rs` and expose a public or hidden `WindowsBackend` method that returns the active-process count.

This would align with the existing platform integration tests, but it would add production surface solely for one test. The internal test provides the same real behavior with less scope.

### 3. Public integration test that infers Job Object emptiness from PIDs

Open the two process handles in the integration test and treat both becoming inactive as proof that cleanup emptied the Job Object.

This does not satisfy the acceptance criterion. Process inactivity is distinct from querying the Job Object's assigned-process accounting, and raw PID observations do not prove which process generation was observed.

## Test Fixture and Synchronization

The test runs a Python root through `ProcessHandler::handle`. The root starts a 30-second descendant with standard streams detached. It then writes its own PID and the descendant PID into one pending file and atomically renames that file to the published identity file. The root waits for a separate release file before exiting with code zero.

The Rust test polls for the identity file while also polling the pinned `handle` future so spawn and assignment can progress. It parses exactly two PIDs and immediately opens owned Win32 process handles with synchronization and limited-query rights. Opening the handles before releasing the root binds every later observation to those exact process generations and prevents PID reuse from substituting synthetic state.

After creating the release file, the test deliberately stops polling the `handle` future. It condition-polls the real root handle until Windows reports it signaled, then checks that the real descendant handle still reports `WAIT_TIMEOUT`. Because the handler future is dormant during this observation, production cleanup cannot race ahead and erase the required root-before-descendant boundary.

The test then resumes the handler future. Normal completion must return `ProcessTermination::Exit(0)`. The test condition-polls until the descendant handle is signaled and `QueryInformationJobObject` reports zero active processes. Only then does it call `ProcessHandler::close`, which must also succeed. A single outer six-second timeout bounds spawn, coordinated exit, classification, descendant termination, accounting convergence, and close.

## Assertions

The regression asserts all of the following:

- the atomically published fixture contains both process identities before release;
- the opened root process handle reaches a real signaled state;
- the opened descendant process handle remains active at that exact boundary;
- `ProcessHandler::handle` returns `ProcessTermination::Exit(0)`;
- normal handler cleanup makes the descendant handle inactive;
- the production run Job Object reports zero active assigned processes;
- `ProcessHandler::close` succeeds;
- the entire operation completes in less than six seconds.

The test does not mutate `RunState` and does not call `record_notification`. Existing direct-state unit tests remain separate coverage for notification bookkeeping.

## Failure Cleanup

The pinned handler future owns the production supervisor and nested kill-on-close Job Object. Dropping it during a panic terminates the assigned fixture tree. The retained backend clone owns the run-wide kill-on-close Job Object, providing a second cleanup boundary. Owned test process handles close through the existing `OwnedHandle` drop implementation. No test-only cleanup method is added to a production type.

## TDD and Mutation Evidence

The behavioral break named by this test is: normal root completion discards the assigned nested Job Object without either terminating it or closing its kill-on-close handle. For the RED run, the temporary local fault replaced `root_job` with a new empty Job Object and forgot the assigned handle before termination. The focused command exited `1` after 2.62 seconds with `assigned descendant remained active after normal root cleanup`. Restoring the production ownership and termination path made the identical command pass in 0.25 seconds; the post-commit rerun passed in 0.43 seconds. An earlier trial that merely omitted `TerminateJobObject` passed because dropping the still-owned kill-on-close handle provides the same externally visible cleanup.

Only a `#[cfg(test)]` module and documentation are changed. No production Rust expression is added or modified, so Rust mutation testing of changed production code is not applicable. The deliberate red-phase production mutation supplies direct evidence that the new test detects the cleanup regression it names.

## Verification Record

- `cargo fmt --all -- --check` exited `0`.
- The post-commit focused Windows regression exited `0`, with one test passed in 0.43 seconds.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` exited `0`.
- `cargo test -p hoimin-cli -- --skip workspace::root::tests::rejects_non_normal_and_linked_parent_components` exited `0` across the package targets.
- `cargo test --workspace -- --skip workspace::root::tests::rejects_non_normal_and_linked_parent_components --test-threads=1` exited `0` in 130.7 seconds.
- The unfiltered package run cannot pass on this host because the pre-existing symlink test receives Windows error 1314: the client lacks the required privilege. The same test failed outside the restricted sandbox. Parallel process-fixture runs also exposed pre-existing PID-publication timing flakes, while the serialized workspace run passed.
- `git diff --check` exited `0`, and the new regression contains neither direct `RunState` mutation nor a `record_notification` call.

## Scope

This change adds one Windows-only regression test and its test-local helpers. It does not change Job Object semantics, completion-port bookkeeping, process cleanup ordering, public APIs, dependencies, or CI configuration. A passing Windows CI URL remains an external issue-closing requirement and is not produced by this local change.
