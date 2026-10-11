---
type: Decision
title: Fuzzing maintenance adoption
description: Evidence gates for corpus distillation, property reuse and targeted exploration.
status: draft
sources:
- id: assessment
  resource: ../superpowers/reports/issue-805/README.md
  revision: 36c0e84e7e93022b3588dede701f3b85212d98ff
  working_tree: modified
  sha256: 40ead0b7481a3566451f9fd1dcdac9df068035c1c0ed025db5532c5bb79d7d8e
- id: regressions
  resource: ../superpowers/reports/issue-807/README.md
  working_tree: untracked
  sha256: 4eeaf34be4234443b6d2ffaede55a9a06c64571074cc95585c387d84c0623771
- id: corpus-experiment
  resource: ../superpowers/reports/issue-803/README.md
  working_tree: untracked
  sha256: e18042e4114ed937a162649afef6dcf1f4a3d39d4dacb8793276797f980a98fe
---

# Fuzzing maintenance adoption

The 2026-10-12 assessment keeps proptest and cargo-fuzz. A Windows random-engine
probe showed a shared sampling oracle can run through proptest and Bolero, but
established no framework, shrinking or coverage advantage. About a dozen duplicated
assertion lines per harness did not justify adding a framework or helper yet.[^assessment]

Small compressed corpus archives can still have material replay costs. Existing
Linux CI logs imply approximately 17 seconds of initialization for python_analyzer
within a 55.496-second stage. This is a rounded estimate, not isolated replay
measurement. That observation motivated the bounded #803 experiment below; it was not a
measurement of compression savings.[^assessment]

No difficult search boundary was demonstrated for #804's ci_selection target;
target/event instrumentation was declined there. #807 initially suspected a versioned sampling compatibility gap, but independent
review found all proposed fixed outputs already checked by the Lean-derived
integration corpus. The redundant trial unit tests were removed; no additional
product/test change was justified.[^regressions] Existing properties and
rejection-boundary tests remain unchanged.[^assessment]

The independent Python fuzz-tool proposal #806 and closed solver/test-generation
studies remain separate. Read the assessment before treating these research
outcomes as implemented product guarantees or measured mutation-score gains.

[^assessment]: [Adoption assessment](../superpowers/reports/issue-805/README.md), source baseline, standalone probes and remaining Linux/corpus limits.

[^regressions]: [Rejected compatibility regression trial](../superpowers/reports/issue-807/README.md), historical trial evidence, existing corpus coverage and final no-change ruling.


# Completed corpus experiment

The #803 hosted follow-up completed in 5m27s on one pinned binary/cache. Median
replay fell from 19.113 to 9.087 seconds and initial merge cost 36.810 seconds.
However, exact set comparison found 47 original feature IDs and 3 original PC
indices absent, with other IDs appearing. Equal PC counts did not establish
preservation. Original replay observations also varied across unchanged runs;
this experiment does not attribute the difference solely to compaction.[^corpus-experiment]

Automatic/scheduled replacement was not adopted. The original cache and managed
seeds were unchanged; temporary workflow changes were removed. Close the bounded
investigation rather than create a new tracing/determinism project. Revisit only
with a concrete need, a defensible preservation method and total cost budget.
The original corpus/binary are not committed; retained hashes and ID sets support
comparison, not reconstruction of expired inputs.[^corpus-experiment]

[^corpus-experiment]: [Bounded corpus experiment](../superpowers/reports/issue-803/README.md), all six replay measurements, control-set differences and remaining uncertainty.
