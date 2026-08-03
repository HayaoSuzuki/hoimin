# Issue #162 process-backend fault coverage

Parent: [#162](https://github.com/tokyogas-tech/hoimin/issues/162)

This plan records the disposition of every process-backend audit row. The three
Linux rows have executable coverage. The three Windows rows remain platform
remainders and each requires its own open, linked follow-up before #162 can be
closed. Direct `RunState` mutation, `record_notification`, and synthetic PIDs do
not count as real Windows Job Object evidence.

## Six-row traceability

| # | Exact audit row | Disposition | Test or required follow-up |
|---|---|---|---|
| 1 | Windows abnormal root exit crosses Job Object notification plumbing, classifies without timeout, and completes within a bound | Follow-up required before parent closure | W1, `test(windows): exercise abnormal root exit through Job Object completion notifications` |
| 2 | Windows root exits while descendants remain assigned and cleanup stays bounded | Follow-up required before parent closure | W2, `test(windows): bound cleanup when a root exits before assigned descendants` |
| 3 | Windows stale/recycled PID cleanup captures a real exited PID and never signals the recycled raw PID | Follow-up required before parent closure | W3, `test(windows): protect Job Object cleanup from a recycled real PID` |
| 4 | Linux cgroup abnormal root exit classifies without timeout and completes within a bound | Covered | `process_handler::cgroup_v2::abnormal_runtime_root_is_classified_and_cleaned_within_six_seconds`; real delegated cgroup-v2 container: pass, no `SKIP:`; attach the eventual `linux-cgroup-v2-hard` run URL before closing #162 |
| 5 | Linux cgroup root exits while descendants persist and cleanup remains bounded | Covered | `process_handler::cgroup_v2::exited_root_is_observed_before_its_descendant_is_cleaned`; real delegated cgroup-v2 container: pass, no `SKIP:`; attach the eventual `linux-cgroup-v2-hard` run URL before closing #162 |
| 6 | Linux cgroup cleanup uses kernel/group ownership and does not signal an unverified stale numeric PID | Covered | `resource::linux::tests::successful_kernel_kill_never_signals_a_numeric_pid_or_group`; exact call counts are numeric PID `0`, process group `0`, kernel group `1` |

The delegated Linux command is:

```console
cargo test --offline -p hoimin-cli --test process_handler cgroup_v2:: -- --nocapture
```

It passed 7/7 in a privileged Linux cgroup-v2 container after moving the test
shell into the cgroup namespace's global root. The focused Linux backend command
also passed all 14 Linux-visible tests:

```console
cargo test --offline -p hoimin-cli resource::linux -- --nocapture
```

The normal macOS host has neither `/proc/self/mountinfo` nor Linux cgroups. A
private-cgroup-namespace container mounted cgroup2 read-write but could not
enable `+memory +pids` because its non-root delegated cgroup was populated
(`EBUSY`). That rejected probe was not counted as coverage. The host-cgroup
namespace fixture exposed `0::/`, writable cgroup2, and the `memory` and `pids`
controllers, and produced no capability skip.

## W1 follow-up proposal

Title:

```text
test(windows): exercise abnormal root exit through Job Object completion notifications
```

Body:

```markdown
Parent: https://github.com/tokyogas-tech/hoimin/issues/162

Exact audit row:

> Windows abnormal root exit crosses Job Object notification plumbing, classifies without timeout, and completes within a bound

The existing direct-state `RunState`/`record_notification` tests do not cross a
real Job Object completion port and therefore do not satisfy this row.

Acceptance criteria:

- On a Windows runner, start a real root assigned to the production Job Object.
- Have the runtime target atomically publish readiness, then terminate abnormally.
- Observe the real completion-port notification path and production classification.
- Assert the result is the platform's abnormal `ProcessTermination::Exit(...)`,
  never `ProcessTermination::Timeout`.
- Assert `ProcessHandler::handle(...)` and `ProcessHandler::close()` complete
  within a deterministic six-second bound and leave no assigned processes.
- Do not substitute `record_notification`, direct `RunState` mutation, or a
  synthetic PID for the real process fixture.

Keep this issue open and linked from #162 until a Windows CI URL demonstrates
the passing fixture.
```

## W2 follow-up proposal

Title:

```text
test(windows): bound cleanup when a root exits before assigned descendants
```

Body:

```markdown
Parent: https://github.com/tokyogas-tech/hoimin/issues/162

Exact audit row:

> Windows root exits while descendants remain assigned and cleanup stays bounded

The existing close-time descendant test does not exercise a real root that
exits first while its descendant remains assigned.

Acceptance criteria:

- On a Windows runner, assign a real runtime root to the production Job Object.
- Have that root start a long-lived assigned descendant and atomically publish
  both process identities before a coordinated zero exit.
- Observe the root's real exited process handle while the descendant's real
  process handle is still active; do not infer this boundary from synthetic state.
- Assert the root result is `ProcessTermination::Exit(0)`.
- Assert `ProcessHandler::handle(...)` and `ProcessHandler::close()` complete
  within a deterministic six-second bound.
- Assert the descendant process handle is no longer active and the Job Object
  has no remaining assigned processes after cleanup.
- Do not substitute direct `RunState` mutation or `record_notification` for the
  completion-port and cleanup paths.

Keep this issue open and linked from #162 until a Windows CI URL demonstrates
the passing fixture.
```

## W3 follow-up proposal

Title:

```text
test(windows): protect Job Object cleanup from a recycled real PID
```

Body:

```markdown
Parent: https://github.com/tokyogas-tech/hoimin/issues/162

Exact audit row:

> Windows stale/recycled PID cleanup captures a real exited PID and never signals the recycled raw PID

The existing generation tests use synthetic PIDs and direct state mutation.
They do not demonstrate the operating-system PID-reuse boundary.

Acceptance criteria:

- On a Windows runner, capture a real assigned process generation, including a
  durable process handle, let it exit, and retain its delayed cleanup state.
- Use a deterministic, bounded fixture to obtain a second real process with the
  same numeric PID but a different generation; an unbounded PID-churn or timing
  loop is not acceptable.
- Run cleanup for the old generation through the production Job Object path.
- Prove through real process handles that the new generation remains active and
  receives no termination request while the old generation is retired.
- Cross the real completion-port notification path; synthetic PIDs,
  `record_notification`, and direct `RunState` mutation do not satisfy the row.
- Bound the complete fixture and cleanup and attach its Windows CI URL.

Keep this issue open and linked from #162 until every acceptance criterion is
demonstrated. If deterministic real PID reuse is not enforceable in Windows CI,
retain this issue as the explicit platform remainder rather than closing #162
from synthetic coverage.
```

## Closure invariant

Before closing #162:

1. Create W1, W2, and W3 as three separate open issues and link each from #162.
2. Replace their placeholders in the traceability table with the actual issue links.
3. Attach the passing delegated `linux-cgroup-v2-hard` run URL to Linux rows 4 and 5.
4. Do not treat a skipped platform test or direct-state test as evidence for any row.
