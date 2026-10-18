- **Stop the second subscript builder after the first admission failure. — ADDRESSED.** `crates/hoimin-cli/src/analyzer/rust.rs:3590` now returns immediately when the neighbor dispatch leaves `self.candidates.failure` set, before mapping gates or `subscript_to_mapping_get_replacement` can execute. The successful path retains the previous dispatch, gates and builder order.
- **Required real helper-entry/allocation regression — ADDRESSED.** `crates/hoimin-cli/src/analyzer/rust/method_replacement_tests.rs:233` is one independently registered Arrange/Act/Assert case enabling both operators with a one-byte budget. It asserts `PayloadLimit`, zero retained bytes, one triggering generated candidate and zero helper entries/allocations (`:250`–`:253`). These counters observe the real mapping helper entry and the allocation immediately before `format!` (`crates/hoimin-cli/src/analyzer/rust.rs:4658`, `:4682`); the existing selected-family and direct-helper positive controls establish counter sensitivity.

### New Breakage in the Fix Diff

- None. The production change only stops continuation after a recorded failure; it does not change successful replacement construction, candidate ordering, ownership or accounting. The new test uses existing thread-local instrumentation and adds no compatibility adapter.

### Out-of-Scope Observations

- None.

### Evidence Checks

- Read `task-9-brief.md`, the fix1 appendix of `task-9-report.md`, the original Important finding in `task-9-review.md`, and the supplied `review-e4e3a1d4..4ebed445.diff` once. Inspected only the relevant existing counters, positive controls and their helper allocation boundary. No git or Cargo commands, test reruns, repository/index/HEAD mutations or subagents were used; only this ignored report was written.
- Checked `/tmp/hoimin-design-audit-5d3aee95/task-9-fix1-red.log` against the reported focused regression command. It reaches the final work-counter assertion and fails with one mapping-helper entry and one allocation; the preceding error, release and generation assertions pass. This demonstrates the specific pre-fix defect rather than relying on candidate counts.
- Checked `task-9-fix1-green.log`: the same named regression passes, 1 passed / 0 failed. Checked `task-9-fix1-focused.log` against the reported `payload_tests`, `method_replacement_tests`, index/slice and grouped-mapping filter command: 46 library and 13 direct-inclusion integration executions pass, including the new regression in both contexts and the relevant positive controls. No failures or ignored tests are reported.
- Checked `task-9-fix1-commit.log`: workspace/parser formatting and Clippy gates all report Passed; recorded commit is `4ebed445`, two files and 27 insertions. Existing e4e3a1d4 public measurements are explicitly preserved as earlier evidence, not claimed as fix-commit measurements.

### Verdict

- **Fix round: All findings addressed, no new Critical/Important breakage.** No open findings in the scoped fix from `e4e3a1d44ed390d95d4e605079c5ce4f6975926a` to `4ebed4457519920e9a28f8f653d8ce96985acdad`.
