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
  revision: 0a545ea642275abc38ff501e6727dbedb691c3e7
  working_tree: modified
  sha256: b5b30d8e87b7c8c85f1e8b01defbc8e69cd6788c369301c4cbd8d00d4826532f
- id: inventory-862
  resource: ../dependency-build-scripts.md
  revision: 0a545ea642275abc38ff501e6727dbedb691c3e7
  working_tree: modified
  sha256: 730c6d8e081922bc008daf2c52845a69ab7fe8d6447c0ee4ad4495ab9dc2c694
- id: policy
  resource: ../dependency-policy.md
  revision: 34723ff3f9b19c28140a6efad0651e542e25b8c1
  working_tree: modified
  sha256: 121c638e21b3cc6deb7995925457c9063a147888618f7013c3641274cc61df75
- id: config
  resource: ../../deny.toml
  revision: e9acbf4e5da6d749a789405522feab6b6ac8ecc0
  working_tree: clean
  sha256: 25b6404c2b676490b995f872907d7aeecab889b26cc4a6dcf202fbf6cca74e69
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
Limit ELv2 to hoimin's own version range and NCSA to libfuzzer-sys 0.4.13.
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

The October 14 inventory records 218 main-workspace packages and 93 fuzz packages
from locked, all-feature metadata. Its changed build-script list is review
input, not a certification of dependency code.[^inventory-862]

[^update-rules]: [Renovate rules](../../renovate.json).
[^python-pins]: [Python package and development requirements](../../pyproject.toml).
[^inventory-862]: [Build script inventory](../dependency-build-scripts.md).
