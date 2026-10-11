---
type: Playbook
title: Updating the Rust pin and minimum version
description: Stable updates, nightly validation, update PRs and distribution checks.
status: draft
sources:
- id: development
  resource: ../development.md
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: 1f7b4ff1e18fa3f7d843f302b0f4b06fe0433f620c719acaa23cbb7f1b86e670
- id: pin
  resource: ../../rust-toolchain.toml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: b2c19d86958ed6810741488db5cd352856f303c53bcf0ea774e72f94ee4d201e
- id: manifest
  resource: ../../Cargo.toml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: 5d93c3c03282087fd84d1f0d5fe7d8ad766415b46aecd164b70e4df7420a0157
- id: ci
  resource: ../../.github/workflows/ci.yml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: bd0a671109b8b6ec2f7b0c30194c82e78e256c0be89312b8be1c030efe017510
- id: fuzz
  resource: ../../tools/ci_fuzz.py
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: 4c3cbbb0ff22ca733bb63d2e015b9ad8e44404e4ca04b5653122313fc86a7e71
- id: contracts
  resource: ../../tests/test_ci_workflow.py
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: d789699a2ce636f18705bfd33d03be887cde4a6a73d2d4dd2813be58ab2e60a9
- id: renovate
  resource: ../../renovate.json
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: clean
  sha256: 76c3a8f9641ecffcbd61ff8c9c7f768e9f05060a0cec73ebd4f58048fd8789d4
- id: rules
  resource: ../../infra/github/Pulumi.yaml
  revision: ddd80e12837632f49b7df2640b64535e33ef16db
  working_tree: modified
  sha256: c45f2337c5d61a266203a500654460fb59cba2c86f90bc1299993d94b9a7f458
---

# Stable pin and minimum supported version

Development, ordinary CI, and wheel builds use the concrete stable version in `rust-toolchain.toml`. Keep the minimum supported Rust version in `Cargo.toml` equal to that pin. Source builds require that version or later; compatible prebuilt wheels do not require Rust to be installed.[^development][^pin][^manifest]

Workflow contract tests check equality of the pin and minimum version. The dedicated older-compiler MSRV job was removed; quality checks and tests use the pinned version. A latest-version canary remains to identify fixes needed for the next update.[^contracts][^ci][^development]

# Update procedure

Check the official release list and update the pin and minimum version in the same PR. Renovate's rust-toolchain manager detects pin updates, but do not assume it also updates Cargo's minimum version; the maintainer must align them. Ordinary updates use the existing Renovate seven-day post-release waiting period.[^development][^renovate]

Run formatting, Clippy, all workspace tests, contract-enabled tests, and Python quality checks. Use the release workflow's PR preview for each OS's wheel build and smoke tests, and run non-Linux CI once against the final ref. The development guide is canonical for exact commands.[^development]

# Nightly and required checks

Fuzzing and randomized test order use a date-pinned nightly. Confirm nightly builds when updating stable. When changing the nightly date, align CI, scheduled fuzzing, execution scripts, cache keys, contract tests, and documented command examples; run fuzzing and randomized tests.[^development][^ci][^fuzz]

The historical `infra/github/Pulumi.yaml` required-check configuration targets `tokyogas-tech/hoimin`, not `HayaoSuzuki/hoimin`. It is outside current work following the transfer. When interpreting that configuration, check its repository and workflow scope and consult the operational procedure linked from the development guide.[^rules]

# Verification scope

This page records the update policy adopted in Issue #739. It does not guarantee execution success for a particular Rust version or OS. Record results and unverified conditions in each update PR. Retain historical MSRV designs as past decisions; use this page and the development guide for current procedures.

[^development]: [development.md](../development.md).
[^pin]: [rust-toolchain.toml](../../rust-toolchain.toml).
[^manifest]: [Cargo.toml](../../Cargo.toml).
[^ci]: [ci.yml](../../.github/workflows/ci.yml).
[^fuzz]: [ci_fuzz.py](../../tools/ci_fuzz.py).
[^contracts]: [test_ci_workflow.py](../../tests/test_ci_workflow.py).
[^renovate]: [renovate.json](../../renovate.json).
[^rules]: [Pulumi.yaml](../../infra/github/Pulumi.yaml).
