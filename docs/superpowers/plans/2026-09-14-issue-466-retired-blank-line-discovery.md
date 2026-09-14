# Issue 466 disposition plan and evidence

Spec: ../specs/2026-09-14-issue-466-retired-blank-line-discovery-design.md

## Plan and execution

- [x] Create an issue-specific worktree from `origin/main`.
- [x] Inspect the historical regex, whole-source enumeration and candidate-limit ordering.
- [x] Verify removal ancestry, tracked-tree absence, active-reference absence and current cargo-mutants policy.
- [x] Record the prior removal as supersession in design, plan and OKF history without restoring code.
- [x] Run bounded import, current skill contracts, OKF validation and document checks.
- [x] Perform final reviews; commit, push and PR publication follow this evidence update.

## Plan self-review

1. The investigation establishes whether the affected command is reachable before considering an implementation change.
2. Historical inspection covers both regex behavior and why the existing timeout/candidate bounds did not interrupt it.
3. The reviewable outcome is a documentation PR; issue closure remains a reviewer decision and no removed dependency returns.

## OKF self-review

1. The architecture concept is the existing home for retired focused-mutation behavior, avoiding a duplicate concept page for one historical defect.
2. The new source uses a content hash and a local relative link, and both design indexes remain reachable from the OKF root.
3. The Japanese catalog paragraph separates observed historical behavior, verified current state and conditions for any future integration.

## Implementation self-review

1. Commit `2f27e2a` removed discovery, its command entry and supporting package, so no live execution path remains to optimize.
2. The documentation records the exact historical ordering: whole-source `findall` precedes per-result candidate checks.
3. The publication diff contains only documentation and index evidence; it does not revive cargo-mutants or the vulnerable expression.

## Test self-review

1. The removed import was run in a subprocess bounded to five seconds; it returned immediately with `ModuleNotFoundError`, confirming unreachability without executing historical pathological input.
2. All three current development-skill contract tests passed, confirming the active repository workflow remains internally consistent after cargo-mutants removal.
3. The OKF validator passed all 16 pages for YAML, reserved files, source footnotes and local links; `git diff --check` also passed. These checks validate documentation structure, not replacement runtime scaling.

## PR self-review

1. The title and body describe supersession evidence and explicitly state that no runtime implementation was restored.
2. The PR references Issue 466 without an automatic closing keyword, leaving closure to reviewer judgment as requested.
3. The final diff is limited to one design, one execution plan and updates to the existing architecture concept and design-source index; historical performance numbers are attributed to the issue rather than presented as rerun measurements.

## Final evidence

- Removal ancestry: `git merge-base --is-ancestor 2f27e2a HEAD` succeeded.
- Current tree: tracked-path and live-reference searches found no removed discovery entry point.
- Historical source: `2f27e2a^` contains newline-consuming `^\s*` and whole-source `_FUNCTION.findall(source)` before candidate-limit enforcement.
- Bounded import: failed immediately with `ModuleNotFoundError: No module named 'tools'`.
- Current tests: `python3 -m unittest discover -s tests -p test_skills.py -v` passed 3/3.
- Documentation: the OKF validator passed 16 pages and `git diff --check` passed.

## Follow-up at 8b33167

The original tasks above describe the earlier disposition PR. Execute this
follow-up in `enhancement/issue-466`; the user authorized autonomous publication.

- [x] Recheck `git merge-base --is-ancestor 2f27e2a HEAD`, the tracked retired
  package and `rg` callers in active sources; inspect the historical regex.
- [x] Map each acceptance condition in the existing spec to retirement, without
  claiming replacement performance or compatibility.
- [x] Run `python3 -m unittest discover -s tests -p test_skills.py -v` and a
  five-second subprocess import probe; no retired mutation tool is executed.
- [x] Record three actual review passes for OKF, design, plan, implementation,
  verification and PR preparation in `../reports/2026-09-14-issue-466-revalidation.md`.
- [x] Update architecture and both source indexes with checked provenance;
  validate YAML, links, source IDs, hashes and source-list completeness.
- [x] Check the final diff, commit, push and publish a documentation PR that
  explicitly closes the obsolete issue by retirement.

Published follow-up: https://github.com/tokyogas-tech/hoimin/pull/534
(initial evidence commit `e1e3806`).
