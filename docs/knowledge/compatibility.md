---
type: Contract
title: Compatibility and change notices
description: Public interfaces, version floors and migration notices.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: issue-899-changed-selection
  resource: ../superpowers/specs/2026-10-15-issue-899-design.md
  revision: 36dd548543b0eaf72f060b163d0af6ab5d227330
  working_tree: modified
  sha256: ce5f03920ef09f08647227e566596ccc47a0de862d969df5a70220c3ec3118e6
- id: issue-899-changed-review
  resource: ../superpowers/reports/2026-10-15-issue-899-review.md
  revision: 36dd548543b0eaf72f060b163d0af6ab5d227330
  working_tree: modified
  sha256: 6a931830d32f7fef3c479a4faa528d44d1cf667b1014ff3f27f45245568ed733
- id: policy
  resource: ../compatibility.md
  revision: 36dd548543b0eaf72f060b163d0af6ab5d227330
  working_tree: modified
  sha256: ca5b0b4265006a40385482b6665822ae376f76c7056d33119d0e11d344b790fc
- id: issue-887-environment
  resource: ../superpowers/plans/2026-10-14-issue-887.md
- id: release
  resource: ../releases.md
  revision: 056cb558953ddf5eb8b229976ee894ae7c6bc87f
  working_tree: modified
  sha256: 44b3ea0e0256ed1653dd2e71d053bd8b9726d8fe4fa9711254bb36a82cfb8d14
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

# Session load contract removal

The 0.7.0 floor removes the report reason `no_compatible_run` and the old optional
SessionLoaded fields. Custom event producers provide one explicit resumed reference
or fresh reason. Report schema 5 also stores the validated environment snapshot;
read historical fallback-reason reports with
the producing binary or rerun. This change alone requires neither plan regeneration
nor a new SQLite session. The environment snapshot format separately requires
regenerating older plans.[^policy][^issue-887-environment]

[^issue-887-environment]: [Environment snapshot contract and merge verification](../superpowers/plans/2026-10-14-issue-887.md).

# Recheck conditions

Review these rules when changing schemas, ranking, candidate identity, CLI
contracts, defaults, supported environments or package layout. The policy is a
review procedure, not proof of compatibility or retrospective migration.

[^policy]: [Compatibility policy](../compatibility.md).
[^release]: [Release guide](../releases.md).
[^template]: [PR template](../../.github/pull_request_template.md).


## Typed changed selection (Issue #899)

Plan schema 7 and report schema 6 store changed selection as null when disabled,
or as an object containing required bounded `context` and optional `diff_base`
when enabled. Boolean changed values and the former sibling context/base fields
are rejected. Regenerate older plans; preserve older reports with their producing
binary or rerun. No conversion updates existing files. The CLI's flags, explicit
selector intersections, Git base semantics, and effective target fingerprints
retain their meaning. [^issue-899-changed-selection][^issue-899-changed-review]

[^issue-899-changed-selection]: [Typed changed selection design](../superpowers/specs/2026-10-15-issue-899-design.md).

[^issue-899-changed-review]: [Issue 899 review and verification](../superpowers/reports/2026-10-15-issue-899-review.md). Current executed scopes, fixture corrections, limitations and release-floor checks.
