---
type: Decision
title: Behavioral mutation research 1125–1131
description: Three restricted default operators and four revised methods awaiting implementation decisions.
status: draft
sources:
- id: reconsideration
  resource: ../superpowers/reports/issues-1125-1131/reconsideration.md
  revision: 39c5a8050163ac383482909a80e9d57e57014b8a
  working_tree: untracked
  sha256: 72d55175e39e1dc3aeb30e39651df4117b2d7b7657803560414dc49d3a348876
- id: reconsideration-probe
  resource: ../superpowers/reports/issues-1125-1131/reconsideration_probe.py
  revision: 39c5a8050163ac383482909a80e9d57e57014b8a
  working_tree: untracked
  sha256: d4082209513ac26137a093cc9f71f710c8821206b9463b97edbfc2b523e6ac58
- id: reconsideration314
  resource: ../superpowers/reports/issues-1125-1131/reconsideration-python314.json
  revision: 39c5a8050163ac383482909a80e9d57e57014b8a
  working_tree: untracked
  sha256: b7b9c3fb6d11f52282796338fe44c57a6e62435249bac97357dffab5c92f19f7
- id: reconsideration315
  resource: ../superpowers/reports/issues-1125-1131/reconsideration-python315.json
  revision: 39c5a8050163ac383482909a80e9d57e57014b8a
  working_tree: untracked
  sha256: 373d64b8b38eaa42ed8020f6830859dc2dc8cceead2a89e073d71030534a9396
- id: report
  resource: ../superpowers/reports/issues-1125-1131/README.md
  revision: 39c5a8050163ac383482909a80e9d57e57014b8a
  working_tree: modified
  sha256: befc677b61bab6459aecd14d92f1fd757e54a55c1101a7c7f9250028ad80a856
- id: methodology
  resource: ../superpowers/reports/issues-1125-1131/methodology.md
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: untracked
  sha256: 7775372c77cf5b28835ae050135aebbb92ecdb9c7ae462fe49d669bb20233e39
- id: review
  resource: ../superpowers/reports/issues-1125-1131/self-review.md
  revision: 39c5a8050163ac383482909a80e9d57e57014b8a
  working_tree: modified
  sha256: 598a5a6a8d948546a6f0f411a3777ddaf7d730e49d23668d2cee02693e0c8b5a
- id: experiment
  resource: ../superpowers/reports/issues-1125-1131/experiment.py
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: untracked
  sha256: ecc3ca9a616fdfbef887ff8a3af6b085b354ad1fd1d01e8d46d45b1ccafbdb83
- id: python314
  resource: ../superpowers/reports/issues-1125-1131/results-python314.json
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: untracked
  sha256: e8a61c6938e0f05207cb97670c37de7a26d88c060a7079974bd7ed76e7c3faf0
- id: python315
  resource: ../superpowers/reports/issues-1125-1131/results-python315.json
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: untracked
  sha256: 38c48ae2baee3ec5c65825142b6d02e65756543b34abf60a880894bdc386daa7
- id: analyzer
  resource: ../../crates/hoimin-python/src/analyzer/rust.rs
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: modified
  sha256: e193ed318e77b6925fd67e76ba4d05f4f12be9936178bd319cdcbe20c1ef1ce4
- id: statement-producers
  resource: ../../crates/hoimin-python/src/analyzer/rust/effective_python.rs
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: modified
  sha256: 38ed0a71d85413f991f5efcb17603f2aa8b9d7df10f8de797460651053d97664
- id: catalog
  resource: ../../crates/hoimin-core/src/config.rs
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: modified
  sha256: 9ca42e15e417f8fce111894501c85eb03f09e439bdea514fff7ab4b5a25de17c
---

# Decisions and current contract

The evaluation adopts `continue_remove`, `comparison_not_implemented` and
`exception_cause_remove` as default operators with individual opt-out. The catalog
contains 97 IDs and 78 defaults; existing selector families, saved schemas and
runtime safety boundaries do not change.[^report][^catalog]

Continue removal allows the remainder of the same iteration. Comparison
replacement applies to unshadowed builtin NotImplemented returns in ordinary
synchronous rich-comparison methods. Cause suppression applies only to explicit
named causes, replacing them with None while retaining implicit context.
The shared admission, selection, payload and cancellation paths remain the
owners of resource control; no separate state store or new dependency is
added.[^analyzer][^statement-producers]

The maintainer asked to proceed first with issues 1125, 1129 and 1130. Only
those three are completed by PR 1134. Methods for 1126, 1127, 1128 and 1131 are
under reconsideration; the initial proposal to close all seven is superseded.
Implementation uncertainty and one overlapping example do not demonstrate low
usefulness.[^report][^reconsideration]

Proposed methods are guarded ordinary-class dataclass recognition, a two-read
sorted key swap, private-sentinel/getter callbacks retaining explicit overrides,
and trusted codecs calls or a documented nominal encoding-method rule. Nine
fixed probes confirm original success and intended mutant assertion failure;
explicit callback overrides still work. These probes do not implement Rust
candidate generation or new incumbent comparisons. The four issues remain
open.[^reconsideration][^reconsideration-probe][^reconsideration314][^reconsideration315]

# Evidence and limits

The fixed protocol compares direct existing, full-default and prototype pools
with a ceiling of two executed candidates each. Fourteen independently written
examples include useful requirement checks and explicit domain/interface controls.
Every original passes weak and strong checks. Both CPython 3.14.9 and 3.15.0b1
(beta) agree on the recorded verdicts; runtime errors and observationally
equivalent controls are not useful missing-assertion leads.[^methodology][^experiment][^python314][^python315]

The base binary observations precede product implementation, and replaying new
defaults changes the control pool. Product tests and final validation have their
own record. No real-project efficacy, universal equivalence, new formal proof,
Linux/Windows execution or peak-memory claim follows from this evidence.
The initial Lean corpus explicitly records these new producers as deferred;
existing proofs remain.[^review][^report]

[^report]: [Source: report](../superpowers/reports/issues-1125-1131/README.md).
[^methodology]: [Source: methodology](../superpowers/reports/issues-1125-1131/methodology.md).
[^review]: [Source: review](../superpowers/reports/issues-1125-1131/self-review.md).
[^experiment]: [Source: experiment](../superpowers/reports/issues-1125-1131/experiment.py).
[^python314]: [Source: python314](../superpowers/reports/issues-1125-1131/results-python314.json).
[^python315]: [Source: python315](../superpowers/reports/issues-1125-1131/results-python315.json).
[^analyzer]: [Source: analyzer](../../crates/hoimin-python/src/analyzer/rust.rs).
[^statement-producers]: [Source: statement-producers](../../crates/hoimin-python/src/analyzer/rust/effective_python.rs).
[^catalog]: [Source: catalog](../../crates/hoimin-core/src/config.rs).

[^reconsideration]: [Source: reconsideration](../superpowers/reports/issues-1125-1131/reconsideration.md).
[^reconsideration-probe]: [Source: reconsideration-probe](../superpowers/reports/issues-1125-1131/reconsideration_probe.py).
[^reconsideration314]: [Source: reconsideration314](../superpowers/reports/issues-1125-1131/reconsideration-python314.json).
[^reconsideration315]: [Source: reconsideration315](../superpowers/reports/issues-1125-1131/reconsideration-python315.json).
