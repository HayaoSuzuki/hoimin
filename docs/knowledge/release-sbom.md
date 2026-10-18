---
type: Contract
title: Release artifact SBOMs
description: Per-artifact Cargo dependency graphs, provenance and incomplete-release rejection.
status: draft
sources:
- id: issues-992-998-sdist-config
  resource: ../../pyproject.toml
  revision: 60dab172af34dc929edc2fb87f27b7724b7d8542
  working_tree: modified
  sha256: dd6497bf51656c0a7cd813409a976cbe1be97a0e86f7f754a1aacce2fddf8f96
- id: issues-992-998-sdist-archive
  resource: ../../tests/test_sdist.py
  revision: 12e80440c1daec46c0e92895b123f34ef098f913
  working_tree: modified
  sha256: 7c0957d23274130c5b4083a9421cb6a0cdd5d0f84c2585562459f9658dfd116c
- id: issues-992-998-sbom-validator
  resource: ../../tools/sbom.py
  revision: 50b9c089ca73aeca2205636a2a9e1823cd244d39
  working_tree: modified
  sha256: 09ff16e05879eddfc4fe50c8d1dc9a6da478a0ddf292e81af457b56a0b053d81
- id: issues-992-998-sbom-capture
  resource: ../../tools/capture_sbom.py
  revision: 50b9c089ca73aeca2205636a2a9e1823cd244d39
  working_tree: modified
  sha256: 5f3eab1f3a3f9d0448fc27e5cbd7d6e591e1acb54dc75ff736d6b43979e5bf01
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

# Repository package versions and generator cleanup

The shipping graph contains exactly one `hoimin-core` and one `hoimin-python`
component, each matching the CLI root version. Missing, duplicate and mismatched
internal components fail validation. Capture removes generator sidecars for all
three shipping workspace members before generation and in final cleanup,
including when generation changes the lockfile and output is rejected. The
original lockfile is restored before that rejection.[^issues-992-998-sbom-validator][^issues-992-998-sbom-capture]

# Publication conditions and provenance

Before generating SHA256SUMS, reject missing required SBOMs, schema violations, metadata mismatches, and inconsistent graph references. PRs changing distribution inputs and manual runs also validate the aggregate artifact.[^ci] Preserve existing protection of published releases and wheel-only PyPI uploads.[^design]

For the vendored parser, record its upstream package and VCS revision, Ruff patch origins, and release-commit-pinned path and README. A Cargo-derived graph is not a complete binary inventory including OS libraries, and does not claim that build dependencies are shipped in the binary.[^release]

# Source distribution build inputs

Source distributions explicitly include the tracked vendored parser manifest,
Rust sources, provenance README and license required by the root Cargo patch.
Real-archive tests compare internal Python API and copied parser files with
their source bytes, and exclude untracked build/cache/scratch outputs. The
`hoimin-python/Cargo.toml` check compares text after CRLF-to-LF normalization,
matching Maturin's archive normalization of that workspace manifest. The copied
vendored manifest, Rust sources, README and license retain raw byte checks.
The include inventory must be updated when vendored Rust modules are added or moved.[^issues-992-998-sdist-config][^issues-992-998-sdist-archive]

# Evidence and recheck conditions

The [verification record](../superpowers/reports/2026-10-10-issue-741-sbom.md) distinguishes self-reviews, regression tests, Hypothesis, structural-mutation fuzzing, Lean model proofs, and 256 implementation comparisons. Lean proofs apply only to the model's publication gate. Local evidence does not include hosted Windows/Linux native builds or actual publication.[^review]

Recheck the correspondence between build settings and SBOM metadata when updating targets, features, the Maturin image, generator, schema, or parser.[^release]

[^release]: [Operations and retrieval](../releases.md).
[^design]: [Issue #741 design](../superpowers/specs/2026-10-10-issue-741-sbom-design.md).
[^review]: [Issue #741 verification record](../superpowers/reports/2026-10-10-issue-741-sbom.md).
[^ci]: [CI and preview selection](../ci.md).

[^issues-992-998-sbom-validator]: [tools/sbom.py](../../tools/sbom.py).
[^issues-992-998-sbom-capture]: [tools/capture_sbom.py](../../tools/capture_sbom.py).

[^issues-992-998-sdist-config]: [pyproject.toml](../../pyproject.toml).
[^issues-992-998-sdist-archive]: [tests/test_sdist.py](../../tests/test_sdist.py).
