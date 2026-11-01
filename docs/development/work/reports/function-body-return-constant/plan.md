# Return constant implementation plan

Execution uses executing-plans inline with independent Lean and final review tasks.
Base: method-call-remove 1865f47, after its cargo clean removed 3.8 GiB. Dedicated
worktree/branch: function-body-return-constant. Spec: [design.md](design.md).

- [x] Observe a public unknown-operator RED test; register the opt-in ID and all/ranking/inventory documentation.
- [x] Extend own-scope body analysis and use deferred builtin annotation resolution; preserve source ranges and skip literal no-ops.
- [x] Exercise all pairs, binding/scope boundaries, encodings/docstrings, selectors/profile/limits/cancellation, CPython and saved-plan verification.
- [x] Generate expectations in Lean, prove model contracts and detect broken controls under 20-second/2-GiB limits; compare every strict case through public CLI.
- [x] Record trials and five implementation/test self-reviews; independent review; full Rust and CI quality/Python/OKF checks; commit, create stacked PR, cargo clean.

## Plan self-reviews

1. The first failing test crosses the public CLI registry before any implementation changes; adjacent body-erasure regressions are checked after extending shared scope analysis.
2. Annotation-name resolution reuses tested deferred semantics and avoids a new runtime/type inference layer. Shared visitor changes must leave existing consumers unchanged.
3. Runtime tests distinguish result assertions from calls alone; test both complementary mutants, exact body spans and unchanged headers/docstrings.
4. Lean owns expected candidates; Rust adapter only observes public output. Include no-op/async/generator/annotation/own-scope negative controls and compile every generated mutant.
5. No parallel Rust build in another feature worktree, no external import features, no default changes. Complete documentation/PR and remove build outputs before final handoff.

Final validation is recorded in [review.md](review.md). PR/stack delivery and cleanup follow the verified commit.
