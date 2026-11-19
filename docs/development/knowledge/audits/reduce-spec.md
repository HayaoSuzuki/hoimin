---
type: Audit
title: Reduce implementation and Lean oracle correspondence
description: Accepted reduce contract, production decision correspondence, native
  validation and remaining proof boundaries.
status: draft
sources:
- id: report
  resource: ../../work/reports/issue-1172/README.md
  working_tree: clean
  sha256: 50cdde221168b5898eacfda2187112fe442145cd159e774b38fad72ebd2c938b
  revision: 626da4b3f14b247cc9260ae8b6b1c9ead5fc6c7c
- id: clarifications
  resource: ../../work/reports/issue-1172/clarifications.md
  working_tree: untracked
  sha256: 007e65a15ac6e1213fb716c77eecd7bdc220ce0d973993ddbf970828c0532a54
- id: design
  resource: ../../work/reports/issue-1172/design-review.md
  working_tree: untracked
  sha256: c917032aaecefd407dc93ad3a322632124a67684397a50be725895d0ace21f6d
- id: model
  resource: ../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecModel.lean
  working_tree: untracked
  sha256: cd17bef5f5100385750529e501bdd6378bb9841cf4ee729adbe030dbb4e8a861
- id: proofs
  resource: ../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecProofs.lean
  working_tree: untracked
  sha256: 1da9be9b0a446e11fd62979dbd423c27dfe91c242dd0928924f6fbb7df1e819a
- id: corpus
  resource: ../../../../formal/HoiminOracle/corpus/reduce-spec.jsonl
  working_tree: untracked
  sha256: eab36d82024719cd69f59715f1335badf71ba9c1efe93011df93e35bbc060c6f
- id: implementation
  resource: ../../../../crates/hoimin-cli/src/reduce/session.rs
  revision: 626da4b3f14b247cc9260ae8b6b1c9ead5fc6c7c
  working_tree: clean
  sha256: e73b156554f4cd2226a65c3f53c28b50c17f09126d13376740df5083edd4dfc1
- id: policy
  resource: ../../../../crates/hoimin-core/src/reduce.rs
  revision: 626da4b3f14b247cc9260ae8b6b1c9ead5fc6c7c
  working_tree: clean
  sha256: 0184b49cbda6c198e6707efff4ade3e8657abf47da7f0c9c0698d34b47653e74
- id: adapter
  resource: ../../../../crates/hoimin-core/tests/lean_reduce_spec_oracle.rs
  revision: 626da4b3f14b247cc9260ae8b6b1c9ead5fc6c7c
  working_tree: clean
  sha256: 56f64b030b974b718eb04a08a5e1e2f95dfb9f00e742338d500c460a44b31302
- id: implementation-review
  resource: ../../work/reviews/2026-11-19-issue-1172-reduce.md
  revision: 626da4b3f14b247cc9260ae8b6b1c9ead5fc6c7c
  working_tree: clean
  sha256: 09c972ee966eceb2a558d134fee071607d94f6f88067972b0954071a92512655
- id: validation
  resource: ../../work/reviews/2026-11-19-issue-1172-reduce-validation.json
  revision: 626da4b3f14b247cc9260ae8b6b1c9ead5fc6c7c
  working_tree: clean
  sha256: dc322ac08b175b650ffa26f3f9b14094c5bc6bd32002e4e25084c5a094b70365
---

# Contract and scope

Issue #1172 defines a local, runner-independent search over explicitly selected Python definitions. The accepted supplement protects kept source ranges, permits only a strict 0–100% reduction threshold, stops on unusable observed times, and gives the deadline precedence over simultaneous completion. Unobserved timing slots are not zero observations.[^clarifications]

# Evidence

The Lean decision model has 22 kernel-checked theorems and a generated 952-row corpus. Bounded exploration covers seven events through depth three: 400 traces and 1134 transitions. Fourteen broken variants are detected. The all-trace artifact-safety theorem is not limited to that depth.[^report][^model][^proofs][^corpus]

The implementation now searches grouped removals, splits rejected groups, resets workers before every execution, freezes the original mutant timeout, and publishes a diff only for certified evidence. All 952 Lean rows are compared with the production policy as independent **internal-fixture** cases; the generated producer metadata remains unchanged. These checks include every expected component and intermediate trace state. They are not public strict runner correspondence.[^implementation][^policy][^adapter]

The implementation review records native macOS tests for editing, grouped search, timeout reuse, workspace metadata, source freshness, output ownership and byte-exact diff application. Its recorded full Rust/Python runs and final targeted reruns are historical evidence at the linked implementation revision, rather than checks of every later change. Linux/Windows hosted CI was not run locally.[^implementation-review][^validation]

The model assumes correctly bound evidence identities and source spans; Python editing and OS deadline enforcement remain outside the formal proof. During the original model audit, a whole-library build hit the process-tree RSS guard; targeted modules and an external proof consumer were checked separately.[^report]

# Design ownership

Source variants own selection, physical edit coverage and the exact diff. Evidence policy owns rates, observations and certification. The search session owns the absolute deadline and certified best result. These are responsibility boundaries, not a requirement to introduce three new classes. The original specification review scored 7/8 (8.75/10), with caller independence then unverified. The implementation review subsequently checked the owned boundaries and recorded the fixes and final diagnostic scores; these scores concern design complexity, not a proof of runtime correctness.[^design][^implementation-review]

# Recheck triggers

Re-run the oracle and review correspondence when changing candidate identity, result matching, retention ranges, threshold arithmetic, timing observations, deadline precedence or publication. Keep the production-policy adapter connected when changing the internal-fixture slice; public strict promotion requires a runner with the same complete premises. Do not treat model success as proof of Python transformation or production behavior.[^report]

[^report]: [Audit report](../../work/reports/issue-1172/README.md).
[^clarifications]: [Accepted clarifications](../../work/reports/issue-1172/clarifications.md).
[^design]: [Software design review](../../work/reports/issue-1172/design-review.md).
[^model]: [Lean model](../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecModel.lean).
[^proofs]: [Lean proofs](../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecProofs.lean).
[^corpus]: [Generated corpus](../../../../formal/HoiminOracle/corpus/reduce-spec.jsonl).

[^implementation]: [implementation](../../../../crates/hoimin-cli/src/reduce/session.rs).

[^policy]: [policy](../../../../crates/hoimin-core/src/reduce.rs).

[^adapter]: [adapter](../../../../crates/hoimin-core/tests/lean_reduce_spec_oracle.rs).

[^implementation-review]: [implementation-review](../../work/reviews/2026-11-19-issue-1172-reduce.md).

[^validation]: [validation](../../work/reviews/2026-11-19-issue-1172-reduce-validation.json).
