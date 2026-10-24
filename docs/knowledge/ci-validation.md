---
type: Contract
title: CI selection and required checks
description: Conservative diff classification, result aggregation and workflow validation.
status: draft
catalog_revision: 00893465e0328204453e5008dc3884035b85c7bd
sources:
- id: guide
  resource: ../ci.md
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: 49f4c0fce8236f4472a6578269d51d3bab73e76fedc5637d6435f6b9f313ff9c
- id: planner
  resource: ../../tools/ci_selection.py
  revision: 1e60e54813dad14f124635496ed773b299bb4b90
  working_tree: modified
  sha256: 44f27dd6683f6b362f3124fede5857675836ad93f13fff7ade1cdf793d205d81
- id: ci
  resource: ../../.github/workflows/ci.yml
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: d18561a3fe6a334590002643c5dcf2ee5d968ae3314100adef826aeba1a1d17a
- id: release
  resource: ../../.github/workflows/release.yml
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: 6b83317d06d13441652b5780036254fea6991fd6e21c860bcb67d0b5a0293df6
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

All workflows are checked with actionlint, ShellCheck, and zizmor at the exact
versions recorded in `uv.lock`. Their manifest ranges permit minor and patch
updates; see [dependency update constraints](dependency-policy.md#development-update-constraints).
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

# Python runtime coverage

Linux Rust validation and manual Windows/macOS Rust validation run with Python 3.14 and 3.15.
Performance artifact names include the Python version so matrix jobs cannot overwrite one another.
Change selection, aggregation and quality tooling retain Python 3.14 as their common baseline.[^guide][^ci]

Development and release wheel jobs exercise the same wheel on both versions.
Both release smoke steps must succeed before the platform artifacts can reach validation and publication.
These workflow conditions describe required checks; successful local Windows execution does not establish successful hosted Linux/macOS execution.[^guide][^release]
