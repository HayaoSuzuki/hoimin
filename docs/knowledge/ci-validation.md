---
type: Contract
title: CI selection and required checks
description: Conservative diff classification, result aggregation and workflow validation.
status: draft
catalog_revision: 00893465e0328204453e5008dc3884035b85c7bd
sources:
- id: guide
  resource: ../ci.md
  revision: 00893465e0328204453e5008dc3884035b85c7bd
  working_tree: modified
  sha256: 68c2906e16a37495208b754a8a6798dcfc2c5b143aeb96a541704fac6206c4c5
- id: planner
  resource: ../../tools/ci_selection.py
  revision: 1e60e54813dad14f124635496ed773b299bb4b90
  working_tree: modified
  sha256: 44f27dd6683f6b362f3124fede5857675836ad93f13fff7ade1cdf793d205d81
- id: ci
  resource: ../../.github/workflows/ci.yml
  revision: 5033e1bb74c4b01c66dc86c3c9f70fc1b8bf11ee
  working_tree: clean
  sha256: 865f1adb4e81c7f848cb8eb06e0f0f3d0cd223d736e44b181784700b38a91968
- id: release
  resource: ../../.github/workflows/release.yml
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: 1432888965f0e344e82875b96a59b2ffb06c6185d1c63a5cd710d2d44bf7ee1f
---

# Selection and aggregation contracts

Unknown diffs select all checks.
Documentation changes still run change selection, quality/workflow checks, and aggregation.
Aggregation requires success for required jobs and rejects failure, cancellation, skipping, or missing results.
Only intentional skips of unselected conditional jobs are accepted.[^planner][^ci]

Changes to generated `docs/cli-reference.md` select Rust validation even when documentation is the only change.
The Rust job runs the generator with `--check` to reject differences between CLI definitions and generated documentation.
See [CLI reference generation and synchronization](cli-reference.md) for scope and regeneration.[^planner][^ci]

# Publication and checking operations

Only superseded PR validation and preview runs are cancelled.
Preview is selected for distribution-input changes and manual runs; post-merge tag reservation and publication use separate jobs with write permissions.
Provenance jobs with write permissions are also limited to post-merge execution, and publication requires successful attestation and verification.
See [release provenance](release-provenance.md) for scope and hosted validation.
Jobs with write permissions do not execute PR head code.[^release]

Attestation requires successful independent archive execution, Linux-wheel execution on glibc 2.17, and ABI recording.
See [artifact execution checks](artifact-smoke.md) for details and limitations.[^release]

After successful GitHub Release publication, an optional enablement variable starts a separate `workflow_dispatch` run for PyPI publication. Production publication requires successful TestPyPI retrieval and verification. Leave the variable unset before registration; see [PyPI wheel publication](pypi-publishing.md).[^release]

All workflows are checked with pinned actionlint, ShellCheck, and zizmor.
Do not apply severity cutoffs or blanket ShellCheck exclusions; document necessary exceptions inline with reasons.
The operational guide is canonical for local commands, exceptions, measurements, and tool limitations.[^guide]

# Unverified conditions and rechecking

The initial Linux cargo-nextest evaluation confirmed matching test lists and successful execution, but found no speedup with two workers, so cargo test was retained.
See the [evaluation results](../performance/nextest/2026-10-11-linux.md) for runtime capability skips versus ignored tests, measurement conditions, resource usage, and reevaluation scope.[^guide]

Hosted checks confirmed full-validation CI and three-platform preview success/timing, and cancellation of old CI on PR repush.
Those measurements cover PRs changing workflows and distribution inputs; documentation-only reduction rates and merge queue execution remain unverified.[^guide]
Required checks at the transferred repository were not configured at the recorded inspection; use `CI result` when enabling them.
The former organization's configuration is outside scope.
Recheck this contract and negative tests when changing path classification, job names, publication events, runners, or lint versions.[^guide]

[^guide]: [CI operational guide](../ci.md).
[^planner]: [Change selection and aggregation](../../tools/ci_selection.py).
[^ci]: [CI workflow](../../.github/workflows/ci.yml).
[^release]: [Release workflow](../../.github/workflows/release.yml).


# Production arithmetic proof gate

`CI result` requires success from the selected `creusot` job. It checks effective
pinned tools and configuration, regenerates current-source obligations and proves
all three release targets using no-cache strategy replay. Its 30-minute budget
includes image setup; tool-image caching cannot substitute for a new proof.
Missing or stale source contracts/goals and final unproved goals fail the job.
See the [proof contract and evidence](creusot-budget-proofs.md).[^ci]
