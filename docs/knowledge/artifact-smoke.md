---
type: Contract
title: Archive and wheel execution checks
description: Execution of verified artifacts, Linux ABI evidence and publication gates.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: guide
  resource: ../artifact-smoke.md
  revision: e0009c4ff3392f6c47a98a06722151fc76302941
  working_tree: modified
  sha256: 644089593bbd34b938133a37ad672893d0b6285286096463db573ac306e6a518
- id: workflow
  resource: ../../.github/workflows/release.yml
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: modified
  sha256: 1432888965f0e344e82875b96a59b2ffb06c6185d1c63a5cd710d2d44bf7ee1f
- id: archive
  resource: ../../tests/archive_smoke.py
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: 448785c2eea428352cb841592c2e6b965ea2e660d505df227a622d382ebd4794
- id: wheel
  resource: ../../tests/wheel_smoke.py
  revision: e0009c4ff3392f6c47a98a06722151fc76302941
  working_tree: modified
  sha256: d7cea7105dc65b3b9d5938a1e2fd9f68d902b97b43038a9d56ac4e2218f4a6de
---

# Running distributed artifacts

A separate job downloads the checksum- and SBOM-validated `verified-release`, rechecks checksums, and executes the archives for the three existing platforms.
Only known regular files are accepted; binary contents are copied to a fixed temporary path.
Fixtures run outside the checkout, with no fallback to another binary.[^guide][^workflow][^archive]

Wheels and archives share a small mutation fixture that checks version/help,
a JSONL execution stream, and preservation of the original source. The shared
invocation uses the default format without `--format`. Results must start with
`run_started`, finish with a complete `run_finished` with exit code 0, and include
at least one conclusive killed/survived outcome. Aggregate JSON and truncated
streams are rejected. These framing checks do not replace the full report
schema/lifecycle validator.[^wheel][^guide]

Evidence is added to existing host-wheel validation; archive jobs do not duplicate
that wheel validation.[^wheel][^guide]

# Linux baseline and publication gates

Standalone binaries run on Ubuntu 22.04, with required GLIBC symbols recorded at or below 2.35.
Wheels are checked with CPython 3.14 and glibc 2.17 in a digest-pinned manylinux2014 environment; auditwheel ABI information and execution results are recorded separately.
These checks do not guarantee support for every Linux environment.[^guide][^workflow]

Evidence is stored as CI artifacts, separately from distribution files.
Archive and ABI jobs must succeed before attestation; publication that requires those attestations also stops on failure.[^workflow]

Recheck conditions and evidence when changing images, distribution layout, operating systems, fixtures, or publication dependencies.
Consult PR/review records for the scope of successful hosted execution; documentation alone does not establish execution success.[^guide]

[^guide]: [Artifact smoke operations](../artifact-smoke.md).
[^workflow]: [Release workflow](../../.github/workflows/release.yml).
[^archive]: [Archive helper](../../tests/archive_smoke.py).
[^wheel]: [Wheel checks and shared fixture](../../tests/wheel_smoke.py).
