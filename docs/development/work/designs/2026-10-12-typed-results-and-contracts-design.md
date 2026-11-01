# Typed Python failures and third-party contracts

Status: approved by the user on 2026-10-12; implementation has not started.

## Intent and scope

Make expected failures and internal invariants explicit in hoimin's Python
tooling and Rust crates. The user requested `returns` and `deal`, and prefers a
third-party Rust contract library when it improves on the existing implementation.
Preserve ordinary input validation, CLI diagnostics, exit codes, serialized
formats, release ordering, and resource-control behavior.

Work starts from main commit `2b29a8630ac8411a0545f17fabdf99f6fb848c00` on
`refactor/typed-results-and-contracts`. Documentation PR #800 remains separate.
Write new guides and review records in English. Perform at least five self-reviews
at each design, planning, implementation, and testing stage; record actual checks.

This spans internal interfaces, dependency installation, and Rust contract
infrastructure. Review this design before writing the implementation plan.

## Current implementation

- Python tooling lives under `tools/`, rather than a Python runtime package.
  `tools/release.py` is also imported by wheel/provenance tooling.
- `version_tuple` raises `ValueError` for invalid stable versions and returns an
  integer tuple. `validate_version` additionally accepts the existing dev suffix.
- The release workflow's reservation job invokes `python tools/release.py tag`
  before installing project dependencies. Several set-version steps also precede
  `uv sync`; installing dependencies later does not cover these imports.
- CI selection also runs before dependency setup. It must remain available to
  choose jobs even when optional tooling dependencies are unavailable.
- Rust already uses standard `Result`, `Option`, `thiserror`, and proptest.
  `hoimin-core::ContractInvariant` expresses domain consistency. The custom
  `contract_require!` and `contract_ensure!` macros provide feature-gated checks,
  stable IDs, and lazily evaluated diagnostic context. Disabled checks evaluate
  neither their condition nor their context.
- Existing checks also occur inside functions. Moving every assertion to a
  function boundary would change its timing and observed state.

These observations come from `tools/release.py`, `tools/pypi_release.py`,
`.github/workflows/release.yml`, `tools/ci_selection.py`,
`crates/hoimin-core/src/contracts.rs`, `crates/hoimin-core/src/budget.rs`,
and `crates/hoimin-core/tests/contracts.rs` at the baseline above.

## Python approach

Use `returns.result.Result` with concrete success and error types for expected
failures that callers compose or distinguish. Preserve public exception behavior
through explicit adapters at existing command/helper boundaries. Do not catch
arbitrary `Exception` and convert programming errors into normal failures.
Prefer direct `Success`/`Failure` construction and ordinary typed control flow
initially; advanced composition is subject to compatibility with the existing
strict `ty` gate. Do not replace the type checker merely to accommodate a library.

Apply `deal` postconditions and relationships between arguments and results to
small deterministic transformations. Contract checks detect implementation bugs;
explicit rejection of invalid external data remains ordinary validation. Avoid
`deal.pure` around subprocesses, filesystem/network operations, mutable registries,
or functions whose actual effects have not been reviewed. Do not globally change
deal's enable/disable state from an imported module.

Begin with release-version parsing/validation and its immediate callers, then
expand only where the same pattern improves failure handling. Keep the accepted
version grammar and existing messages. A successful stable parse yields exactly
three nonnegative integer components; invalid input yields a typed failure, and
existing adapters still raise their documented `ValueError`.

Before importing these dependencies into release tooling, inventory every
invocation, including imported use and direct Python subprocesses in tests.
Introduce a locked, named internal-tooling dependency group and explicitly
install/use its environment at all affected workflow entry points, including tag
reservation and set-version. Include the group in development environments.
CI selection remains dependency-free. Do not add Python runtime dependencies to
the distributed Rust CLI merely to support repository maintenance tools.

Registry metadata checked on 2026-10-12 reports returns 0.29.0 (Python >=3.11,
BSD-3-Clause) and deal 4.24.6 (Python >=3.8; upstream declares MIT). Python 3.14
execution, decorator typing, Hypothesis integration, exact installed license
texts, dependency audits, and compatibility are acceptance checks, not established
by those minimum-version declarations. Avoid the broad `deal[all]` extra.

## Rust comparison and adoption

| Option | Benefit | Cost or limitation | Decision |
| --- | --- | --- | --- |
| Existing macros | Stable IDs/context, exact interior check placement, no evaluation when disabled | No declarative pre/post relationship or entry-state snapshot support | Baseline for comparison, not a requirement to retain |
| `contracts` 0.6.11 | Declarative pre/postconditions, invariants, `old(...)`, custom messages | Procedural macro execution; MPL-2.0 needs a scoped policy decision; boundary checks do not directly replace interior assertions | Preferred runtime-contract candidate, conditional on measured compatibility |
| `creusot-contracts` | Specifications for deductive verification | Introduces a separate verification workflow and specification language | Consider for a later proof target; does not replace the runtime-check requirement |

