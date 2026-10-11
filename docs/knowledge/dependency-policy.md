---
type: Contract
title: Rust dependency licenses and sources
description: Deny-by-default policies for both workspaces and build-script diff review.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: policy
  resource: ../dependency-policy.md
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: b5cc6a71b5f60ff821dabede82729651f391f5b2fed823acef5da68b5099677f
- id: config
  resource: ../../deny.toml
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: f3558abe6b2875940b85f606023443c7bb0ee4f5e33e859342f9b6081841d789
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
