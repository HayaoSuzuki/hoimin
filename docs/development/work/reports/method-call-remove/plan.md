# Method call removal implementation plan

> Execution uses executing-plans inline, with independent formal evidence and review tasks.

Goal: add a bounded opt-in attribute-call-to-receiver mutation in the existing Rust analyzer.
Spec: [design.md](design.md). Stack base: main 3596c98. Python 3.14, repository Rust
1.98.1. No product dependency, collector, external import or default-operator change.

- [x] Establish adjacent operator baseline; add a public CLI test and observe unknown-operator RED.
- [x] Register operator and behavioral ranking; add guarded collector branch and explicit-alias exclusions.
- [x] Validate precedence, source encodings, nested calls, receiver effects, exclusions, selector/profile/limit/cancellation and plan/verify/progress paths.
- [x] Check Lean model and broken controls under 20-second deadlines; compare every generated case through public CLI and compile replacements with CPython.
- [x] Run bounded real-source probes, five implementation/test review passes, independent review, full Rust tests, CI clippy/fmt and applicable Python/OKF checks. Commit docs/evidence, create PR, cargo clean before starting the next feature.

## Plan self-reviews

1. Red/green crosses the public parser boundary before production changes, and adjacent baseline distinguishes existing failures.
2. Collector work remains small and uses existing guards; no parallel Rust build in another worktree.
3. Observe actual evaluation count and lookup removal with caller-visible traces, not just replacement text.
4. Oracle expectations originate in Lean; parser/encoding boundaries are exercised independently by CPython and public plan tests.
5. Separate commit/worktree and resource cleanup precede the return-stub feature; docs and raw observations are reviewable in the PR.

Validation is recorded in [review.md](review.md). Delivery and cleanup follow the verified commit.
