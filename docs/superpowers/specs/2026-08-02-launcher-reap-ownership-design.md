# Launcher Reap Ownership Design

## Context

The Linux cgroup launcher is spawned as a `tokio::process::Child`. Before the
target executes, the launcher raises `SIGSTOP` so the cgroup backend can move it
into the prepared cgroup. `wait_for_launcher_stop` currently observes this with
`waitpid(WUNTRACED | WNOHANG)`.

`waitpid` consumes an observed exit status. If the launcher exits before raising
`SIGSTOP`, `wait_for_launcher_stop` reaps it and then reports that it did not
stop. The Tokio `Child` still owns process cleanup, so its later kill and wait
operations can act on a stale PID and report `ECHILD`.

## Requirements

- Observe the launcher's `SIGSTOP` without consuming any child status.
- Detect an exited, killed, or dumped launcher promptly and report the existing
  invalid-launcher error.
- Leave all reaping to the owning Tokio or standard-library `Child`.
- Preserve the existing one-second polling deadline and five-millisecond poll
  interval.
- Keep the change Linux-only and avoid raising the minimum supported kernel
  version.
- Do not change cgroup attachment, cleanup ordering, or public error schemas.

## Design

Replace `waitpid` with `waitid` using `P_PID` and the flags
`WSTOPPED | WEXITED | WNOHANG | WNOWAIT`.

`WSTOPPED` reports the expected launcher stop, while `WEXITED` makes premature
termination observable immediately. `WNOHANG` preserves the current polling
loop. `WNOWAIT` leaves every reported status waitable, so the `Child` owner can
perform the only consuming wait.

Each poll initializes a `siginfo_t` to zero and calls `waitid` for the exact
launcher PID. A successful call with `si_pid() == 0` means no matching state is
available yet. `CLD_STOPPED` with `si_status() == SIGSTOP` succeeds. Every other
reported child state returns the existing `InvalidCgroupData` message. System
call errors and the deadline keep their existing `ResourceError::io` mapping.

## Alternatives Considered

- A PID file descriptor plus `waitid(P_PIDFD)` would also protect against PID
  reuse, but it requires Linux 5.4 or newer and belongs with Issue #99's identity
  tracking work.
- `/proc` inspection or `kill(pid, 0)` cannot reliably distinguish the required
  stopped state and retains PID races.
- Keeping `waitpid` cannot preserve reaping ownership because `waitpid` has no
  portable `WNOWAIT` behavior.

## Testing

Linux-only tests use real child processes rather than mocking wait status:

1. A child that raises `SIGSTOP` is accepted, then continued and reaped by its
   `Child` owner.
2. A child that exits before stopping causes `wait_for_launcher_stop` to return
   the invalid-launcher error, after which `Child::wait` still succeeds. This is
   the regression test proving the observer did not reap the child.

Host-independent formatting, lint, and workspace tests remain required. The
Linux regression tests run in a Linux container and in GitHub Actions.

## Scope

This change fixes only launcher state observation and reaping ownership for
Issue #100. PID identity hardening, process-tree termination, and other cgroup
cleanup changes remain separate issues.
