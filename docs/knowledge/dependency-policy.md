---
type: Contract
title: Dependency licenses, sources and update constraints
description: Rust license/source policies, build-script review and development update constraints.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: update-rules
  resource: ../../renovate.json
  revision: c2ccb34946bd581cac41b1ca924b7491ab0c16ac
  working_tree: clean
  sha256: ebb3d6499a848ede376d0c5789fdba307fb33d8ffde2be65d1de4bf54982d2b0
- id: python-requirements
  resource: ../../pyproject.toml
  revision: c2ccb34946bd581cac41b1ca924b7491ab0c16ac
  working_tree: clean
  sha256: 31b7a78adbd0739a4ffe81019faa4bc3c7cc72bd8311bfef0c713a38eb9dcd83
- id: inventory
  resource: ../dependency-build-scripts.md
  revision: c2ccb34946bd581cac41b1ca924b7491ab0c16ac
  working_tree: clean
  sha256: 354b84133c997f5fca425c30a916c69f94fe5cd3ac22a2b19475d55fb28a83e7
- id: policy
  resource: ../dependency-policy.md
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: modified
  sha256: 7a83b9486d2cb15409898cf1f7fb45e1e7a3da2a6ca245ae5d4738db012deed0
- id: config
  resource: ../../deny.toml
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: modified
  sha256: 6a163126e8cf5ec5667d1d04ad86ea4ef7f7fd63524232f84cdeb123270ce9ec
- id: workflow
  resource: ../../.github/workflows/dependency-audit.yml
  revision: c2ccb34946bd581cac41b1ca924b7491ab0c16ac
  working_tree: clean
  sha256: 09436fc28383b7188d7c5e7282880962d365ec52984234f75a9f5466d1d481b6
- id: development
  resource: ../development.md
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: modified
  sha256: eb6e66876840d9a00154dfb75311bbb34620a2bf43d885e4a59b5717f91475d7
---

# Check scope

Check the main and fuzz workspaces with fixed lockfiles, all features, and no target filtering.
This includes normal, build, development, and other-OS dependencies; it does not mean every crate is included in every distribution.[^policy][^workflow]

# Allowances and exceptions

Reject unapproved licenses and unknown registry/git sources.
Limit ELv2 to `hoimin-budget-math`, `hoimin-core`, `hoimin-python` and
`hoimin-cli` versions `>=0.14.0, <0.15.0`,
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

The optional Rust `contracts` feature uses repository macros without an external
contract crate or MPL-2.0 exception. Dated adoption reviews describe the former
dependency. See [typed results and contracts](typed-results-and-contracts.md).[^policy][^config]

# Development update constraints

CI Python updates are limited to the supported 3.14 minor series. Review the
Renovate `actions/python-versions` rule alongside package support when changing
that range.[^update-rules][^python-requirements]

Python requirements such as `hypothesis>=6.170.1,<7` permit minor and patch
updates within the current major version. The 0.x ranges for `returns`,
`pypi-attestations`, `hypothesmith`, and `shellcheck-py` permit minor updates
below 1.0. Hypothesis uses matching ranges in the `dev` and `fuzz` groups;
`uv.lock` records the selected versions. See the [development guide](../development.md#dependency-vulnerability-audits)
for updating all groups.[^python-requirements][^development]

General Rust 0.x dependencies use explicit `<1` upper bounds to permit minor
updates. Ruff remains in the vendored parser's 0.6 series to preserve AST type
identity, and `creusot-std` remains pinned to the proof toolchain. The
[dependency update procedure](../dependency-policy.md#dependency-updates) covers
both Cargo workspaces and the source-review conditions for license exceptions.
[^policy]

Renovate's Cargo and Python managers use `update-lockfile` and enable lockfile
maintenance. Committed lockfiles determine the versions used by frozen Python
syncs and locked Cargo builds; range changes define the versions available to
future updates. Dependency audits remain separate checks.[^update-rules][^policy][^development]

The October 21 inventory records 223 main-workspace packages and 111 fuzz packages
from locked, all-feature metadata. Its changed build-script list is review
input, not a certification of dependency code.[^inventory]

[^update-rules]: [Renovate rules](../../renovate.json).
[^python-requirements]: [Python package and development requirements](../../pyproject.toml).
[^inventory]: [Build script inventory](../dependency-build-scripts.md).
[^development]: [Development guide](../development.md).


# Verification-only dependencies

`creusot-std` and `creusot-std-proc` have exact 0.14.0 LGPL-2.1-or-later
exceptions. Normal target builds exclude their cfg-gated macros and libraries;
unfiltered lock audits include them. Upgrades require review of upstream source,
macro output, generated proof artifacts and notices. The
[licensing record](../licensing.md#creusot-verification-dependencies) distinguishes
these facts from redistribution compliance. See the
[proof contract](creusot-budget-proofs.md).[^policy][^config]
