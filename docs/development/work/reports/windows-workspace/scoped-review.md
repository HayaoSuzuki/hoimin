# Follow-up review of the named-entry observation

Reviewed the frozen `review-fix-diff.txt` delta to `3514ecdb`.

## Finding disposition

**Important finding resolved.** `read_fixture_bytes` inspects the named entry without following a symlink, requires a regular file and compares the entry identity with the retained handle identity. The byte read uses the lock owner and restores its original file position. All eleven updated observations supply the expected parent and final name. The coordinator contention case uses the independently locked coordinator handle. The remaining lease observations refer to the corresponding managed root and its retained lease.

The two new rstest invocations independently exercise a missing name and a replacement with identical bytes. The old helper reached the final assertion and failed both tests. The replacement case requires object identity to distinguish the two files. The scoped diff changes only test code.

## Issues

| Severity | Open findings |
| --- | --- |
| Critical | None |
| Important | None |
| Minor | None |

## Design diagnostics

**8.75/10.** The fix encapsulates name-to-handle verification, closing the sixth diagnostic in the initial review. Seven of eight diagnostics now have supporting evidence. Contemporaneous design-effort timing remains unavailable.

## Declined to judge

- The full workspace run and final completion gate belong to the coordinator.
- A native Unix execution was outside the scoped Windows recheck.

## Assessment

The scoped change is approved. `review-green.log` records 697 affected Windows test invocations, with 689 passed, zero failed and eight existing ignored fixtures in 101.79 seconds. The coordinator recorded exit 0. All 29 original failures and both new regression cases are covered by this group. The full all-features workspace and final strict lint remain separate completion gates.
