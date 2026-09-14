# Issue 457 implementation plan and evidence

Spec: ../specs/2026-09-14-issue-457-fingerprint-shared-walk-design.md

## Plan

- [x] Add regression tests for one walk, equivalent records and argument-ordered failures. The issue's 1/10/30-pattern measurement is the performance RED; the previous implementation structurally called `resolve_one` once per pattern.
- [x] Replace per-pattern `resolve_one` traversal with compiled pattern states and one union traversal.
- [x] Run focused tests, release measurement, formatting, Clippy and workspace tests.
- [x] Record implementation, test and PR reviews, commit, push and open the issue PR.

## Plan self-review

1. The performance assertion counts traversal construction rather than wall-clock time, so normal CI detects the original pattern-count multiplier.
2. Error cases cover earlier unmatched before later invalid and earlier selected unsupported before later invalid; neither can be replaced by compile-all fail-fast behavior.
3. Scope stays in fingerprint glob resolution. Manifest recheck, exact-file reads, hashing and output schema do not change.

## OKF self-review

1. The new concept states only the fingerprint include contract and links the issue-specific design and implementation source.
2. Current behavior, proposed decision and measured evidence remain distinct; release timings will include host and command details.
3. YAML parsing, footnotes, local links and root-index reachability are explicit final checks.

## Implementation self-review

1. Call graph: `resolve` invokes `resolve_patterns` once; `resolve_patterns` constructs one `WalkBuilder` after compiling the valid prefix. No pattern loop contains `build()`.
2. Error order: the first invalid glob is deferred until every preceding valid pattern has a match or error. Unsupported entry errors are stored on each matching pattern and replayed by input position.
3. Filesystem boundary: the walker receives a union override and retains `hidden(false)`, disabled ignore files, `parents(false)` and `follow_links(false)`. Exact-file hashing code is unchanged.

## Test self-review

1. Performance sensitivity: the unit regression increments the walk count adjacent to the actual `build()` call and covers 0/1/2/4 patterns with 0 and 64 unrelated entries. A repeated-build implementation increments beyond one.
2. Compatibility: integration tests cover overlaps and assert exact error text when an earlier unmatched or unsupported pattern precedes a later invalid pattern.
3. Existing coverage: the focused suite still exercises duplicate globs, basename/root-relative matching, escaped metacharacters, ignored/hidden files, symlinks, non-UTF-8 paths, unreadable files and exact overlap.

## PR self-review

1. Scope: the diff changes only fingerprint glob resolution, its tests and issue-specific design/knowledge records; no manifest, hashing or CLI schema changes are present.
2. Evidence: focused tests passed 26/26; scoped all-target/all-feature Clippy passed; full workspace/all-features passed 1,638 tests with 13 ignored and no failures.
3. Reviewability: the design explains the non-obvious deferred-invalid rule, the OKF page is reachable from both indexes, and the untracked `.venv` links remain excluded.

## Release evidence

Rust 1.98.0 release build on macOS arm64, 10,000 unrelated files and 30 matching root files. Three `/usr/bin/time -p` runs after build gave 0.33/0.02/0.02 seconds for one pattern (first run cold) and 0.04/0.04/0.03 seconds for 30 patterns. Both cases produced a valid one-candidate plan. These figures support the structural one-walk assertion; they are not a CI threshold or a claim that matcher comparisons are independent of pattern count.

The first full-suite attempt failed seven `analysis_depth` cases because the new worktree lacked its expected `.venv/bin/python`. After linking the worktree-local ignored `.venv/{bin,lib,pyvenv.cfg}` to the repository environment and placing that bin directory on `PATH`, the fresh full run passed. No product change was made for this environment failure.

## Independent review fixes

1. Reproduced the cross-pattern negation defect: `*.toml`, then `!a.toml` incorrectly reported the first positive pattern unmatched. The union now contains positive patterns only; independent matchers retain negative semantics.
2. Replaced the returned literal traversal count with an increment next to the actual `WalkBuilder::build` call and expanded the gate across 0/1/2/4 patterns and two unrelated-tree sizes.
3. Rechecked traversal errors. Walker construction/iteration errors still fail the active valid prefix immediately, matching the former first-pattern traversal for the paths selected by its positive union. The selected-entry unsupported errors remain stored per matching pattern so argument order wins over a deferred invalid glob. Native unreadable-directory ordering is platform-dependent and is not presented as a deterministic performance assertion.

## Publication and CI evidence

- Commit `4446105e26478e4c4ee8b7a11e6635afe437c15a` was pushed as `perf/issue-457-fingerprint-shared-walk` and published in PR [#522](https://github.com/tokyogas-tech/hoimin/pull/522).
- Quality, MSRV, Ubuntu Rust, Lean audit, randomized-order Rust, core dependency purity, wheel smoke and Linux best-effort jobs passed for that head. The hard-cgroup job was skipped by workflow policy.
- The first contracts job failed in the existing timing-sensitive `shell::tests::blocked_monitor_join_defers_both_roots_without_recursive_cleanup` test. Its rerun passed without a source change; the fingerprint implementation and focused regression were not named in the failure.
