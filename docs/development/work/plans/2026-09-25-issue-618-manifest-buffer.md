# Issue 618 implementation plan

Use `perf/issue-618-manifest-buffer`, created from current main `c7b7a5b`. The user authorized implementation through publication; the coordinator owns CI polling, merges and worktree cleanup.

1. Commit this plan and design before code/test edits.
2. Add a dedicated allocator/lifetime integration test using valid public-plan candidates, fixed 24-candidate count and 256-KiB literal payloads. Isolate tracking around a single poll of public prepare with its source read held behind the occupied blocking worker. Assert the raw buffer is freed, then resume and verify one/multiple selections and public dry-run output without executing the test marker. Add bounded channel/subprocess waits and RAII release.
3. Run the regression against unchanged production and confirm failure is the live raw buffer, not fixture parsing or scheduling. Preserve this evidence.
4. Add the explicit buffer drop after typed conversion. Run GREEN and existing plan validation tests covering malformed headers, unknown fields, candidate tampering and source/fingerprint mismatches.
5. Perform three actual implementation reviews and three test reviews; record findings. Run full workspace tests, exact CI workspace and parser clippy, both fmt checks and diff checks. No Lean model or Python mutation run is needed for this Rust allocation-lifetime change.
6. Obtain independent review, address findings, commit implementation/evidence, and publish via `gh stack`. Keep no more than two active issues in this lane and rebase unpublished work onto current main if needed.

## Plan self-review

1. Checked the public async path: unchanged-target resolution is synchronous, then `source_records` awaits Tokio filesystem work. Occupying the only blocking worker makes the observation boundary deterministic without replacing production code.
2. Checked the allocator test's scope: fixture creation, runtime setup and output assertions stay outside pointer tracking; assertions require a unique manifest-sized allocation and actual deallocation before the source read. A whole-prepare memory limit is intentionally avoided because issue 608 changes its later peak.
3. Checked failure cleanup and resources: release the blocking worker even on assertion panic, retain a bounded receive deadline, and use existing controlled Python for the never-executed marker command. Clean only local core/CLI cache artifacts before building in the new worktree, with one Cargo job and no Lean invocation.
