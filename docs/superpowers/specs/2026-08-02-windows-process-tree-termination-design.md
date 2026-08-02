# Windows Process Tree Termination Design

## Problem

The focused-mutation runner starts Cargo with redirected log handles. On POSIX,
timeout and interruption terminate the new process group. On Windows, the
runner only terminates the root Cargo process. Rust compiler and test
descendants can therefore survive, retain log and target-directory handles,
and poison later commands.

## Design

Add a Windows tree terminator that resolves
`%SystemRoot%\System32\taskkill.exe` and invokes it as
`taskkill.exe /PID <pid> /T /F`. The `/T` option includes descendants and `/F`
provides the forced termination semantics required after a timeout or user
interruption. Suppress taskkill output and bound the command itself with the
same two-second lifecycle window used for root reaping. A missing `SystemRoot`
is a lifecycle failure rather than a reason to fall back to executable search.

Inject the tree terminator into `CommandRunner` for deterministic tests. On
Windows, `_terminate` calls it before waiting for the root process to be
reaped. POSIX process-group behavior remains unchanged.

If taskkill cannot start, times out, or exits unsuccessfully, kill and reap the
root process as a best-effort cleanup, then record a `ProcessLifecycleError`
and block runner reuse. This is fail-closed: descendants may remain, so a later
command must not start and contend for their locks.

## Verification

- A platform-neutral unit test forces the Windows branch and proves the tree
  terminator receives the Cargo root PID instead of calling root `terminate`.
- A helper test proves the exact taskkill argument vector, timeout, and quiet
  stdio policy.
- A failure test proves taskkill errors trigger root fallback cleanup, are
  recorded, and block runner reuse.
- Existing POSIX descendant and Windows inherited-handle tests remain intact.

## Non-goals

- Introducing Windows Job Objects or recursive process enumeration.
- Graceful shutdown of Cargo after timeout/interruption.
- Changing POSIX process-group termination.
