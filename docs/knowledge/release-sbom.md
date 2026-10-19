---
type: Contract
title: Release artifact SBOMs
description: Per-artifact Cargo dependency graphs, provenance and incomplete-release rejection.
status: draft
sources:
- id: issues-992-998-sdist-config
  resource: ../../pyproject.toml
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: 152f20bea30856e826524da74b812b2b8a9dce105b4431881cbda83de801cec6
- id: issues-992-998-sdist-archive
  resource: ../../tests/test_sdist.py
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: f814313d63bee7a6ada4559e19cd2e17cd23b927eaf87dab3903070aedb877c5
- id: issues-992-998-sbom-validator
  resource: ../../tools/sbom.py
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: 8ba2531e849d5250d0e271cf2ec0800e0b89cbcadb4c68567003df182bf58b05
- id: issues-992-998-sbom-capture
  resource: ../../tools/capture_sbom.py
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: c6bd0663352ac0ccf8f5b5f718293c7d5fc4fa0f8f62148095337a328c2abf4f
- id: ci
  resource: ../ci.md
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: modified
  sha256: ca97d4bd598e9d671714fd1c67e903effa93d2622cc8d3efc6502bd35dbed930
- id: release
  resource: ../releases.md
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: modified
  sha256: 68a8bd954295add52934ddf8e38a3429d9a90c351771f12f6b73081ccefc462a
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

The shipping graph contains exactly one `hoimin-budget-math`, one `hoimin-core` and one `hoimin-python`
component, each matching the CLI root version. Missing, duplicate and mismatched
internal components fail validation. Capture removes generator sidecars for all
four shipping workspace members before generation and in final cleanup,
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


The source archive also retains the production math crate's manifest and three
Rust source files, including cfg-gated proof annotations. The
[Creusot contract](creusot-budget-proofs.md) distinguishes verification-only
registry dependencies from the ordinary shipping graph.
