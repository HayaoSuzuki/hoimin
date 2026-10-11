---
type: Contract
title: GitHub Release build provenance
description: Artifact/attestation identity, source identity, publication gates and
  hosted evidence.
status: draft
sources:
- id: workflow
  resource: ../../.github/workflows/release.yml
  revision: e9acbf4e5da6d749a789405522feab6b6ac8ecc0
  working_tree: clean
  sha256: eb7bf9a5256861b6ef699965be89c0b3d79f33acb7fdf3f62228acec622a1489
- id: validator
  resource: ../../tools/release.py
  revision: e9acbf4e5da6d749a789405522feab6b6ac8ecc0
  working_tree: clean
  sha256: 2a5ff1d47bdb4f0147db9458d0bccc559d0f7b582d7f78f9542cf1d9d62fec82
- id: guide
  resource: ../releases.md
  revision: fd416a6965a5732d75fa5519a5932e4a78a9e18a
  working_tree: modified
  sha256: 147e604b107011c8842c200280446eb1a71b18f4bd5407c6cda32972d5d9f61a
- id: review
  resource: ../reviews/2026-10-10-issue-750-release-provenance.md
  revision: c08574663a363228fed310fbf75157831b4a5669
  working_tree: modified
  sha256: 70e05665ae643e033fadb37a5499b175cf5e8ddce881192b13fc783e3a4f809f
- id: hosted
  resource: ../reviews/2026-10-10-issue-750-hosted-verification.json
  revision: c08574663a363228fed310fbf75157831b4a5669
  working_tree: untracked
  sha256: ae58498410a4d1d5ce6918244e4af5b78667d82daba568e5c3502ed4943a9c8f
---

# Attested files and publication gates

GitHub Release provenance covers 13 files: three archives, three wheels, six SBOMs, and one SHA256SUMS file.
Attestation and publication jobs download the same immutable `verified-release` artifact produced by validation in the same run.
Both jobs revalidate the complete inventory, artifact digests and commits in SBOMs, and checksum-list bytes without repairing or regenerating inputs.[^workflow][^validator]

The attestation job pins the official Action by full SHA and uses only contents read, id-token write, and attestations write permissions.
PRs and manual previews do not sign or register attestations.
Before allowing publication, verify every file with the generated bundle and check rejection of altered bytes, another repository, and a different source SHA.
Validation or evidence-storage failures stop publication; protection against modifying already published Releases remains in place.[^workflow]

For PyPI, the same build job also registers a single-subject attestation for each wheel. Preserve the 13-file GitHub Release inventory and do not create new build attestations at publication time. See [PyPI wheel publication](pypi-publishing.md) for TestPyPI-to-PyPI gates and configuration.[^workflow]

# Source and workflow identity

Require the prepared commit, checkout HEAD, and event source SHA to match.
Source SHA and workflow SHA have different meanings; compare them separately using `--source-digest` and `--signer-digest`.
Also constrain the workflow path and repository to the consumer's expected values.
Do not assume a checkout operation changes OIDC source identity.[^workflow][^guide]

Retain the existing build-time adjustment of manifests and lockfiles to the reserved version.
The attestation identifies the workflow and source including that adjustment; it does not guarantee an unchanged working tree, reproducible builds, absence of vulnerabilities, or OS code signing.[^guide]

# Verification evidence and rechecking

The [verification record](../reviews/2026-10-10-issue-750-release-provenance.md) records local regressions, shell control flow, strict lint, mutation checks, and at least five self-reviews at each stage.
Local tests are not treated as issuing GitHub OIDC certificates.[^review]

Post-merge run `38052986762` published v0.3.4 and checked certificate source/workflow identity, signatures for all 13 files, four rejection cases, byte identity between published files and verified-release, and SBOM digests.
Both source and workflow SHA for that measurement were `c08574663a363228fed310fbf75157831b4a5669`.
Certificate attributes and digests are stored in the [hosted evidence JSON](../reviews/2026-10-10-issue-750-hosted-verification.json).[^hosted]
Recheck when changing triggers, permissions, official Actions, GH CLI, artifact retrieval, or the distribution inventory.[^guide][^review]

[^workflow]: [Release workflow](../../.github/workflows/release.yml).
[^validator]: [Checksum and SBOM validation](../../tools/release.py).
[^guide]: [Consumer verification and reruns](../releases.md#github-build-provenance).
[^review]: [Issue #750 verification record](../reviews/2026-10-10-issue-750-release-provenance.md).
[^hosted]: [v0.3.4 certificate attributes, published digests and rejection results](../reviews/2026-10-10-issue-750-hosted-verification.json).

Release bootstrap installs locked internal-tooling before importing the typed version helpers; provenance-only setup includes the same group. See [typed results and contracts](typed-results-and-contracts.md).[^validator][^workflow]
