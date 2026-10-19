---
type: Contract
title: Dependency licenses, sources and update constraints
description: Rust license/source policies, build-script review and development update constraints.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: update-rules
  resource: ../../renovate.json
  revision: 0a545ea642275abc38ff501e6727dbedb691c3e7
  working_tree: modified
  sha256: 9c86694f5e095191faca959a0c73c67dfaec811306be6e5cdad79733d184288f
- id: python-pins
  resource: ../../pyproject.toml
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: 152f20bea30856e826524da74b812b2b8a9dce105b4431881cbda83de801cec6
- id: inventory-862
  resource: ../dependency-build-scripts.md
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: modified
  sha256: 293e9edad94b07cc782dcfeffea9286c2fbd7107f5631419a9bdaf63fa050aef
- id: policy
  resource: ../dependency-policy.md
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: modified
  sha256: 4188abee29e44b4c215e9104d757096c284ed3178da336913d3294a8bfbe7403
- id: config
  resource: ../../deny.toml
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: modified
  sha256: 1ade36661bac5b0062323ef9af713ab60792e13af26573560af9b7b703425900
- id: workflow
  resource: ../../.github/workflows/dependency-audit.yml
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: baf4dd85d13e34ebf2f217cfa00a3a67914a41b380da2c6d5f3e284185731d15
---

# Check scope

Check the main and fuzz workspaces with fixed lockfiles, all features, and no target filtering.
This includes normal, build, development, and other-OS dependencies; it does not mean every crate is included in every distribution.[^policy][^workflow]

# Allowances and exceptions

Reject unapproved licenses and unknown registry/git sources.
Limit ELv2 to `hoimin-budget-math`, `hoimin-core`, `hoimin-python` and
`hoimin-cli` versions `>=0.13.0, <0.14.0`,
plus the `hoimin-fuzz` 0.0.0 harness. NCSA remains limited to libfuzzer-sys 0.4.13.
Review local paths through repository diffs; passing automated checks does not establish safety or fulfillment of license obligations.[^policy][^config]

# Build-script changes

Regenerate the custom-build target inventory with dependency updates and review additions, removals, and version changes.
The initial approach uses manual diff review, without a custom analyzer or a build-script rejection gate.
It does not guarantee the safety of proc macros or script contents.[^policy]

Recheck the inventory and narrow exceptions when dependencies, features, toolchains, or hoimin's minor version change.
Maintain vulnerability audits and SBOMs as separate checks and evidence.[^policy][^workflow]

[^policy]: [Dependency policy](../dependency-policy.md).
[^config]: [Allowlist configuration](../../deny.toml).
[^workflow]: [Dependency audit workflow](../../.github/workflows/dependency-audit.yml).

The optional `contracts` 0.6.11 proc macro has an exact-version MPL-2.0 exception; the global allow-list remains unchanged. See [typed results and contracts](typed-results-and-contracts.md).[^policy][^config]

# Development update constraints

CI Python updates are limited to the supported 3.14 minor series. Review the
Renovate `actions/python-versions` rule alongside package support when changing
that range. Hypothesis has matching exact dev/fuzz pins so lock regeneration
cannot select a newer version than the dependency update under review.
These constraints do not disable Action updates or replace dependency audits.
[^update-rules][^python-pins]

The October 19 inventory records 213 main-workspace packages and 111 fuzz packages
from locked, all-feature metadata. Its changed build-script list is review
input, not a certification of dependency code.[^inventory-862]

[^update-rules]: [Renovate rules](../../renovate.json).
[^python-pins]: [Python package and development requirements](../../pyproject.toml).
[^inventory-862]: [Build script inventory](../dependency-build-scripts.md).


# Verification-only dependencies

`creusot-std` and `creusot-std-proc` have exact 0.14.0 LGPL-2.1-or-later
exceptions. Normal target builds exclude their cfg-gated macros and libraries;
unfiltered lock audits include them. Upgrades require review of upstream source,
macro output, generated proof artifacts and notices. The
[licensing record](../licensing.md#creusot-verification-dependencies) distinguishes
these facts from redistribution compliance. See the
[proof contract](creusot-budget-proofs.md).[^policy][^config]
