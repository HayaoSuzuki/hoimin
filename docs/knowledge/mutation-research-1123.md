---
type: Decision
title: Inheritance mutation research 1123
description: Restricted override deletion and super-call movement recommendations;
  default base-order mutation and no duplicate delegation-deletion operator.
status: draft
sources:
- id: base-order-builder
  resource: ../../crates/hoimin-python/src/analyzer/rust/base_order.rs
  revision: d80c1dbc1eeb97b43f503767cbb06bfd64ec5622
  working_tree: untracked
  sha256: fb339c331c5d50021cee252ba5ed343e02d58866b9f285d8258eb0d79b37d81f
- id: implementation
  resource: ../superpowers/reports/issue-1123/implementation.md
  revision: d80c1dbc1eeb97b43f503767cbb06bfd64ec5622
  working_tree: untracked
  sha256: c50be6851331aaa2952ec0020354cc65f3e9562963ae5657b3d13b4989249a07
- id: base-order315
  resource: ../superpowers/reports/issue-1123/results-base-order-python315.json
  revision: 0033b008582b24bead8bdf28f8980275e96500a3
  working_tree: untracked
  sha256: 803692ec5fb5c458934fc25bccc9fc2d741b4a6ba13127999536c3a185674cd5
- id: base-order314
  resource: ../superpowers/reports/issue-1123/results-base-order-python314.json
  revision: 0033b008582b24bead8bdf28f8980275e96500a3
  working_tree: untracked
  sha256: 3c314a7e9374926d6cd61d591c11d6ae14b35c097a03878f7bde2ce84d907717
- id: base-order
  resource: ../superpowers/reports/issue-1123/base-order.md
  revision: 0033b008582b24bead8bdf28f8980275e96500a3
  working_tree: untracked
  sha256: 0e91fa3072e301d37a373a2858652dbfdcadf98a3cdec1f40ebfa590ea8ace24
- id: report
  resource: ../superpowers/reports/issue-1123/README.md
  revision: d80c1dbc1eeb97b43f503767cbb06bfd64ec5622
  working_tree: modified
  sha256: ddef891b6477dee5a7a7635b6e069db0790bd80f66359e5f68b779c713a5402c
- id: review
  resource: ../superpowers/reports/issue-1123/self-review.md
  revision: 0033b008582b24bead8bdf28f8980275e96500a3
  working_tree: modified
  sha256: 309528345f61c64aa4c5a8bb4983c3d3db5fd5faa06216063cdd4e4415bb33ee
- id: experiment
  resource: ../superpowers/reports/issue-1123/experiment.py
  revision: 0033b008582b24bead8bdf28f8980275e96500a3
  working_tree: modified
  sha256: 66ec4381b6bdea968978c994ebd0f6a618e7453ab1a2ca1f70939d59d3e15e37
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
same-method `super()` call within a straight-line method body. Those operators
remain proposals. The product follow-up separately implements `base_order_shuffle`
as a default runtime operator with individual opt-out, bringing the catalog to
102 IDs / 83 defaults. It accepts distinct simple-name bases without runtime MRO
validation; inspect execution failures before interpreting a kill as a missing
assertion.[^report][^implementation][^base-order-builder]

Nine fixed examples compare all generated default candidates, four nearest
related operators (including opt-in IDs), and one prototype per example.
All pools fit the 64-candidate execution ceiling. CPython 3.14.9 and 3.15.0b1
agree on descriptors and verdicts: specialized provider dispatch and callback
order each expose an additional missing assertion in their fixture. Direct
list-ordering changes overlap an existing operator. Identical implementations
and commuting operations survive; constructor and initialization-dependency
changes produce runtime errors that are not credited as assertion gaps.
These observations do not estimate real-project prevalence.[^python314][^python315]

Six additional multiple-inheritance examples evaluate `base_order_shuffle`:
swap two bases, or seed a shuffle of three or more with a first-two swap if
the shuffle preserves the original order. Both new interpreter receipts record
three useful assertion gaps, two surviving controls and one MRO `TypeError`;
all existing/prototype candidates are executed. Correct cooperative `super()`
can still yield an order-sensitive kill, while explicit base calls can survive.
This probes required dispatch behavior, not the correctness of `super()` use.
The report identifies the available binary by hash and does not attest its
build revision. The multiple-inheritance prototype does not extend the original
single-inheritance adoption boundary.[^base-order][^base-order314][^base-order315]

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
[^base-order]: [Base-order research](../superpowers/reports/issue-1123/base-order.md).
[^base-order314]: [Base-order CPython 3.14 results](../superpowers/reports/issue-1123/results-base-order-python314.json).
[^base-order315]: [Base-order CPython 3.15 results](../superpowers/reports/issue-1123/results-base-order-python315.json).
[^implementation]: [Base-order implementation](../superpowers/reports/issue-1123/implementation.md).
[^base-order-builder]: [Source-preserving base-order builder](../../crates/hoimin-python/src/analyzer/rust/base_order.rs).
