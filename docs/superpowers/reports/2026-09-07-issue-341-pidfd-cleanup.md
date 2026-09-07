# Issue #341: pidfd-safe cgroup cleanup fallback

## Cause

`kill_all_listed_pids` read numeric process IDs from `cgroup.procs` and passed
them to `kill(pid, SIGKILL)`. Linux can recycle a PID after the membership read
and before `kill` resolves it. The fallback could then signal a process outside
the owned cgroup.

The cgroup v2 documentation also permits duplicate PIDs in one `cgroup.procs`
read when a process migrates away and back or when the kernel recycles a PID
during the read. A recent membership snapshot therefore cannot establish
process identity.

## Safety contracts

The `cgroup.kill` fast path retains its existing behavior. The kernel kills the
whole subtree and protects that operation against forks and migration.

The fallback performs these operations for each PID in the snapshot:

1. `pidfd_open(pid, 0)` acquires a stable reference to the process that owns the
   number at that point.
2. A second read of the same cgroup's `cgroup.procs` confirms that the number
   remains a member after acquisition.
3. `pidfd_send_signal(pidfd, SIGKILL, NULL, 0)` targets the acquired process.

An `ESRCH` result from either syscall means the acquired process has exited, so
cleanup continues. Other errors, including `ENOSYS`, `EPERM`, `EMFILE`, and
`EINVAL`, return a typed cleanup error. The fallback does not retry with
`kill(pid, ...)`. `OwnedFd` closes each pidfd on success, skipped membership,
and error paths.

Both `cgroup.procs` and `pidfd_open` use process IDs as seen from the caller's
active PID namespace. A cgroup namespace or a non-root cgroup2 mount changes
path visibility, not that PID mapping. Hoimin continues to resolve the owned
cgroup path from its current cgroup2 mount before cleanup.

Kernel documentation for these contracts:

- [Control Group v2](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html)
- [`pidfd_open(2)`](https://man7.org/linux/man-pages/man2/pidfd_open.2.html)
- [`pidfd_send_signal(2)`](https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html)

## TDD evidence

The first ordering test failed at the unimplemented safe-signaling boundary,
then passed after the helper acquired a handle and re-read membership. Separate
tests observed `CgroupCleanup` before the implementation handled `ESRCH` from
`pidfd_open` and `pidfd_send_signal`; each passed after its error branch changed.

The fake-cgroup integration test failed at the unimplemented fallback seam. It
now changes `cgroup.procs` during pidfd acquisition and confirms that cleanup
skips the migrated process. A real Linux test acquires a pidfd for an owned
`sleep` child, sends `SIGKILL` through the pidfd, reaps the child, and checks the
terminating signal. Error tests cover unsupported, permission, descriptor-limit,
and invalid-argument failures without signaling a numeric PID. A drop recorder
checks handle release on signal failure.

## Verification

The Linux platform slice passed in `rust:1.98-bookworm` on an aarch64 Linux
Docker host:

```sh
docker run --rm \
  -v "$PWD:/workspace" \
  -v /private/tmp/hoimin-issue-356-linux-target:/target \
  -v /private/tmp/hoimin-issue-356-linux-cargo/registry:/usr/local/cargo/registry \
  -w /workspace \
  -e CARGO_TARGET_DIR=/target \
  rust:1.98-bookworm \
  cargo test -p hoimin-cli resource::linux::platform::tests --lib
```

Result: 13 passed, 0 failed.

The following checks also completed with exit code 0:

```sh
cargo test --workspace --all-features --quiet
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

The workspace test ran on macOS. Clippy ran in the Linux container. A second
operator reproduced the 13-test Linux platform slice from the same diff. An
independent review found no blocking issue in the syscall ABI, cleanup ordering,
error handling, descriptor ownership, or PID-namespace assumptions.

## Limitations

Linux added `pidfd_open` in 5.3 and `pidfd_send_signal` in 5.1. On a kernel
without pidfd support, failure of both `cgroup.kill` and `pidfd_open` leaves
cleanup pending instead of risking a signal to an unrelated process.

Pidfds prevent PID-reuse redirection. The second membership read also rejects a
process that exits or migrates before validation. A process can still migrate
out after the final membership read and before `pidfd_send_signal`; older
kernels provide no atomic membership-and-signal fallback. `cgroup.kill` remains
the migration-safe path on kernels that support it.

The real-process test exercises the Docker host kernel. It does not emulate an
old kernel that returns `ENOSYS`; injected syscall failures cover that branch.
