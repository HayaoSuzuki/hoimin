---
type: Decision
title: Behavioral mutation research 1125–1131
description: Three restricted default operators and four evidence-based deferrals.
status: draft
sources:
- id: report
  resource: ../superpowers/reports/issues-1125-1131/README.md
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: untracked
  sha256: 49e49d60906d5d2a86b2c6c768aa425b6426962f566de2ce863afe71fada596b
- id: methodology
  resource: ../superpowers/reports/issues-1125-1131/methodology.md
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: untracked
  sha256: 7775372c77cf5b28835ae050135aebbb92ecdb9c7ae462fe49d669bb20233e39
- id: review
  resource: ../superpowers/reports/issues-1125-1131/self-review.md
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: untracked
  sha256: 853f88c6972239bea82983c77a06718d3ce3ee58e1c41b05f59d905cf964015f
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

Dataclass factory sharing is deferred pending a class-namespace callable-identity
contract; the shared import resolver deliberately rejects those lookups. Sort
priority swapping provides no new useful requirement over existing element
deletion in this corpus. Lambda capture removal also breaks explicit-argument
callers and overlaps an existing default lead. Encoding-policy loss awaits a
recognized API boundary rather than matching arbitrary errors keywords.
These are bounded decisions, not universal claims about usefulness.[^report]

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
