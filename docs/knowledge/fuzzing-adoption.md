---
type: Decision
title: Fuzzing maintenance adoption
description: Evidence gates for corpus distillation, property reuse and targeted exploration.
status: draft
sources:
- id: assessment
  resource: ../superpowers/reports/issue-805/README.md
  working_tree: untracked
  sha256: 8e97670911b94e411881ef6f4d0f66bd6180b582592d7c3e204f0bb1759781a4
---

# Fuzzing maintenance adoption

The 2026-10-12 assessment keeps proptest and cargo-fuzz. A Windows random-engine
probe showed a shared sampling oracle can run through proptest and Bolero, but
established no framework, shrinking or coverage advantage. About a dozen duplicated
assertion lines per harness did not justify adding a framework or helper yet.[^assessment]

Small compressed corpus archives can still have material replay costs. Existing
Linux CI logs imply approximately 17 seconds of initialization for python_analyzer
within a 55.496-second stage. This is a rounded estimate, not isolated replay
measurement. #803 remains a manual, bounded merge/feature-identity comparison;
no replacement cache or automatic cmin was adopted.[^assessment]

No difficult search boundary was demonstrated for #804's ci_selection target;
target/event instrumentation was declined there. #807 identified a narrower
compatibility gap: sampling properties do not pin the versioned output sequence.
Fixed known-answer regressions are the proposed improvement; they do not replace
properties, rejection-boundary tests or human specification review.[^assessment]

The independent Python fuzz-tool proposal #806 and closed solver/test-generation
studies remain separate. Read the assessment before treating these research
outcomes as implemented product guarantees or measured mutation-score gains.

[^assessment]: [Adoption assessment](../superpowers/reports/issue-805/README.md), source baseline, standalone probes and remaining Linux/corpus limits.
