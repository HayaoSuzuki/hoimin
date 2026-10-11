---
type: Contract
title: Typed results and executable contracts
description: Expected release failures, optional budget checks, and verification limits.
status: draft
sources:
- id: release
  resource: ../../tools/release.py
  revision: 948cc626c0c649f3defa70a6b7b0997d65262f66
  working_tree: modified
  sha256: 2a5ff1d47bdb4f0147db9458d0bccc559d0f7b582d7f78f9542cf1d9d62fec82
- id: budget
  resource: ../../crates/hoimin-core/src/budget.rs
  revision: 948cc626c0c649f3defa70a6b7b0997d65262f66
  working_tree: modified
  sha256: 9fdcfa6648dfd3503a23dbb483845bd5f42fab43ba4af8f4595f7cb34decf6b5
- id: policy
  resource: ../dependency-policy.md
  revision: 948cc626c0c649f3defa70a6b7b0997d65262f66
  working_tree: modified
  sha256: 00f4a2fdc1edf081158f4e35ed39ded9a357f36bf76a7e7d605c63e547735761
---

# Expected failures and implementation defects

Release version parsing returns typed Success/Failure values. Existing exception
adapters preserve caller behavior; deal postconditions reject inconsistent
successful results. Ordinary malformed input remains a ValueError, and neither
publication I/O nor CI selection is wrapped in contracts. The locked dev and
provenance groups include internal-tooling. Every fresh release bootstrap
installs that group before importing the helpers.[^release]

# Optional budget checks

The Rust contracts feature activates contracts 0.6.11 under the contract_checks
alias. Budget reserve/release check the domain invariant after explicit normal
returns, including errors. Reserve snapshots a scalar total and checks accounting;
no ledger collection is cloned. The macro does not intercept the question-mark
operator, so allocator errors retain an explicit return. Disabled builds evaluate
neither snapshots nor conditions. Unconditional validation and uniqueness checks
remain in place.[^budget]

# Adoption and evidence limits

The MPL allowance is exact-version scoped; both dependency graphs and build-script
inventories are reviewed. The default distribution does not compile the optional
proc macro. Existing interior custom checks retain their local timing and richer
context, as mapped in the review record.[^policy]

Runtime contracts and sampled properties are not formal proofs. This change adds
no new theorem or proof about Rust execution. See the [review record](
../reviews/2026-10-12-typed-results-and-contracts.md) for executed checks and the
[maintainer guide](../development.md#internal-python-results-and-rust-contracts)
for setup and activation.

[^release]: [Release helpers](../../tools/release.py).
[^budget]: [Budget implementation](../../crates/hoimin-core/src/budget.rs).
[^policy]: [Dependency policy](../dependency-policy.md).
