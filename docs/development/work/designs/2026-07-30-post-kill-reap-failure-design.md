# Post-kill Reap Failure Design

## Scope

Fix #67 so the focused-mutation command runner never treats a process as
cleaned up when its final bounded wait after `kill` expires.

## Root cause

`CommandRunner._terminate` performs a graceful stop and bounded wait, escalates
to `kill`, and performs a second bounded wait. The second
`subprocess.TimeoutExpired` is currently discarded. The enclosing command then
retains `exit_code=None`, reports only the original timeout or interruption,
and leaves the reusable runner with no indication that it may still own a live
root process.

## Design

Introduce `ProcessLifecycleError`, a typed exception that identifies the
command whose root process could not be reaped after forced termination.
`_terminate` raises this exception only after both bounded waits expire.

The command timeout or interruption remains the primary outcome. `run` catches
the lifecycle error, stores its diagnostic in `CommandRecord.cleanup_errors`,
completes the record with `exit_code=None`, closes and checks command logs, and
then raises the original `CommandTimedOut` or `CommandInterrupted`. It also
poisons the runner with the typed lifecycle error. Every later `run` call
raises that same lifecycle error before allocating paths or launching a
process. This fail-closed boundary prevents later mutation commands from
starting while root-process state is unknown.

The JSON checkpoint already serializes `cleanup_errors`. The Markdown report
will add a command cleanup failures section containing the command label,
unknown exit status, and diagnostic, so the final human-readable artifact does
not imply successful cleanup.

## Alternatives considered

- Returning a boolean from `_terminate` is smaller, but loses typed failure
  semantics and makes accidental ignoring easy.
- Raising the lifecycle error as the primary command result makes the cleanup
  problem visible, but violates the requirement that the original timeout or
  interruption remain primary.
- Stopping only in `run_workflow` duplicates lifecycle knowledge above the
  process owner and leaves direct `CommandRunner` reuse unsafe.

## Testing

- Deterministic fake-process tests make the initial command wait, graceful
  post-terminate wait, and final post-kill wait expire for both timeout and
  interruption paths.
- Runner tests assert the original outcome remains primary, the record contains
  the lifecycle cleanup error, `exit_code` remains `None`, and a later command
  is rejected without invoking `Popen`.
- Reporting tests assert both the checkpoint JSON and Markdown expose the
  cleanup failure and unknown exit status.
- A POSIX integration test starts a root with a descendant, handles a real
  timeout, and verifies both recorded PIDs cease to exist. Existing Windows
  inherited-handle coverage continues to verify descendant cleanup there.

## Documentation impact

This changes the internal developer mutation-evidence runner rather than the
published `hoimin` CLI contract. The focused development guide already
documents bounded process cleanup and retained artifacts; README changes are
not required.
