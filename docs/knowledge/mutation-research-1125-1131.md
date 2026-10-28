---
type: Decision
title: Behavioral mutation research 1125–1131
description: Seven default-on operators with guarded scopes and preserved research evidence.
status: draft
sources:
- id: four-implementation
  resource: ../superpowers/reports/issues-1125-1131/four-implementation.md
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: untracked
  sha256: 8f211d76ee496e1b020f2610eed1b5e0459ead3eefa41a554694d42a6871dd50
- id: four-review
  resource: ../superpowers/reports/issues-1125-1131/four-implementation-review.md
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: untracked
  sha256: c8e8609e2b9b5c4a20bfd7cc2d073a93c46cf123e06c11955a876c97478c34da
- id: dataclass-builder
  resource: ../../crates/hoimin-python/src/analyzer/rust/dataclass_factory.rs
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: untracked
  sha256: 0ac901178cfe8928f6ee4c14fe7bdd18fee54bc9285481f7e998817011dd9ec4
- id: call-builders
  resource: ../../crates/hoimin-python/src/analyzer/rust/followup_calls.rs
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: untracked
  sha256: e50da599c28c3c5ef10c98a842a6b75c7b27b89aabe2a1be5534bc6f9751d437
- id: capture-builder
  resource: ../../crates/hoimin-python/src/analyzer/rust/lambda_capture.rs
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: untracked
  sha256: 552ed0400c8b9f475243014231409435af102e780392552ef856fee44d17f1f4
- id: reconsideration
  resource: ../superpowers/reports/issues-1125-1131/reconsideration.md
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: modified
  sha256: 9621257e891cba513df3590835509dc622abdb7f24c5b3e17977ce3bbc890143
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
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: modified
  sha256: 78f9c5d2beb9d3ab2cb2416f138b8810e1a651d00a89b8d639abb078bed26a0f
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
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: modified
  sha256: 3850a59ed4546ae33722b5c1b17b40b585dd1267a9ed4f2c72f735d8a179df0b
- id: statement-producers
  resource: ../../crates/hoimin-python/src/analyzer/rust/effective_python.rs
  revision: 42130ce3e82912c3fa4f7574b0a347d64401545e
  working_tree: modified
  sha256: 38ed0a71d85413f991f5efcb17603f2aa8b9d7df10f8de797460651053d97664
- id: catalog
  resource: ../../crates/hoimin-core/src/config.rs
  revision: f3703cd875712b7a25bb37e5e2b19771eda8a52e
  working_tree: modified
  sha256: 4935baa6e7996ddea9d49e9f8d755a06fe1e19d9a3e1b990e9b3b2a0ffed9228
---

# Decisions and current contract

The implementation adopts `continue_remove`, `comparison_not_implemented`,
`exception_cause_remove`, `dataclass_factory_share`, `sort_key_swap`,
`lambda_capture_late` and `encoding_errors_ignore` as default operators with
individual opt-out. The catalog contains 101 IDs and 82 defaults; existing selector families, saved schemas and
runtime safety boundaries do not change.[^report][^catalog]

Continue removal allows the remainder of the same iteration. Comparison
replacement applies to unshadowed builtin NotImplemented returns in ordinary
synchronous rich-comparison methods. Cause suppression applies only to explicit
named causes, replacing them with None while retaining implicit context.
The shared admission, selection, payload and cancellation paths remain the
owners of resource control; no separate state store or new dependency is
added.[^analyzer][^statement-producers]

The maintainer subsequently authorized the remaining four implementations in
the same PR1134; all seven are now implementation/completion targets. Earlier
temporary deferral and open-status conclusions are superseded, without altering
old measurements. Factory sharing has an independently guarded ordinary-class
namespace; sorted swaps simple reads with proven builtin identity; callbacks
use private getters/sentinels to preserve explicit overrides; encoding uses
trusted codecs imports or a documented nominal method rule.[^four-implementation][^report]

Class-body captures are excluded while method-local loops remain eligible.
Multiline calls are selected at the actual edit-start line before replacement
allocation. Both requirements were found by independent review and pinned by
red/green analyzer tests and executable/public CLI regressions. No project
imports, new dependencies, state store or legacy readers are added.[^four-review][^dataclass-builder][^call-builders][^capture-builder]

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

[^four-implementation]: [Source: four-implementation](../superpowers/reports/issues-1125-1131/four-implementation.md).
[^four-review]: [Source: four-review](../superpowers/reports/issues-1125-1131/four-implementation-review.md).
[^dataclass-builder]: [Source: dataclass-builder](../../crates/hoimin-python/src/analyzer/rust/dataclass_factory.rs).
[^call-builders]: [Source: call-builders](../../crates/hoimin-python/src/analyzer/rust/followup_calls.rs).
[^capture-builder]: [Source: capture-builder](../../crates/hoimin-python/src/analyzer/rust/lambda_capture.rs).
