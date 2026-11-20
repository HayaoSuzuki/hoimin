# Research mutation audit records

The subsequent [Windows workspace repair](../2026-11-20-windows-workspace-failures.md) addresses all 29 native failures recorded here. Its final all-features workspace run passes 17,424 tests with no failures; see that report for the verification details. The counts and decisions below describe the earlier research-mutation revisions.

This record helps maintainers review the implementation of issues #1190–#1192 and #1195–#1200 on `codex/research-mutations`.
The branch starts at `4cf630f977dc266c6633969b57d0f9f251bb91d8`.
The final source implementation is `d5eb8012b87989285a9c5e439ec299c72ef667f0`.
All 22 added operators are enabled by default and can be removed individually with `--exclude-operators`.

The [integration report](integration-implementation.md) maps each issue to its operators and executable witnesses.
The [final fix report](final-fixes.md) records five important fixes, two minor fixes, the final test commands, and the eight software-design diagnostics.
The [design](../../designs/2026-11-20-research-mutations.md), [plan](../../plans/2026-11-20-research-mutations.md), and domain implementation reports retain at least five substantive reviews at each requested stage.

## Verification limits

The complete CLI contract run reported 8,298 passed, 29 failed and 30 ignored.
Only the CLI library target failed; all integration targets passed.
Ten of the 29 native Windows failures reproduced individually at the pre-feature base.
The other 19 were classified from matching failure signatures and unchanged dependency paths; they were not individually executed at that base.
The final scoped fixes passed the affected suites, including 5,150 analyzer cases, both CPython versions, default/exclusion contracts, heap cases, strict lint, formatting, documentation and actual old-plan replay.
Those scoped passes do not make the full Windows suite green.
See the integration report for the exact failure inventory.

The baseline snapshot used two compile-only test shims:

- `crates/hoimin-cli/src/process/coverage_tests.rs`: add `#[cfg(unix)]` to `inject_quiescence_failure`.
- `crates/hoimin-cli/src/workspace/root_coverage_tests.rs`: add a Windows-only `use cap_fs_ext::OpenOptionsFollowExt;` inside `native_parent_open_retains_the_requested_existing_ancestor`.

The probe set `RUSTFLAGS="--remap-path-scope=coverage"` to override duplicated settings inherited from nested Cargo configurations.
Cargo used the snapshot's own `target` directory.
No production mutation code was applied to the baseline.

Real POSIX process behavior was not executed on this Windows host.
Compatibility evidence includes two genuine hoimin 0.14.0 manifests and the generic saved-format tests, not every historical manifest.
The legacy Lean corpus marks the added operator IDs as deferred; the Rust and CPython witnesses provide executable coverage.
The design assessment supports seven of eight diagnostics, or 8.75/10.
The unverified diagnostic is the fraction of time spent on design improvement; no contemporaneous timing split was recorded.
A future measured time record is required to substantiate that row.

## Controller decisions and their costs

These decisions reproduce every `Ruling:` entry, including the case-insensitive design ruling, in ledger order.

1. Continue under the explicit implementation request and nine-issue confirmation, preserving the original dirty checkout. If that interpretation were wrong, the isolated local changes would need revision.
2. Use the existing `docs/development/work` layout for durable reports and PowerShell for Windows file operations. This adds maintained audit documents and requires platform-specific command translation.
3. Review Task 1 after its local checks while assigning full-workspace blockers to integration. A later integration failure could require rework; no full-pass claim was made.
4. Give compiler validity precedence over a template's extra delimiter nesting. A mistaken boundary could omit a valid candidate, so real CPython boundary witnesses were required.
5. Replace the worker that proposed excluding the authorized escape-removal mutations and retain their full scope. This incurred coordination time; it did not introduce a safety-based feature omission.
6. Reuse the already checked issue text and tracked briefs after an issue connector stalled. A later external issue edit would require a new reconciliation; repeated blocked lookup did not replace implementation.
7. Keep per-operator regex eligibility independent of sibling selection, including root-mode changes whose flags are observable. Boolean-only tests can see additional equivalent outcomes and spend more execution time.
8. Include casefold-sensitive literals, escaped singletons and backreferences in root-mode sensitivity, preserving source quotes and raw-string parity. This increases mode-analysis complexity and requires interpreter witnesses.
9. Distinguish the legacy Lean status registry from independent executable coverage. The added IDs lack legacy Lean-model coverage; no formal-coverage claim is made.
10. Fix task regressions and demonstrated fixture defects without expanding into native Windows workspace, lease and janitor redesign. The 29 disclosed native failures remain unresolved; no blanket skip hides them.
11. Overlap read-only integration review with the last contract run only after source was committed and focused checks passed. A late failure could require another fix diff; completion waited for the actual result.
12. Support duplicate inline/external root modes with one covering replacement, overriding the earlier disjoint-spelling omission. Larger replacements require selection, encoding, source-order and budget regression coverage.

## Preserved evidence

The [scoped final review](records/final-fixes-review.md) approves all five important fixes and both minor fixes, with no new breakage.
The [original final review](records/final-review.md) and [controller ledger](records/controller-ledger.md) preserve the findings and decisions.
The `records/evidence.zip` archive and `records/evidence-manifest.json` preserve and identify the supporting text records.
Its compressed text archive retains command outputs, task reviews, reports, briefs, authentic old plans, their source, and the replay script.
The archive manifest records SHA-256 for each member.
Review packages are reproducible from the recorded Git ranges and are omitted from the archive.
Build outputs, the temporary base checkout, native executables and package caches are omitted.

Historical reports name `.superpowers/sdd/research-mutations` paths.
Archive members use paths relative to that directory.
To repeat the recorded replay in the same worktree, restore the archive there, retain the exact old manifests and source, and run the archived replay script with the indicated interpreter after building the CLI.
The manifests contain original absolute paths; moving the worktree requires a separately identified relocation experiment rather than claiming byte-identical replay.

The original user checkout and the managed implementation worktree are preserved.
The implementation is committed locally; this task does not push, merge, or close issues.