crates.io metadata for contracts 0.6.11 reports Rust 1.77 and MPL-2.0; hoimin's
configured toolchain is Rust 1.99.0. Review the exact published proc-macro source,
dependency graph, build scripts, advisories, and license notices before adopting.
Do not globally add MPL-2.0 to `deny.toml`: any accepted allowance must be scoped
to the reviewed crate/version and accompanied by the required notice handling.
Check both the root and fuzz dependency graphs.

Use `BudgetLedger::reserve` and `release` as the initial migration target.
Declarative postconditions can cover all successful and error returns, reducing
duplicated exit-path checks. Snapshot only bounded scalar values where useful;
do not clone the ledger or collections to express an invariant.

Enable third-party contracts through the existing hoimin `contracts` feature
using an optional dependency and conditional attributes. Do not use the crate's
global disabling feature as hoimin's switch: Cargo feature unification must not
silently disable another consumer's assertions. Do not substitute `cfg(test)`
contracts, since integration tests exercise the library without that cfg.

Adopt third-party attributes where they improve coverage or clarity, then retire
redundant custom checking code as migration permits. Keep domain invariants as
ordinary reusable predicates. Map every existing interior assertion before
removing the generic macros; preserve checks that a boundary annotation cannot
faithfully express. Retained exceptions need a concrete rationale, not an assumed
preference for custom code. Preserve existing stable diagnostic IDs and useful
context, or explicitly document and review any internal diagnostic migration.

## Verification and release constraints

- Demonstrate that malformed versions still fail with the same externally
  observable exception/CLI behavior and valid versions preserve ordering.
- Use bounded Hypothesis properties for version round-trips and error handling;
  retain a small number of distinct grammar regressions. Keep AAA structure and
  branch-free test bodies. Avoid tests that only assert decorator presence.
- Verify contract violations with deliberately broken behavior and show valid
  success/error paths pass. Check Python 3.14, Ruff, and strict `ty` without broad
  suppressions. Add targeted mypy checks only if they provide additional needed
  assurance; do not assume its returns plugin is understood by ty.
- Exercise release bootstrap in a fresh environment with the locked tooling
  group, without reserving tags or publishing artifacts during local tests.
- Run Rust contract tests with feature enabled and disabled, including disabled
  condition/context evaluation, early error returns, and bounded `old` snapshots.
  Run strict Clippy, affected proptests, core dependency purity, and dependency
  license/advisory checks. Preserve resource and cleanup contracts.
- Inspect ordinary release dependencies/artifact contents and determine the
  release floor from the final distribution impact. No version change is decided
  merely from adding a development dependency.
- Use focused tests first. Monitor disk before Cargo work and clean owned Cargo
  outputs between substantial build phases, never during an active build.
- Update maintainer guidance, affected OKF concepts, attribution/build-script
  inventory when relevant, and the stage review record in the same branch.

## Design self-reviews

1. Reviewed the actual Python layout and avoided assuming a Python runtime
   package. Identified dependency-free CI/release bootstrap before proposing imports.
2. Distinguished expected failures, programming errors, input validation, and
   contract violations; preserved exception adapters and CLI behavior.
3. Compared declarative Rust checks with exact existing interior checks, including
   disabled evaluation, stable IDs/context, all returns, and feature unification.
4. Checked registry/toolchain/license evidence. Marked Python 3.14 execution,
   strict typing, proc-macro compatibility, and license handling as unverified.
5. Limited the first migration to a coherent Python path and budget boundaries;
   required useful properties and sensitivity checks, bounded state snapshots,
   explicit evidence, resource care, and final distribution-based version review.

## Upstream references

- [returns Result](https://returns.readthedocs.io/en/latest/pages/result.html)
- [returns distribution metadata](https://pypi.org/pypi/returns/json)
- [deal contracts](https://deal.readthedocs.io/basic/intro.html)
- [deal upstream metadata](https://github.com/life4/deal/blob/master/pyproject.toml)
- [contracts documentation](https://docs.rs/contracts/0.6.11/contracts/)
- [contracts distribution metadata](https://crates.io/api/v1/crates/contracts/0.6.11)
- [Creusot contracts](https://docs.rs/creusot-contracts/latest/creusot_contracts/)

This document records design and static inspection only. No product dependencies
have been installed for this task, no library has been adopted, and no new runtime
contract or formal proof has been verified yet.
