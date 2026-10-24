---
type: Decision
title: Fuzzing maintenance adoption
description: Evidence gates for corpus distillation, property reuse and targeted exploration.
status: draft
sources:
- id: issue-851-protocol
  resource: ../maintenance/2026-10-13-analyzer-protocol.md
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
- id: python315-design
  resource: ../superpowers/specs/2026-10-24-python-315-fuzz-design.md
  revision: 8e908943831decdb5fc83ca0ffb143459778240b
  working_tree: clean
  sha256: 4c842b26dc099c8f2d925d693bf7eb25c02495484e5ce83f4049e65f05cf138e
- id: python315-generator
  resource: ../../tools/hypothesmith_corpus.py
  revision: 8e908943831decdb5fc83ca0ffb143459778240b
  working_tree: clean
  sha256: 7c7f282aab165eb34fc144de79273f52c4ad041441a95b127815262b651f9c28
- id: python315-campaign
  resource: ../../tools/ci_fuzz.py
  revision: 8e908943831decdb5fc83ca0ffb143459778240b
  working_tree: clean
  sha256: 0a96e44bd61f299f982ac2dd6ec153236f1901e799bd863ab8fb472227eb8c5c
- id: python315-guide
  resource: ../development.md
  revision: 8e908943831decdb5fc83ca0ffb143459778240b
  working_tree: clean
  sha256: 4ca769277121239b950ddc1d2bfcc174d05f67133b89f2c12bbbd3cbbab71355
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

# Current analyzer fuzz scope

Issue #851 retires the unused analyzer JSONL parser, its `analyzer_protocol` target and five seeds. The live `python_analyzer` target still compiles production modules, now including `types.rs`; candidate validation and report lifecycle targets remain. CI enumerates the seven remaining manifest targets. Historical #805 counts describe that earlier revision, not the current inventory.[^issue-851-protocol]

[^issue-851-protocol]: [Issue #851 maintenance record](../maintenance/2026-10-13-analyzer-protocol.md).


# Python 3.14 and 3.15 input generation

The campaign builds Rust fuzz targets once, then generates inputs in isolated
3.14 and 3.15 environments. Both lanes run grammar/libcst generation and filter
source with their own CPython compiler. The 3.15 lane adds structured lazy-import
and unpacking-comprehension generation; eleven fixed seeds cover the new forms.
Source is compiled, never executed, before entering the shared hash-named corpus.
The raw-byte Rust analyzer target still exercises malformed-input rejection.
These input properties do not establish mutant runtime semantics.[^python315-design][^python315-generator]

Each uv invocation selects its interpreter explicitly. Temporary Python
environments are removed after generation or failure and do not replace the
development venv. Stage names, command arguments, explicit environment selectors
and generator JSON identify the lane; inherited environment contents are not
logged. Python setup/generation stays inside the existing deadline. Seven Rust
targets run once with the original byte/time/RSS guards and failure handling.
The workflow's 3.14 driver is distinct from its two generation interpreters.
See the canonical [development procedure](../development.md#bounded-ci-fuzzing).[^python315-guide]
Windows generator checks do not establish a Linux sanitizer campaign result.[^python315-campaign]

[^python315-design]: [Python 3.14・3.15のfuzz入力生成](../superpowers/specs/2026-10-24-python-315-fuzz-design.md).
[^python315-generator]: [Bounded source generation](../../tools/hypothesmith_corpus.py).
[^python315-campaign]: [Campaign orchestration](../../tools/ci_fuzz.py).

[^python315-guide]: [Development fuzz procedures](../development.md#bounded-ci-fuzzing).
