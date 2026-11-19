---
type: Audit
title: Reduce specification and pre-implementation oracle
description: Accepted reduce boundaries, Lean model evidence, design ownership and
  unimplemented correspondence.
status: draft
sources:
- id: report
  resource: ../../work/reports/issue-1172/README.md
  working_tree: untracked
  sha256: eae26641341e362ba0f9eac84bf1e2fd5f0421ca167b99151a39d2baacd1a965
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
---

# Contract and scope

Issue #1172 defines a local, runner-independent search over explicitly selected Python definitions. The accepted supplement protects kept source ranges, permits only a strict 0–100% reduction threshold, stops on unusable observed times, and gives the deadline precedence over simultaneous completion. Unobserved timing slots are not zero observations.[^clarifications]

# Evidence

The Lean decision model has 22 kernel-checked theorems and a generated 952-row corpus. Bounded exploration covers seven events through depth three: 400 traces and 1134 transitions. Fourteen broken variants are detected. The all-trace artifact-safety theorem is not limited to that depth.[^report][^model][^proofs][^corpus]

Every corpus case remains model-only. There is no product reduce implementation or same-premise adapter. The model assumes correctly bound evidence identities and source spans; Python editing and OS deadline enforcement are excluded. A whole-library build hit the existing process-tree RSS guard; targeted modules and an external proof consumer were checked separately.[^report]

# Design ownership

Source variants own selection, physical edit coverage and the exact diff. Evidence policy owns rates, observations and certification. The search session owns the absolute deadline and certified best result. These are responsibility boundaries, not a requirement to introduce three new classes. The design review scores the specification 7/8 (8.75/10), leaving caller independence unverified until implementation exists.[^design]

# Recheck triggers

Re-run the oracle and review correspondence when changing candidate identity, result matching, retention ranges, threshold arithmetic, timing observations, deadline precedence or publication. Connect the actual implementation before promoting any row to strict or internal-fixture. Do not treat model success as proof of Python transformation or production behavior.[^report]

[^report]: [Audit report](../../work/reports/issue-1172/README.md).
[^clarifications]: [Accepted clarifications](../../work/reports/issue-1172/clarifications.md).
[^design]: [Software design review](../../work/reports/issue-1172/design-review.md).
[^model]: [Lean model](../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecModel.lean).
[^proofs]: [Lean proofs](../../../../formal/HoiminOracle/HoiminOracle/ReduceSpecProofs.lean).
[^corpus]: [Generated corpus](../../../../formal/HoiminOracle/corpus/reduce-spec.jsonl).
