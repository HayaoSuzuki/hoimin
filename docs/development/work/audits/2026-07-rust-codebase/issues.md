# Issue Map

| Finding IDs | GitHub issue | Priority | Depends on | Worktree branch | Status |
| --- | --- | --- | --- | --- | --- |
| RUST-AUDIT-001 | https://github.com/tokyogas-tech/hoimin/issues/23 | P1 | none | `fix/issue-23-contain-windows-portable-children` | issue_created |
| RUST-AUDIT-003 | https://github.com/tokyogas-tech/hoimin/issues/24 | P1 | none | `fix/issue-24-reap-after-termination-errors` | issue_created |
| RUST-AUDIT-007 | https://github.com/tokyogas-tech/hoimin/issues/25 | P1 | none | `fix/issue-25-reject-progress-set-mismatch` | issue_created |
| RUST-AUDIT-002 | https://github.com/tokyogas-tech/hoimin/issues/26 | P2 correctness | none | `fix/issue-26-preserve-reservation-identity` | issue_created |
| RUST-AUDIT-005 | https://github.com/tokyogas-tech/hoimin/issues/27 | P2 correctness | none | `fix/issue-27-validate-plan-configuration` | issue_created |
| RUST-AUDIT-004 | https://github.com/tokyogas-tech/hoimin/issues/28 | P2 design | none | `refactor/issue-28-capability-relative-workspace-paths` | issue_created |
| RUST-AUDIT-008 | https://github.com/tokyogas-tech/hoimin/issues/29 | P2 design | none | `refactor/issue-29-progress-stall-policy` | issue_created |

## Recommended execution order

1. **Correctness and cleanup prerequisites:** #23, #24, and #25 are the P1 roots; then #26
   and #27 address the P2 confirmed correctness roots. All five are mutually independent.
2. **Shared-boundary refactors:** #28 makes workspace operations capability-relative. It has
   no predecessor, but follows the correctness wave in the recommended priority order.
3. **Independent subsystem refactors:** #29 resolves the progress stall-policy contract. It
   has no predecessor and is independent of the workspace boundary.
4. **Maintainability-only follow-ups:** none. No maintainability candidate survived Task 9
   validation.

The table's `none` dependencies are intentional: Task 9 established that no accepted root
changes an interface or invariant required by another. RUST-AUDIT-006 was rejected and receives no
issue.
