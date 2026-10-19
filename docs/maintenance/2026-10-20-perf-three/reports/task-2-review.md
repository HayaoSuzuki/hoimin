### Spec Compliance

- ✅ Spec compliant for Task 2, base `4c906846e4515735d7e89c17a2b374483ba299ff` through head `e5e33603a4ed6a269d4131fbb6dbe99a409de05c`. Private immutable storage contains entries, directories and total together; derived structural equality and the existing construction boundary remain (`crates/hoimin-cli/src/workspace/manifest.rs:97`, `:102`, `:227`). The production change outside this module is the required const removal (`crates/hoimin-cli/src/workspace/copy.rs:512`).
- ✅ Required focused test/evidence surfaces are present: standalone and actual-worker retained-heap binaries (`crates/hoimin-cli/tests/workspace_manifest_heap.rs:5`, `crates/hoimin-cli/tests/workspace_worker_metadata_heap.rs:5`), sibling failure/charge preservation and worker reset after plan destruction (`crates/hoimin-cli/src/workspace/copy.rs:1935`), pending-cleanup metadata checks (`crates/hoimin-cli/src/workspace/mod.rs:1936`), and fixed-workload provenance (`docs/maintenance/2026-10-20-issue-1007/verification.json:6`).
- ⚠️ Native Windows lock/cleanup preservation and final whole-branch integration remain controller checks, explicitly excluded from local claims (`docs/maintenance/2026-10-20-issue-1007/README.md:39`). No conclusion about RSS or physical disk allocation follows from this evidence (`README.md:9`).

### Strengths

- Storage has no filesystem or charge authority and no mutation/Arc exposure; accessor and content-comparison changes simply forward to the immutable data (`crates/hoimin-cli/src/workspace/manifest.rs:92`, `:110`, `:137`). Existing plan/worker field types and lifecycle ordering have no production hunks.
- Both regression gates fail on the old implementation with retained owners: 7,510,400 clone bytes and 474,312 actual-worker bytes (`/tmp/hoimin-perf-three-task2/red.log:10`, `/tmp/hoimin-perf-three-task2/red-worker.log:9`). Setup/output allocation is outside the observation window, and retained and peak readings are distinct (`docs/maintenance/2026-10-20-issue-1007/probe.rs:9`).
- Literal and generated real-file expectations cover structural equality, byte/hash/size views and handle drop order; private Weak checks cover final release and detached blocking ownership (`crates/hoimin-cli/src/workspace/manifest.rs:443`). Worker mutation checks the sibling bytes and original expected hashes (`crates/hoimin-cli/src/workspace/copy.rs:1935`).
- Measurement reporting separates metadata retention, time and logical payload/charges, including timing regressions and below-resolution clone samples (`docs/maintenance/2026-10-20-issue-1007/README.md:21`, `:25`, `:37`). The diff's 144 observation rows contain 72 samples per revision and stable retained results across payload sizes; they support the stated narrow metadata conclusion.

### Issues

- Critical: none.
- Important: none.
- Minor: none requiring a change in this task gate.

### Assessment

**Task quality:** Approved.

**Reasoning:** The change localizes sharing in the existing metadata abstraction while keeping cleanup and accounting in their original owners. Deterministic heap gates discriminate the prior inefficiency, and focused content/lifetime controls support the intended preservation claims.

- Review method: read the supplied diff once in bounded portions, compacting the large measurement-only portion; no diff regeneration, git mutations, builds or test reruns.
- One focused unchanged-code inspection: the new cancellation test asserts exactly one strong owner while materialization is paused. Checked only the pause placement (`crates/hoimin-cli/src/workspace/copy.rs:673`); it occurs before file copying and final manifest cloning, so that assertion matches the ownership phase.
- Existing receipts inspected: GREEN isolated heaps (`/tmp/hoimin-perf-three-task2/green-heap.log:10`, `:18`), workspace 240 passed/7 existing ignored (`final-workspace.log:273`), recovery and heap binaries (`final-recovery-heap.log:9`, `:17`, `:27`, `:57`, `:73`, `:81`), materialization 9 passed/1 existing ignored (`final-materialization.log:17`), separate cancellation release (`final-cancel-release.log:8`), and normal fmt/clippy hooks (`commit-hooks.log:2`). No unexpected warnings/errors in these GREEN receipts.
