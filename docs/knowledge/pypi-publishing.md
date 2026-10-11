---
type: Playbook
title: Publishing wheels to PyPI
description: Automatic publication via TestPyPI, Trusted Publishers, original build attestations and recovery.
status: draft
catalog_revision: 36aa4cd696f4db28cd4811a0995d85df89b56eb5
sources:
- id: guide
  resource: ../pypi-publishing.md
  revision: 34723ff3f9b19c28140a6efad0651e542e25b8c1
  working_tree: modified
  sha256: 3751752a365a7cabde2b7daa7b022ac647b98ed8d26de369701425be33b36576
- id: validator
  resource: ../../tools/pypi_release.py
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: clean
  sha256: 9b9000c2455426120e2be78994084014d26d2bc8eeaa840f08d6a6b01a05e895
- id: tests
  resource: ../../tests/test_pypi_release.py
  working_tree: modified
  sha256: 16e2e267eeeab4738108301763f0c3e197aaaa90b600f12d5f91dbdb31cd0507
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
- id: wheel-smoke
  resource: ../../tests/wheel_smoke.py
  revision: ff50918cb14ff39c67c0a594b665fd29d73080b4
  working_tree: modified
  sha256: 4132f96832912a24f6c7af14bff21fb53b3650f60d39c73aa4d1e9e56cf84369
- id: wheel-smoke-tests
  resource: ../../tests/test_wheel_smoke.py
  revision: ff50918cb14ff39c67c0a594b665fd29d73080b4
  working_tree: modified
  sha256: e8653a2473cc383c6c99911cc9f095beb3133fb6233f74c5e85ae71845f220ba
- id: release-workflow
  resource: ../../.github/workflows/release.yml
  revision: afab14d952113004ee999ea279358f16f2a0c6c6
  working_tree: modified
  sha256: 76b469c08efcb0563945ce9bb1c49e83ceb7610cc20774063544b90da33e1477
- id: ci-tests
  resource: ../../tests/test_ci_workflow.py
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: 30a5e9e68cc2b9c3f15809a75eb74525dfb4c045dd41b8803c666e6d52aabd2d
- id: provenance
  resource: ../../tools/pypi_provenance.py
  revision: afab14d952113004ee999ea279358f16f2a0c6c6
  working_tree: modified
  sha256: 7a79bea64db1a5c4d89c6236c3b8819a1d71ed956b8ea755c70c5ea2f748abe6
- id: provenance-tests
  resource: ../../tests/test_pypi_provenance.py
  revision: afab14d952113004ee999ea279358f16f2a0c6c6
  working_tree: modified
  sha256: 4b0610342f0bc824f3d2c7b6852750eb3cd0969ff18d04cda3f83d08c2ccb8f5
- id: review
  resource: ../reviews/2026-10-10-issue-771-pypi-provenance.md
  revision: 52e12a30bf40cb8b0040e21e776c8b75bc37ccc6
  working_tree: modified
  sha256: a35194bb515f91d4aeb1d0f1176b729cca04ab4ff55de1864820e5e7f127c499
- id: bootstrap
  resource: ../reviews/2026-10-11-issue-771-hosted-verification.md
  revision: 3dbf6dfdb107cb83c1834fa8d3bf057e95243326
  working_tree: modified
  sha256: e42ecdafe5fb25831f6652d8ba97d6517571b7ef0a0dbaf9c19f6a7b42b77263
- id: bootstrap-evidence
  resource: ../reviews/2026-10-11-issue-771-hosted-verification.json
  revision: 3dbf6dfdb107cb83c1834fa8d3bf057e95243326
  working_tree: untracked
  sha256: 3d2f452405473746b7dfe837c8ed00f21453e05fc3dc3476565b3d1a19b37883
---

# Publication scope and procedure

With `PYPI_AUTO_PUBLISH=true`, a merge to main creates a GitHub Release and automatically starts a publication-only `release.yml` run. The same wheels reach PyPI only after TestPyPI publication and retrieval/verification of all three wheels and both attestations succeed. An unset variable disables automatic publication. The [publication guide](../pypi-publishing.md) is canonical for registration values and commands.[^guide]

Preparation checks tag membership in main, published GitHub Release status, and all three wheel names, hashes, metadata, and license text. Only validated wheels reach publication jobs, which neither check out source nor build it. At the index-publication stage, only the TestPyPI and PyPI upload jobs receive `id-token: write`.[^release-workflow][^validator]

# Account configuration

Register a Trusted Publisher on both PyPI and TestPyPI with owner `HayaoSuzuki`, repository `hoimin`, and workflow `release.yml`. Use environment `pypi` or `testpypi` to match the destination. Both GitHub environments need main-only branch restrictions; adding the workflow alone does not configure them.[^guide]

Required environment reviewers make publication wait for approval. For fully automatic publication, retain main-only restrictions while allowing publication without reviewers. Enable the automatic-publication variable after registration and initial hosted verification.[^guide]

# Validation scope and recheck conditions

GitHub Release PR preview is selected for distribution-input changes and always for manual runs without a tag. Publication-only manual runs skip builds.
Preview preparation uses read permissions; only post-merge tag reservation and publication use write permissions.[^release-workflow]

