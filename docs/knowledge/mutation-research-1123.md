---
type: Decision
title: Inheritance mutation research 1123
description: Restricted override deletion and super-call movement recommendations;
  no duplicate delegation-deletion operator.
status: draft
sources:
- id: report
  resource: ../superpowers/reports/issue-1123/README.md
  revision: 312b72d3e06e44145bbac28e998c97a8ca4f587d
  working_tree: untracked
  sha256: 673e47887a0ed375cffd739d8a65660da84dc9e4fd34e87a693c6d345fff3794
- id: review
  resource: ../superpowers/reports/issue-1123/self-review.md
  revision: 312b72d3e06e44145bbac28e998c97a8ca4f587d
  working_tree: untracked
  sha256: 7963d79f02ac87e2c0d236e3d2d6195f34a2b32b726a355afcc4049c8b1c8d3a
- id: experiment
  resource: ../superpowers/reports/issue-1123/experiment.py
  revision: 312b72d3e06e44145bbac28e998c97a8ca4f587d
  working_tree: untracked
  sha256: 18b29c9f91480ce98af4347838756c7c84d73573dd55811d883c14bd915c9414
- id: python314
  resource: ../superpowers/reports/issue-1123/results-python314.json
  revision: 312b72d3e06e44145bbac28e998c97a8ca4f587d
  working_tree: untracked
  sha256: 30e2de9538518e7bb2b60099d4ca1ad851bfca68b2e693d8987eacd0a4e1678d
- id: python315
  resource: ../superpowers/reports/issue-1123/results-python315.json
  revision: 312b72d3e06e44145bbac28e998c97a8ca4f587d
  working_tree: untracked
  sha256: 3abae8e298bbece58e64b674247cb63b1d39bb39f339e8214d452c9e247b482c
- id: collector
  resource: ../../crates/hoimin-python/src/analyzer/rust.rs
  revision: 312b72d3e06e44145bbac28e998c97a8ca4f587d
  working_tree: clean
  sha256: 3850a59ed4546ae33722b5c1b17b40b585dd1267a9ed4f2c72f735d8a179df0b
- id: guard
  resource: ../../crates/hoimin-python/src/analyzer/rust/deletion.rs
  revision: 312b72d3e06e44145bbac28e998c97a8ca4f587d
  working_tree: clean
  sha256: a0694b2a6698b36f2e508e9ec60f17517c519b5e4af545b483e5ccc43affa836
---

# Decision and evidence

Issue 1123 recommends limited adoption of two future operators: remove an
ordinary overriding method to restore parent dispatch, and move a standalone
same-method `super()` call within a straight-line method body. No new product
operator is implemented by this research. The catalog remains 101 IDs / 82
defaults. Accepted implementations should follow the existing default-on,
individual opt-out policy.[^report]

Nine fixed examples compare all generated default candidates, four nearest
related operators (including opt-in IDs), and one prototype per example.
All pools fit the 64-candidate execution ceiling. CPython 3.14.9 and 3.15.0b1
agree on descriptors and verdicts: specialized provider dispatch and callback
order each expose an additional missing assertion in their fixture. Direct
list-ordering changes overlap an existing operator. Identical implementations
and commuting operations survive; constructor and initialization-dependency
changes produce runtime errors that are not credited as assertion gaps.
These observations do not estimate real-project prevalence.[^python314][^python315]

Standalone delegation deletion produces byte-identical mutants through default
`statement_delete` in both deletion fixtures, so no SCD ID is recommended.
Existing expression-removal guards remain authoritative; assigned/returned
calls are outside that overlap claim.[^collector][^guard]

# Implementation boundary and recheck triggers

The proposal starts with ordinary undecorated same-module single inheritance,
proven ordered class bindings and builtin `super` identity, matching simple
method signatures, no constructors/descriptors/async/generators or dynamic MRO.
A shared bounded index should serve both operators. The research prototype is
fixed-corpus and does not implement those guards. Source-preserving edits,
encoded/newline cases, selector ranges, cancellation/payload limits and public
plan/verify safety are prerequisites for implementation, not verified new
product properties.[^report][^experiment]

Recheck when those restrictions change, production implementation is proposed,
or actual usage shows different overlap/equivalence/error rates. Prototype
wall times do not establish product latency or memory costs. No runtime target
import for planning, new dependency, compatibility path or formal-proof claim
is proposed.[^review]

[^report]: [Source: report](../superpowers/reports/issue-1123/README.md).
[^review]: [Source: review](../superpowers/reports/issue-1123/self-review.md).
[^experiment]: [Source: experiment](../superpowers/reports/issue-1123/experiment.py).
[^python314]: [Source: python314](../superpowers/reports/issue-1123/results-python314.json).
[^python315]: [Source: python315](../superpowers/reports/issue-1123/results-python315.json).
[^collector]: [Source: collector](../../crates/hoimin-python/src/analyzer/rust.rs).
[^guard]: [Source: guard](../../crates/hoimin-python/src/analyzer/rust/deletion.rs).
