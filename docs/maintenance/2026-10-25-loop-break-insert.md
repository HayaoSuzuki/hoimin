# Loop break insertion validation

Scope: Windows, CPython 3.14.9 and 3.15.1, Rust 1.99.0; baseline
`29e7df70048e1d0d086594b12166e13354653ee0`. No parser/dependency changes.

## Design self-reviews

1. Confirmed the accepted body-tail insertion and opt-out policy.
2. Distinguished tail-reached termination from conditional continue paths.
3. Scoped eligibility to for, async for and while; direct abrupt exits excluded.
4. Chose nonempty anchors, preserving existing candidate admission semantics.
5. Checked explicit selections, saved plans, budgets and dedicated-worktree scope.

## Implementation self-reviews

1. Reviewed loop/else and nested-loop termination semantics.
2. Checked inline suites, compound tails, comments and EOF.
3. Checked UTF-8, tabs and CRLF source boundaries.
4. Checked enclosing-symbol metadata and shared-anchor prefix order.
5. Found the physical-newline/backslash bug; reproduced RED and repaired it with
   borrowed parser logical-newline tokens (all five regression tests GREEN).

## Validation self-reviews

1. CPython runtime checks cover first tail-reached exit and suppressed else.
2. Runtime fixtures include nested loops, inline suites and async for, LF/CRLF;
   the dedicated integration test passed on both supported interpreters.
3. Default inclusion, individual exclusion, explicit subsets and saved plans:
   three tests passed independently with each interpreter.
4. Existing fuzz target selects every registered operator; added a loop seed.
   No local ASAN campaign was run on Windows.
5. Fresh independent review validated 112 additional CPython 3.14 mutants across
   compound/decorated/class/multiline/EOF layouts. It found a stale CLI defaults
   fixture; no other production defect was reported.

## Review-fix self-reviews

1. Reproduced the stale default fixture failure before correction.
2. Updated the count and name together, preserving declaration order.
3. The full workspace run exposed the ranking inventory's fixed count; corrected
   it together with the usage-text assertions.
4. Clippy exposed checked-conversion and empty-assert style violations in new
   tests; corrected the tests without changing production behavior.
5. Required a fresh full workspace and Clippy run after those corrections.

## Verification evidence

- Missing operator RED; initial four analyzer regressions GREEN.
- Trailing explicit-continuation RED; logical-newline repair GREEN (five tests).
- Core/analyzer suite passed before the final two edge-case regressions.
- CPython integration: passed on 3.14.9 and 3.15.1.
- Public default/exclude/explicit/saved-plan tests: 3/3 on each interpreter.
- Python suite: 1,255 passed, 53 skipped, default development environment.
- Ruff lint/format and ty: passed (ty required the documented fuzz dependency group).
- Documentation site build: passed. OKF structure: 60 pages; reference inventory:
  no missing links/sources or unreachable pages.
- Full workspace and final Clippy: pending at this checkpoint.

## Resource policy

Build jobs limited to two; incremental compilation and Rust debug info disabled.
Free physical memory and disk observed throughout; roughly 100 GiB disk remained
at the validation checkpoint. Final cleanup and delivery evidence is added below.
