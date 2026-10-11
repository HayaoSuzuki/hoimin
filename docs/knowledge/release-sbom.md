---
type: Contract
title: Release artifact SBOMs
description: Per-artifact Cargo dependency graphs, provenance and incomplete-release rejection.
status: draft
sources:
- id: ci
  resource: ../ci.md
  revision: a9f9d424cd7fc0c21ece1c89c233448e531a5b64
  working_tree: modified
  sha256: e39e1ca23612fc82c4e8a7f1aef276b28a91553a9df9339645400b4fd6983c01
- id: release
  resource: ../releases.md
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: cab75b11fc1b7ca03f24c37b845344b839cfdfd3f636b9e94776ea42079a58d9
- id: design
  resource: ../superpowers/specs/2026-10-10-issue-741-sbom-design.md
  working_tree: untracked
  sha256: 9025a3ca80f190d69b0df391bbb536df0304ad9948db1910420fab5bf7c4d6b9
- id: review
  resource: ../superpowers/reports/2026-10-10-issue-741-sbom.md
  working_tree: modified
  sha256: 83c27615f328d8d1157cb8d138ed4520227bfab96cd5687a7bea9aebcafb3e1c
  revision: 1d2c0912d029318fd40c09f3e6576735c0a740e3
---

# Mapping SBOMs to artifacts

A release publishes six documents for standalone binaries and wheels on three platforms. CycloneDX JSON 1.5 records Cargo's normal and build transitive dependencies, the distribution file's SHA-256, release commit, adjusted Cargo.lock SHA-256, target, and features. Linux wheel evidence is captured inside the manylinux container, separately from the standalone host environment.[^release]

# Publication conditions and provenance

Before generating SHA256SUMS, reject missing required SBOMs, schema violations, metadata mismatches, and inconsistent graph references. PRs changing distribution inputs and manual runs also validate the aggregate artifact.[^ci] Preserve existing protection of published releases and wheel-only PyPI uploads.[^design]

For the vendored parser, record its upstream package and VCS revision, Ruff patch origins, and release-commit-pinned path and README. A Cargo-derived graph is not a complete binary inventory including OS libraries, and does not claim that build dependencies are shipped in the binary.[^release]

# Evidence and recheck conditions

The [verification record](../superpowers/reports/2026-10-10-issue-741-sbom.md) distinguishes self-reviews, regression tests, Hypothesis, structural-mutation fuzzing, Lean model proofs, and 256 implementation comparisons. Lean proofs apply only to the model's publication gate. Local evidence does not include hosted Windows/Linux native builds or actual publication.[^review]

Recheck the correspondence between build settings and SBOM metadata when updating targets, features, the Maturin image, generator, schema, or parser.[^release]

[^release]: [Operations and retrieval](../releases.md).
[^design]: [Issue #741 design](../superpowers/specs/2026-10-10-issue-741-sbom-design.md).
[^review]: [Issue #741 verification record](../superpowers/reports/2026-10-10-issue-741-sbom.md).
[^ci]: [CI and preview selection](../ci.md).
