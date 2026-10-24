---
type: Contract
title: Archive and wheel execution checks
description: Execution of verified artifacts, Linux ABI evidence and publication gates.
status: draft
catalog_revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
sources:
- id: guide
  resource: ../artifact-smoke.md
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: f0faea153ba309bb369cca512a601d8eff3dc0ba7be6c90591d9fa439c63f20c
- id: workflow
  resource: ../../.github/workflows/release.yml
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: 6b83317d06d13441652b5780036254fea6991fd6e21c860bcb67d0b5a0293df6
- id: archive
  resource: ../../tests/archive_smoke.py
  revision: 79b6bde29b05e9904c8efff59e02d0c45377d7f7
  working_tree: untracked
  sha256: 448785c2eea428352cb841592c2e6b965ea2e660d505df227a622d382ebd4794
- id: wheel
  resource: ../../tests/wheel_smoke.py
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: 1307409b11a058d890f774f1e112967602d85cfb89fbf64a2121cbc99c0d7a7b

- id: python-package
  resource: ../../pyproject.toml
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: 22594ccc32d6f477fa399d637265b0bfca7eaf308ba7530d98d1513f8e56bdab

- id: python-runtime-design
  resource: ../superpowers/specs/2026-10-24-python-314-315-design.md
  revision: 16d4d5ca955dff6ab5470e70481798bc96eed194
  working_tree: clean
  sha256: e5a0fefe437b90d86045864bf82831bcdc9a210147eb07c3211d3fbbadeb3cba
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

# Supported Python runtimes

The wheel permits Python 3.14 and 3.15 (`>=3.14,<3.16`).
Support covers existing syntax and mutation operators; Python 3.15 lazy imports and unpacking in comprehensions remain outside this stage.[^python-package][^python-runtime-design]

Each platform release job checks the same wheel on both versions before packaging the release assets.
The smoke helper uses its running interpreter for uvx and its temporary venv, and retains separate runtime reports.
The glibc 2.17 floor check remains a separate CPython 3.14 check; it does not establish that floor on 3.15.[^guide][^workflow][^wheel]

[^python-package]: [Python package metadata](../../pyproject.toml).
[^python-runtime-design]: [Python 3.14・3.15の実行環境対応](../superpowers/specs/2026-10-24-python-314-315-design.md).
