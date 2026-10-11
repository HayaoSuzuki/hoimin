---
type: Contract
title: Rust dependency licenses and sources
description: Deny-by-default policies for both workspaces and build-script diff review.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
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
