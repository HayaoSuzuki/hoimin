# Issue 608 implementation plan

Work in `perf/issue-608-borrowed-ranking`, based on main `bf09c91`. The user authorized design, implementation, verification, commits and publication; the coordinator owns CI polling, merge and cleanup.

1. Commit the reviewed design and this plan before code/test changes.
2. Add an isolated allocator regression compiling the actual private ranking module. Construct public-plan fixtures with 64 flat list literals and vary payload sizes (32 bytes, 32 KiB, 256 KiB); setup, serialization, target resolution and assertions stay outside measurement. Require validation success and bounded extra heap, with an eager-clone sensitivity control. Run against the old implementation to demonstrate RED.
3. Add semantic regressions comparing validation with the prior clone-and-rank equality oracle across metadata tampering, ordering fields and equal-key ties. Add public plan→verify dry-run coverage selecting one and multiple candidates, checking IDs/ranks/details and absent test marker; corrupt an unselected candidate's coherent ranking to prove all retained candidates are validated.
4. Extract shared ranking context and replace clone/re-sort revalidation with borrowed metadata and adjacency checks. Preserve plan generation sorting and error text. Add a narrow README explanation of all-candidate ranking validation without body duplication.
5. Run focused GREEN checks, then three implementation and three test self-reviews. Run the full workspace suite, both exact CI clippy commands, both fmt checks and diff checks. Record measured bytes and all results.
6. Obtain independent review, address findings, commit implementation/evidence, and publish with `gh stack` after successful verification. Keep at most two active worktrees/PRs in this lane.

## Plan self-review

1. Checked that the memory regression uses genuine public-plan candidates, fixed candidate count and repeated deterministic allocator measurements. Measuring only revalidation excludes manifest decoding and candidate creation from the performance claim.
2. Mapped behavior risk to tests: all ranking metadata, score and every comparator tie-breaker, stable ties, empty input, changed/line/symbol contexts, and an unselected tampered row. Existing literal ranking/category tests remain the independent scoring oracle.
3. Checked resource and validation scope: use the assigned cache with one job and clean only local core/CLI artifacts on switching worktrees. No new Python source/tests or Lean command is needed. Public CLI subprocesses use the existing bounded helper, and the final record distinguishes local checks from remote CI.
