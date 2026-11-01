# Issue 915 implementation plan

1. Add characterization for mixed LF/CRLF/CR sources at the public scoped resolver:
   tracked HEAD/context, explicit diff base, unborn index, and untracked files.
   Split line-count and range properties so each test has one behavior; retain
   independent oracle algorithms and shrinking bounds.
2. Run characterization before production edits under the shared build slot.
   Existing behavior is correct, so passing characterization is expected; do not
   invent a missing feature or report that baseline as a TDD failure.
3. Replace the map/boolean combination with private `GitChangedRanges` from the
   tracked producer and a consuming physical conversion. Keep the original
   algorithms and public types; merge physical unborn/untracked ranges afterward.
4. Run focused checks and two temporary sensitivity mutations: bypass conversion
   and feed unborn physical ranges through conversion. Restore immediately and
   verify each mutation fails for the intended observable row mismatch.
5. Review implementation in five passes, run workspace all-feature tests and all
   four prek checks under the guard, review verification evidence in five passes,
   update the existing selection knowledge page and hashes, and prepare a PR body.

## Plan review passes

1. Dependency pass: characterization precedes production changes; final suite
   follows restored mutation probes, so failing probes cannot leak into commits.
2. Behavior pass: mapped HEAD/context, explicit base, unborn, and untracked to
   separate tests; existing integration cases retain deletion/binary/rename scope.
3. Oracle pass: test literals and independent materialized text do not use the
   implementation cursor; bounded exhaustive cases cover non-UTF-8 bytes too.
4. Resource pass: all Cargo, prek and commit hooks wait for the parent build slot;
   each uses the task guard, 2 jobs, 2 test threads, and this tree's target directory.
5. Delivery pass: docs stay in plans (outside original-source catalogs); update
   existing selection concept, preserve unrelated source history, and report actual
   check results and platform limits. Parent owns push/PR creation.

## Sensitivity probes

- Remove the call to `lines::translate` in `GitChangedRanges::into_physical`,
  preserving the Git ranges as output. The tracked mixed-newline case must reject
  Git row 2 instead of physical rows 2–3.
- Feed the unborn branch's physical map into `GitChangedRanges::into_physical`.
  The bare-CR fixture has two physical rows but one Git row, so the source-bound
  check must reject this accidental second conversion.

Both probes target observable unit confusion. The consuming type is private to
Git resolution; no compile-fail test freezes its representation.
