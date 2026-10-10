---
type: Contract
title: 互換性と変更通知
description: Public interfaces, version floors and migration notices.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: policy
  resource: ../compatibility.md
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: 2119cc8dca02becfeabf53279f82316a34e3d37b47ca3f7b31925657d91f2a16
- id: release
  resource: ../releases.md
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: 6e4d9d1a8d57a4af10e4533479e8a6fca43c371e45744adf24a6bde5fa6024ad
- id: template
  resource: ../../.github/pull_request_template.md
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: 264d87e967cd21cb15361897010c805cbd9bc99d808989288052c0c13f0c3d95
---

# Compatibility scope

The canonical [compatibility policy](../compatibility.md) covers CLI, reports,
saved plans/ranking/IDs, sessions, defaults, runtime support and package contracts.
Bug fixes can change candidate populations within a patch series; stable scores
are not promised. Breaking supported contracts and expanding defaults require a
minor-version floor increase. Metadata-only release checks retain the current
minor series. [^policy]

# Release decisions

The floor is a minimum, not an exact next tag. The existing allocator chooses
between it and the next stable patch. All five manifests/lockfiles must agree
when the floor changes. Published tags and artifacts remain immutable, and the
TestPyPI/PyPI gates remain in force. [^release]

The PR template records affected interfaces, the version decision and migration.
Generated release notes are not a compatibility validator. Plan/session readers
still enforce their actual schemas and fingerprints; policy text does not add
migration support. [^template][^policy]

# Recheck conditions

Review these rules when changing schemas, ranking, candidate identity, CLI
contracts, defaults, supported environments or package layout. The policy is a
review procedure, not proof of compatibility or retrospective migration.

[^policy]: [Compatibility policy](../compatibility.md).
[^release]: [Release guide](../releases.md).
[^template]: [PR template](../../.github/pull_request_template.md).
