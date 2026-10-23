---
type: Contract
title: Typed results and executable contracts
description: Expected release failures, optional budget checks, and verification limits.
status: draft
sources:
- id: release
  resource: ../../tools/release.py
  revision: e9acbf4e5da6d749a789405522feab6b6ac8ecc0
  working_tree: clean
  sha256: 2a5ff1d47bdb4f0147db9458d0bccc559d0f7b582d7f78f9542cf1d9d62fec82
- id: budget
  resource: ../../crates/hoimin-core/src/budget.rs
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: modified
  sha256: 395d904f50670b157a67c9511376e63fb2e34427cbf1eae07f7f47edceb19ad8
- id: policy
  resource: ../dependency-policy.md
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: modified
  sha256: 7a83b9486d2cb15409898cf1f7fb45e1e7a3da2a6ca245ae5d4738db012deed0
- id: macros
  resource: ../../crates/hoimin-core/src/contracts.rs
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: clean
  sha256: 20934de885ef95edce36e1273f28fee58393fc230908e080c41c2014733a139a
- id: manifest
  resource: ../../crates/hoimin-core/Cargo.toml
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: modified
  sha256: 77325329d4772cadf34b50bc2a94ae207a65f4ec5d9c94d124300b919db87141
- id: ci
  resource: ../../.github/workflows/ci.yml
  revision: 462f97b26749ea0caa3b83879c94bdd3cdce0f50
  working_tree: clean
  sha256: 865f1adb4e81c7f848cb8eb06e0f0f3d0cd223d736e44b181784700b38a91968
---

# Expected failures and implementation defects

Release version parsing returns typed Success/Failure values. Existing exception
adapters preserve caller behavior; deal postconditions reject inconsistent
successful results. Ordinary malformed input remains a ValueError, and neither
publication I/O nor CI selection is wrapped in contracts. The locked dev and
provenance groups include internal-tooling. Every fresh release bootstrap
installs that group before importing the helpers.[^release]

# Optional budget checks

The optional Rust `contracts` feature enables repository-owned standard-library
contract macros; its feature list is empty and it adds no external dependency.
Budget reserve/release check the invariant after successful and error results.
Reserve snapshots the old scalar total and checks that success adds the amount
and an error leaves the total unchanged. A closure contains fallible operations,
so its `?` returns reach the common checks. No ledger collection is cloned.
Disabled builds evaluate neither the snapshot nor contract conditions.
Unconditional validation and allocator uniqueness assertions remain active.
The repository's own CI continues to run contracts-feature tests.[^budget][^macros][^manifest][^ci]

# Adoption and evidence limits

The removed third-party macro's October 12 adoption review and preserved MPL
notice are historical evidence. Current contracts-feature builds use the same
repository macros as the other interior checks, with stable check IDs and debug
context. Python deal/returns use and existing Lean/Creusot proofs retain their
separate scopes.[^policy][^macros]

These current Rust claims come from source inspection, not a fresh runtime test.
Runtime contracts and sampled properties are not formal proofs. This change adds
no new theorem or proof about Rust execution. See the historical [October 12 review record](
../reviews/2026-10-12-typed-results-and-contracts.md) for checks at its recorded
revision and the [maintainer guide](../development.md#internal-python-results-and-rust-contracts)
for setup and activation.

[^release]: [Release helpers](../../tools/release.py).
[^budget]: [Budget implementation](../../crates/hoimin-core/src/budget.rs).
[^policy]: [Dependency policy](../dependency-policy.md).

[^macros]: [Repository contract macros](../../crates/hoimin-core/src/contracts.rs).
[^manifest]: [Core feature definition](../../crates/hoimin-core/Cargo.toml).
[^ci]: [Rust CI](../../.github/workflows/ci.yml).
