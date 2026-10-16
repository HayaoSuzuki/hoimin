---
type: Contract
title: Compatibility and change notices
description: Public interfaces, version floors and migration notices.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: issue-939-compatibility
  resource: ../compatibility.md
  revision: 465ff928c1f2ceb9f9b8f8084465475e45799ee8
  working_tree: modified
  sha256: 789f97d7475c4fbabdc81bb7aa735674429711e8bc00253872658f2d149527c3
- id: issue-899-changed-selection
  resource: ../superpowers/specs/2026-10-15-issue-899-design.md
  revision: 7b6cb19ba0181759f6b5726f985c5fd7700d313e
  working_tree: clean
  sha256: fa68b3e4aa56ab0ee3dd19f818a0b932a033dce550dbc869f956c261578b501f
- id: issue-899-changed-review
  resource: ../superpowers/reports/2026-10-15-issue-899-review.md
  revision: ad65b8ce55b0dabc59712775e362d5d614b27559
  working_tree: modified
  sha256: 50a53fa428a46289ede33f60ae1bcc342f8cfa3b2a03e37ebc5ad127ff5444bc
- id: policy
  resource: ../compatibility.md
  revision: 5237d1e53a2c8b34fca80f6eeb7d644a01d732e2
  working_tree: modified
  sha256: b1d7318075ad936964d4d1bba3c53be93ca3bdb8672fe662973ceda6d6fe0c9a
- id: issue-887-environment
  resource: ../superpowers/plans/2026-10-14-issue-887.md
- id: release
  resource: ../releases.md
  revision: 5237d1e53a2c8b34fca80f6eeb7d644a01d732e2
  working_tree: modified
  sha256: 9702fae5c3fe2d64dab60343a6a67dcda74e6e6c73e74dff10372596037194b7
- id: template
  resource: ../../.github/pull_request_template.md
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: 264d87e967cd21cb15361897010c805cbd9bc99d808989288052c0c13f0c3d95
- id: issue-949-compatibility
  resource: ../compatibility.md
  revision: 35c21145ccdfb6a044f9ae1dad2a5e4ff90deece
  working_tree: clean
  sha256: 203448fab2450fd7c419a354e1a2904a935e88cfd334f74b91d6f25b14220aea
---

# Current saved formats and release floor (Issue #949)

Plan schema 9 and report schema 7 store the project root only in
`normalized_config.selection.root`; the outer root field is rejected by typed
readers. Regenerate schema-8 and older plans. Read older reports with their
producing binary or rerun with the intended version. The minimum release version
is 0.11.0; fingerprint schema 11 and session schema 5 remain unchanged.
Progress retains its opaque configuration contract, while review validates typed
configuration.[^issue-949-compatibility]

This notice supersedes the current schema and version-floor statements in the
historical Issue #936 and #939 sections below. Their format and validation
changes remain in effect. Follow the [0.11.0 migration notice](../compatibility.md#single-normalized-project-root-in-0110)
for commands and retained contracts.[^issue-949-compatibility]

[^issue-949-compatibility]: [Single normalized root compatibility notice](../compatibility.md#single-normalized-project-root-in-0110).

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

## Required saved arrays (Issue #923)

The 0.9.0 minimum follows published v0.8.12. Typed saved configurations require
explicit `import_roots` and `fingerprint_files` arrays; omission and null are
rejected, while empty arrays remain valid. Verify rejects affected plans before
baseline execution, and review rejects reports missing run import roots.
Progress keeps its opaque configuration contract. That change required
regeneration only for plans missing the fields; it retained schema numbers and
fingerprint bytes. The schema-8 change below independently supersedes that plan
regeneration guidance. Read affected reports with their producing binary or
rerun.[^policy][^release]

## Single saved fingerprint input list (Issue #936)

At Issue #936, plan schema 8 stored fingerprint input records only under
`normalized_config.fingerprint_inputs`; the former top-level field is rejected.
Verify and review both reject schema-7 and older plans with regeneration guidance.
The floor is 0.10.0 because the 0.9 series has shipped. No conversion or automatic
deletion takes place. Follow the [canonical regeneration notice](../compatibility.md#single-saved-fingerprint-input-list-in-0100)
for commands and retained contracts; the [selection/plan contract](design/selection-plan-verify.md#single-saved-fingerprint-input-list-issue-936)
records implementation and test evidence.[^policy][^release]

## Duplicate report configuration fields (Issue #939)

Review rejects duplicate known configuration fields, including nested fields,
from raw current-schema report JSON. Current writers emit unique fields, so
valid output needs no regeneration. This invalid-input validation fix retains
report schema 6 and the 0.10.0 release floor. Progress keeps its opaque object/null
configuration contract in JSON and JSONL. Follow the
[compatibility notice](../compatibility.md#duplicate-report-configuration-fields)
for scope and the [session/report contract](design/session-report.md#duplicate-report-configuration-fields-issue-939)
for implementation evidence.[^issue-939-compatibility]

[^issue-939-compatibility]: [Duplicate report configuration notice](../compatibility.md#duplicate-report-configuration-fields).