GitHub Release builds use event `github.sha` for PR and manual validation, and `pull_request_target`'s `merge_commit_sha` for post-merge publication. PR-update payloads can contain stale `merge_commit_sha` values, so validation must not use that field to choose its target. Checkout, downstream commit inputs, and concurrency groups use the same selection conditions.[^release-workflow][^ci-tests]

Post-build wheel smoke tests check `Elastic-2.0` and the transferred repository's Repository, Issues, and Changelog URLs before isolated installation and CLI execution. Metadata regressions reject the former `MIT` license and former organization's URLs. Update these expectations when changing the license or public URLs.[^wheel-smoke][^wheel-smoke-tests]

License-text comparison tolerates only CRLF/LF differences from Windows checkout. Text or whitespace changes are rejected; wheel bytes and SHA-256 verification remain unchanged.[^validator][^tests]

Local tests validate rejection of missing/corrupt wheels, wrong versions or licenses, duplicate metadata, and invalid tags. Actual OIDC authentication, environment approval, and index uploads are outside local validation and require initial TestPyPI publication checks.[^tests][^guide]

After workflow or validator fixes, start a new manual run after merging to main. GitHub Re-run uses the original commit and does not incorporate code fixes.[^guide]

Reuse a validated publication without reuploading only when all three existing files and both attestations pass validation. Do not use unconditional `skip-existing`. For partial uploads, inspect registered files and hashes and publish a new version if needed. Recheck registration and validation when changing the workflow name, owner, environments, Python support, wheel platform names, or license text.[^guide][^validator]

[^guide]: [PyPI publication guide](../pypi-publishing.md).
[^validator]: [Artifact validation](../../tools/pypi_release.py).
[^tests]: [Publication preparation regressions](../../tests/test_pypi_release.py).
[^wheel-smoke]: [Wheel smoke tests](../../tests/wheel_smoke.py).
[^wheel-smoke-tests]: [Wheel validation regressions](../../tests/test_wheel_smoke.py).
[^release-workflow]: [GitHub Release build and publication workflow](../../.github/workflows/release.yml).
[^ci-tests]: [CI workflow regressions](../../tests/test_ci_workflow.py).

# Build attestations and publication events

Generate each wheel's SLSA attestation at build time and convert it to PEP 740 without changing the original signed statement.
PyPI validates both attestations against the uploading Trusted Publisher, so build and publication both use `release.yml`.
A reusable publication workflow would change OIDC `job_workflow_ref`; define upload jobs directly in the same file.[^provenance][^review]

The build's `pull_request_target` event cannot publish to PyPI.
A restricted post-merge job uses `actions: write` to dispatch main, and the publication-only run checks the original tag, source, workflow, and wheel digest.
Do not confuse the publication certificate's source revision with the build revision.[^release-workflow][^provenance]

Distinguish recorded real-signature rejection tests from control-flow tests with substituted external services.
On 2026-10-11 (JST), main with the format fix published v0.3.6 to TestPyPI and PyPI, then retrieved and verified all three wheels from each index.
SHA-256 values matched GitHub Release, and SLSA and Publish signature verification succeeded.
The build source was `c9f92eb`, the publication run used `3dbf6df`, and the original build statement was preserved byte for byte.[^bootstrap][^bootstrap-evidence]

After confirming rejection of five cases (altered bytes, wrong repository, wrong source, wrong workflow, and corrupt signature), `PYPI_AUTO_PUBLISH=true` was set and read back.
The initial failure, recovery, run IDs, and publication-certificate attributes are recorded in the hosted report and JSON.[^provenance-tests][^bootstrap][^bootstrap-evidence]

v0.3.7 also passed publication and retrieval verification on both indexes. PyPI's version API returned 404 immediately after publication, so only the failed verification job was rerun after propagation. This historical recovery used a manual rerun.[^bootstrap]

[^provenance]: [Attestation conversion and post-publication validation](../../tools/pypi_provenance.py).
[^provenance-tests]: [Attestation and publication-gate regressions](../../tests/test_pypi_provenance.py).
[^review]: [Issue #771 design and verification record](../reviews/2026-10-10-issue-771-pypi-provenance.md).
[^bootstrap]: [Issue #771 hosted bootstrap record](../reviews/2026-10-11-issue-771-hosted-verification.md).
[^bootstrap-evidence]: [Both indexes' verification JSON](../reviews/2026-10-11-issue-771-hosted-verification.json).

## Waiting for post-publication 404s

Only post-publication verification retries HTTP 404, using exponential delays of 10, 20, and 40 seconds, then at most 60 seconds, with at most 61 attempts and a 600-second monotonic budget.
Network and verification time count toward that budget; no new verification starts if the deadline has been reached after sleeping.
An in-flight verification can finish after the deadline, so the entire job, including dependency setup, also has a 15-minute limit.
Wait logs identify the index, version, and remaining budget. This does not relax inspect's absence decision, non-404 errors, byte checks, or signature rejection conditions.[^guide][^provenance][^provenance-tests][^release-workflow]
