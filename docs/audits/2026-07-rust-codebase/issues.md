# Issue Map

| Finding IDs | GitHub issue | Priority | Depends on | Worktree branch | Status |
| --- | --- | --- | --- | --- | --- |
| RUST-001 | pending Task 10 | P1 | none | pending | accepted |
| RUST-003 | pending Task 10 | P1 | none | pending | accepted |
| RUST-007 | pending Task 10 | P1 | none | pending | accepted |
| RUST-002 | pending Task 10 | P2 correctness | none | pending | accepted |
| RUST-005 | pending Task 10 | P2 correctness | none | pending | accepted |
| RUST-004 | pending Task 10 | P2 design | none | pending | accepted |
| RUST-008 | pending Task 10 | P2 design | none | pending | accepted |

## Recommended execution order

1. RUST-001, RUST-003, and RUST-007 are the P1 roots. They are mutually independent and may
   proceed in parallel; their table order is not a dependency.
2. RUST-002 and RUST-005 are P2 confirmed-correctness roots. They may proceed independently
   of the P1 work and of each other.
3. RUST-004 and RUST-008 are the remaining P2 design roots and have no predecessors. For
   RUST-008, remediation must choose either (a) reset on adjacency-breaking same-set
   regression and status-induced indeterminate transitions, or (b) rename and document the
   policy as cumulative since improvement and align its tests.

RUST-006 is rejected and receives no issue. Task 10 will replace each pending issue and
branch cell after creating the corresponding GitHub issue.
