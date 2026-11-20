# Windows workspace failure implementation plan

> For agentic workers: use systematic-debugging, test-driven-development and executing-plans. Root performs edits and coordinated verification; independent read-only diagnosis informs the changes.

**Goal:** Resolve the 29 native Windows workspace failures remaining after the research mutation implementation, preserving ownership and cancellation contracts.

**Architecture:** Keep native locks, sharing policies and exact-object deletion authority unchanged. Observe locked test evidence through its owning handle. Exercise real Windows fault boundaries and reject unproven absence before file creation. Repair production behavior where a test identifies a missing guarantee.

**Tech stack:** Rust 1.100, cap-std, Windows native handles, rstest, existing bounded proptest suites.

**Spec:** User request to fix the Windows failures; original inventory in research-mutations/integration-implementation.md and archived command receipts. This request supersedes the earlier decision to leave native failures outside scope.

## Constraints and review focus

- Retain 22 default-enabled mutation operators and all previous work on this isolated branch.
- Do not weaken locks, delete-sharing policy, identity checks, cancellation, or exact-object deletion authority to make fixtures pass.
- Each test invocation covers one semantic case, arranged before its action and assertions. Use named parameterized cases and existing meaningful generated ownership invariants.
- Real Windows tests require the native host: sandbox publication returns invalid-handle error6 before the tested operation, unlike the original native run. Record both observations.
- Preserve original failure counts as historical evidence; record actual final suite outcomes separately.

## Design self-review

1. Lock evidence belongs to the lock-owning file description; an independent path reopen tests a different authority on Windows.
2. Name replacement must use genuine retained rename authority or occur after the original capability is released; keep identity mismatch assertions.
3. Cancellation retry requires the same kind of deletion authority as initial production cleanup; never manufacture ByName fallback after pin transfer.
4. Missing-path interpretation must validate the parent and propagate its inspection failure before mutation.
5. Fault injection belongs at reachable semantic native boundaries; Unix mode-bit checks and Windows DACL checks differ. Native security behavior must retain coverage rather than disappear behind broad skips.

## Plan self-review

1. Reproduce all workspace failures centrally before changes and separate sandbox-only setup failures from native failures.
2. Trace each exact failing line; retain stronger lock/pin contracts and replace incompatible fixture setup or observation only.
3. Existing failing cases provide RED; add focused cases where a shared production change introduces a new boundary or invariant.
4. Test root and owned-workspace groups, then CLI contracts and the whole workspace with the same two-job/two-thread CI artifact settings.
5. Review every amended file and all 29 dispositions, record five implementation and test reviews, and obtain a fresh final review before claiming completion.

## Tasks

- [x] Reproduce the native workspace suite and record the exact failure set.
- [x] Restore missing-entry parent validation on the Windows write path; run the existing failure and neighboring root contracts.
- [x] Correct locked evidence, retained rename, janitor fixture lifetime and cancellation/owner-slot fixtures with preserved assertions.
- [x] Exercise applicable native bootstrap/publication fault boundaries and require retry/rollback evidence.
- [x] Run affected tests, strict lint, formatting, full workspace and CLI contracts; investigate any remaining failures.
- [x] Record implementation/test reviews, software-design diagnostics, final independent review and local commits.
