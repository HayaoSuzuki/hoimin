# Windows Real PID Reuse Feasibility Report

## Conclusion

The exact acceptance criteria of
[#223](https://github.com/tokyogas-tech/hoimin/issues/223) are not enforceable
on standard Windows CI. A durable handle to the exited old generation makes its
PID ineligible for reuse. Once that handle is closed, supported Windows process
creation still cannot request or bound reuse of that particular PID. The issue
must remain open as the explicit platform remainder required by its own body.

## Evidence chain

### Operating-system contract

1. Microsoft documents `PROCESS_INFORMATION.dwProcessId` as valid until all
   handles to the process are closed and the process object is freed. Only at
   that point may the PID be reused.
2. Microsoft documents that consumers of Job Object completion-port PIDs must
   maintain an open process handle to guarantee the identifier has not been
   recycled.
3. `CreateProcessW` accepts executable, environment, handle, startup, and
   creation-flag inputs. The PID is system-assigned and returned through the
   output `PROCESS_INFORMATION`; there is no requested-PID parameter.
4. Ordinary Job Object completion messages are not guaranteed notifications,
   which precludes strengthening a bounded fixture into an OS-level delivery
   guarantee.

Official sources:

- [PROCESS_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/ns-processthreadsapi-process_information)
- [CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)
- [JOBOBJECT_ASSOCIATE_COMPLETION_PORT](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_associate_completion_port)
- [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)

### Production data flow

The current backend already applies Microsoft's recommended protection:

```text
CREATE_SUSPENDED child
  -> OpenProcess durable handle
  -> assign run-wide Job Object
  -> assign nested root Job Object
  -> resume
  -> ActiveRoot { UUID, PID, signal, OwnedHandle }
  -> real completion-port exit message
  -> remove matching oldest PID generation
  -> close OwnedHandle
  -> PID becomes eligible for future reuse
```

The old generation cannot simultaneously remain delayed with its handle and
share its PID with a new process. Production cleanup necessarily crosses and
retires the old completion state before that PID becomes reusable.

### Acceptance matrix

| Criterion | Reachable? | Evidence |
|---|---:|---|
| Capture a real assigned generation with a durable handle, let it exit, retain delayed cleanup | Yes | `SuspendedChild::open`, `register_root`, and existing Windows integration tests |
| Deterministically obtain a second real process with the same PID while the old state is retained | No | `PROCESS_INFORMATION` forbids reuse while any old handle remains; `CreateProcessW` cannot request a PID |
| Run old cleanup through the production Job Object path | Yes by itself | `drain_until_root_exit` consumes the real completion port and removes the old generation |
| Prove a same-PID new generation remains active while old cleanup runs | No | The same-PID precondition is unreachable until old cleanup closes the handle |
| Cross the real completion port without synthetic state | Yes by itself | Existing Windows process tests do so; ordinary message delivery remains documented as non-guaranteed |
| Bound the complete fixture and cleanup | No for the full conjunction | Handle closure plus PID churn has no supported deterministic bound and is explicitly disallowed |

## Local investigation record

Environment:

```text
WindowsProductName: Windows 10 Home
WindowsVersion: 2009
rustc: 1.97.1 (8bab26f4f 2026-07-14)
cargo: 1.97.1 (c980f4866 2026-06-30)
branch: test/issue-223-real-pid-reuse
base: 5ceb3e2
```

Focused production-path baseline:

```console
cargo test --offline -p hoimin-cli resource::windows -- --nocapture
```

After the initial dependency build exceeded a 120-second command bound, the
fresh rerun completed successfully: 12 Windows resource tests passed, none
failed, and 165 library tests were filtered out. The focused test bodies
finished in 0.17 seconds; compilation and test enumeration finished in 60.2
seconds.

Bounded real-handle probe:

```text
old_pid: 8076
durable_handle_get_process_id: 8076
old_exit_code: 0
bounded_new_processes: 64
reused_while_old_handle_open: false
new PID range observed: 2332..45736
elapsed_seconds: 4.001
outer bound: 30 seconds
```

An earlier 512-process version of the same diagnostic hit its 60-second outer
bound and produced no result, so it is not counted as evidence. The 64-process
probe is only corroboration; absence of reuse in a finite sample cannot replace
the Microsoft contract.

## Alternatives rejected

- Closing the handle before notification cleanup weakens the production PID-
  reuse barrier and violates the durable-handle criterion.
- Bounded PID churn is probabilistic and is expressly excluded by the issue.
- An unbounded loop has neither deterministic runtime nor acceptable cleanup.
- Synthetic PIDs, injected completion packets, and direct state mutation do not
  cross the operating-system boundary.
- Kernel-table manipulation or a modified hypervisor is not a supported
  `windows-latest` capability and would not test the production environment.

## Final verification record

The documentation-only disposition received fresh verification on the Windows
host:

```text
cargo test --offline -p hoimin-cli resource::windows -- --nocapture
  PASS: 12 passed, 0 failed, 165 filtered out; focused bodies 0.15s

cargo fmt --all -- --check
  PASS

cargo clippy --offline --workspace --all-targets --all-features -- -D warnings
  PASS: finished in 1m 04s

git diff --check
  PASS
```

The unfiltered `cargo test --offline --workspace` run had one environment-only
failure after 175 library tests passed: the existing
`workspace::root::tests::rejects_non_normal_and_linked_parent_components`
fixture could not create a Windows link and returned error 1314
(`ERROR_PRIVILEGE_NOT_HELD`). The focused test failed identically outside the
restricted token, confirming the host lacks the required link privilege. The
fresh command below then passed every remaining workspace test:

```console
cargo test --offline --workspace -- --skip workspace::root::tests::rejects_non_normal_and_linked_parent_components
```

The primary library result was 175 passed, 0 failed, 1 ignored, and the one
named test filtered out; all integration, core, and documentation test binaries
also completed without failures.

`git diff --name-only` plus repository status show only the three Markdown files
in this disposition. Rust mutation testing is therefore not applicable: no Rust
production or test code changed, and an empty-diff mutants run would not provide
evidence for #223.

## Disposition

No Rust implementation or new test is warranted. The bounded alternative is to
continue running the existing real Windows completion-port handle-retention
tests while explicitly stating that they do not cover real PID reuse. Keep #223
open, keep it linked from #162, and do not attach a CI URL as issue-closing
evidence unless every original criterion becomes enforceable.
