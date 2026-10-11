# Typed Results and Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Introduce explicit Python failures and executable contracts, adopting a third-party Rust crate where it improves existing checks without weakening behavior.

**Architecture:** Migrate Python release-version internals behind existing exception adapters and bootstrap their locked dependencies at every affected entry point. Independently migrate Rust budget exit-path checks to optional declarative contracts, preserving other checks until their equivalence is established. These two implementation tasks each yield independently testable software and separate commits; shared documentation and final validation close the change.

**Tech Stack:** Python 3.14, returns 0.29.0, deal 4.24.6, Hypothesis, uv, Ruff, ty; Rust 1.99.0, contracts 0.6.11, proptest, cargo-deny/audit.

**Spec:** [Approved design](../specs/2026-10-12-typed-results-and-contracts-design.md).

## Global constraints

- Preserve ordinary input validation, CLI diagnostics, exit codes, serialized formats, release ordering, and resource-control behavior.
- Use work branch `refactor/typed-results-and-contracts`, based on main `2b29a8630ac8411a0545f17fabdf99f6fb848c00`; keep PR #800 separate.
- Write new guides and review records in English. Perform at least five self-reviews at each design, planning, implementation, and testing stage; record actual checks.
- Keep CI selection dependency-free. Do not add Python runtime dependencies to the distributed Rust CLI merely to support repository maintenance tools.
- Avoid `deal[all]`, blanket exception capture, global deal enable/disable changes, broad lint exclusions, and ledger/collection clones for contract snapshots.
- Enable Rust checks through the existing `contracts` feature, with optional dependency/conditional attributes; do not rely on `cfg(test)` or global `disable_contracts`.
- Any MPL-2.0 allowance is scoped to the reviewed crate/version, with appropriate notice handling; check both root and fuzz dependency graphs.
- Use focused tests first. Monitor disk before Cargo work and clean owned Cargo outputs between substantial build phases, never during an active build.
- Decide version-floor impact from final distribution behavior, not merely from adding a development dependency.

## Review focus

1. Fresh release jobs have no third-party modules before setup: version/tag helpers and imported provenance helpers must find the locked dependencies.
2. Malformed versions include Unicode digits, leading zeroes, trailing newlines, and very long digit runs: preserve the existing grammar and ordinary failures, not contract errors.
3. Contracts-disabled Rust builds must not evaluate conditions, context, or snapshots, including optimized builds and integration tests.
4. Early reservation/release failures must not mutate the ledger; enabled contracts must check all ordinary returns and preserve recognizable invariant IDs.
5. Existing interior checks and unrelated CLI error paths must remain intact; importing a dependency must not introduce runtime package requirements or silent permissive fallbacks.

## Task 1: Typed Python release-version handling and bootstrap

**Files:**
- Modify: `tools/release.py`, `pyproject.toml`, `uv.lock`, `.github/workflows/release.yml`.
- Test: `tests/test_release.py`, `tests/test_pypi_release.py`, `tests/test_pypi_provenance.py`, `tests/test_ci_workflow.py`.
- Create: `tests/test_release_version_contracts.py`.

**Interfaces:**
- Add `type VersionTuple = tuple[int, int, int]` in `tools/release.py`.
- Add `parse_stable_version(version: str) -> Result[VersionTuple, ValueError]`.
- Add `check_release_version(version: str) -> Result[None, ValueError]` for the existing stable-or-dev grammar.
- Preserve `version_tuple(version: str) -> tuple[int, ...]` and `validate_version(version: str) -> None` as explicit adapters which raise the contained `ValueError` on failure. Preserve `SEMVER` for existing importers.
- Produce dependency group `internal-tooling = ["returns==0.29.0", "deal==4.24.6"]`; include it in `dev` and `provenance`. No additions to `[project].dependencies`.

- [x] Review exact PyPI distributions' licenses and dependencies, then add the pinned minimal group and regenerate `uv.lock`. Run Python 3.14 imports/decorator smoke checks after `uv sync --frozen --group fuzz --group workflow --no-install-project`. Keep plugin/mypy extras out unless a demonstrated typing issue requires them.
- [x] Write failing behavioral tests after this environment setup, before implementing the new interfaces. A bounded Hypothesis triple of nonnegative integers (maximum `2**32-1`) builds `major.minor.patch`, calls the typed parser, and asserts `Success(parts)` and adapter equality. A second property appends a positive bounded dev counter and asserts `check_release_version(...) == Success(None)` while stable parsing fails. Use explicit cases for the distinct malformed grammar classes in Review Focus; each test body follows AAA without branching. Assert unchanged adapter error type/message for ordinary invalid strings. For digit runs beyond Python's conversion limit, preserve the existing conversion `ValueError`; do not claim every grammar-matching string converts successfully.
- [x] Run `uv run --frozen --no-sync pytest tests/test_release_version_contracts.py -q`; require failure for missing typed interfaces, rather than an environment/import failure.
- [x] Implement direct `Success`/`Failure` construction with concrete annotations. Use deal relationships/postconditions to require successful stable results to contain three nonnegative components matching the original canonical string; error results remain normal values. Catch only conversion `ValueError` where needed. Keep stable-or-dev validation independent of integer conversion so its existing acceptance does not narrow. Invalid external input must not raise a deal contract violation.
- [x] Implement the adapters with explicit typed success/failure handling, preserving messages and callers. Add a sensitivity test which intercepts the narrow deterministic conversion seam and returns an incorrect tuple for a valid input; assert that an enabled deal contract rejects it. Do not globally toggle deal state inside tests. Keep contracts scoped to pure transformations rather than decorating the release subprocess/file operations.
- [x] Inventory all release-module imports and workflow invocations with `rg`. Install `internal-tooling` before tag reservation and all set-version/bootstrap entry points, using pinned setup-uv and `uv sync --frozen --only-group internal-tooling --no-install-project`; invoke through `uv run --frozen --no-sync` in that environment. Keep existing build synchronization where required. Provenance-only sync gains the group through its include. Update workflow fixtures/contracts only where their execution environment changed; preserve status gates, write permissions, and checkout trust.
- [x] In a fresh temporary uv environment, install only the locked group and execute `tools/release.py --help` plus set-version against a temporary copied manifest/lock fixture. Assert output versions in all four files and no changes in the real repository. Exercise provenance imports in a fresh provenance-only environment. Do not push tags, upload packages, or access a real publication endpoint. Add a workflow regression ensuring bootstrap installation precedes the first affected invocation in each job.
- [x] Run focused release, wheel, provenance, and workflow tests; run Ruff and `ty check` with fuzz dependencies present. Diagnose decorator/generic typing failures without replacing the checker or using broad ignores. Run frozen uv audits for Linux, macOS, and Windows; update exact pins if audit evidence requires a supported patch. Record results and five implementation self-reviews covering failure typing, grammar, adapter behavior, dependency/bootstrap coverage, and strict typing. Commit a working Python migration including its docs/review record.

## Task 2: Third-party Rust contracts for budget boundaries

**Files:**
- Modify: `crates/hoimin-core/Cargo.toml`, `crates/hoimin-core/src/budget.rs`, `Cargo.lock`, `fuzz/Cargo.lock`, `deny.toml`.
- Modify if impacted: `docs/dependency-build-scripts.md`, `docs/licensing.md`.
- Test: `crates/hoimin-core/tests/contracts.rs`, `crates/hoimin-core/tests/budget.rs`, the unit tests in `crates/hoimin-core/src/budget.rs`.
- Preserve: `crates/hoimin-core/src/contracts.rs` and existing interior checks until migration equivalence is established.

**Interfaces:**
- Keep `BudgetLedger::reserve`, `release`, `ContractInvariant::invariant`, all public error types, and the CLI's `hoimin-core/contracts` feature propagation.
- Add optional dependency under unambiguous name `contract_checks = { package = "contracts", version = "=0.6.11", optional = true }`; core feature becomes `contracts = ["dep:contract_checks"]`.
- Conditional `contract_checks::ensures` attributes check `self.invariant()` after every normal return from `reserve` and `release`. Retain the ID `budget.total.invariant` in the assertion message. Where useful, compare a bounded scalar `old(self.reserved(kind))` with the post-state; no collection snapshots.

- [ ] Inspect the exact published crate source and its dependencies/build targets before compiling it. Check generated condition timing, early returns, borrowing, `old` expansion, panic behavior, and source/license files. Record the comparison with the custom macros and Creusot. Add only a version-scoped MPL-2.0 allowance with upstream evidence and rationale; retain the global license policy. Include notices where the actual distribution/build usage requires them. If an acceptance condition fails, document the concrete reason and compare another suitable crate before retaining custom code; do not silently drop the Rust deliverable.
- [ ] Write failing behavior/sensitivity tests for the migration. In unit tests with `contracts` enabled, corrupt an otherwise real ledger to violate its invariant, exercise an early `reserve` error and a `release` error, and expect a panic containing the stable ID. Add a test exposing the specific new pre/post relationship using a deliberately invalid state transition at a private seam if needed. Reuse existing valid/error-path budget properties; extend them only for uncovered relationships. Run the affected tests first to identify what existing macros already detect and what the new contract adds.
- [ ] Add the optional dependency and synchronize both lockfiles. Review normal, build, and all-features graphs; ensure ordinary default builds do not depend on the contract crate. Generate the build-script inventory for both workspaces and update it only if changed. Snapshot scalar counts only within conditional attributes so disabled builds do not evaluate them.
- [ ] Replace repeated `check_invariant()` calls in budget exits with conditional declarative postconditions. Remove the helper and its now-unnecessary unused-self suppression only after every normal exit is covered. Keep internal allocator uniqueness assertions and domain invariant predicates. Document any internal diagnostic format change while preserving the recognizable ID and identifying function/condition. Do not claim byte-for-byte preservation of custom panic formatting if the crate supplies a different format.
- [ ] Run default and feature-enabled budget/contract tests, including disabled-condition/context tests already present. Add an evaluation-counter check around the adopted attribute/`old` path for both modes. Run a release-profile check for the disabled mode. Reuse existing early-error/no-mutation tests and bounded proptests; no exhaustive new cross-product suite.
- [ ] Run `cargo clippy -p hoimin-core --all-targets --features contracts --locked -- -D warnings`, affected CLI feature tests, and ordinary core-dependency purity. Run cargo-deny license/source checks for both root/fuzz manifests and advisory checks. Check disk between build phases and perform `cargo clean -p hoimin-core` after the corresponding verification phase is complete; record actual cleanup and retained shared artifacts.
- [ ] Map each remaining custom call site to its timing/context requirement in the review record. Retain only checks with an identified reason to defer migration, or migrate additional straightforward sites with targeted tests. Record five Rust implementation reviews covering exit coverage, feature behavior, diagnostic migration, dependency/license policy, and bounded cost. Commit the independently passing Rust migration.

## Task 3: Documentation, full validation, and integration

**Files:**
- Modify: `docs/development.md`, `docs/dependency-policy.md`, affected release/provenance knowledge concepts and `docs/knowledge/references/design-documents.md`.
- Create: `docs/reviews/2026-10-12-typed-results-and-contracts.md`.
- Update other guides only when implementation changes their commands or guarantees.

**Interfaces:** Document Task 1's group/functions and Task 2's feature/dependency decisions; introduce no additional runtime interface.

- [ ] Update maintainer commands, bootstrap setup, contract activation/limits, failure adapters, library comparison, and remaining custom-check rationale. Refresh only reread OKF sources and their hashes. State that runtime contracts and sampled properties are not formal proofs. Existing Lean evidence retains its actual scope.
- [ ] Record five testing self-reviews: (1) meaningful red/green and sensitivity evidence, (2) malformed inputs and normal failures, (3) fresh environments/entry points across three OS workflows, (4) Rust enabled/disabled/early-return behavior and cost, (5) completeness and limitations of final checks. Distinguish executed local checks from hosted CI and skipped tests. Preserve the five design and five planning reviews.
- [ ] Run Python suite, repository Ruff format/check and ty, strict docs build, workflow lint, affected Rust workspace/contract tests and dependency audits appropriate to the final diff. Do not repeat expensive checks after they pass unless changes or failures justify it. Recheck locks and no untracked build artifacts. Record pre/post disk space and performed Cargo cleanup.
- [ ] Check ordinary wheel metadata/dependency graph and compatibility policy. Preserve the 0.4 release series unless the final change warrants a documented floor decision; do not create a release or publish to PyPI as part of this refactor.
- [ ] Obtain an independent whole-branch review following the applicable review skill, resolve material findings, and commit all implementation, documentation, and evidence. Push the work branch and create/attach a PR against current main. Inspect hosted docs, quality, dependency, Rust contract, and release preview checks; fix failures attributable to this change. Leave merging to the user.

## Planning self-reviews

1. **Spec coverage:** Mapped Python internals/bootstrap, Rust adoption, conditional evaluation, policy/attribution, stage reviews, documentation, and final release impact to tasks. Retained unsupported proofs and full repository-wide rewrites outside this first migration.
2. **Step specificity:** Fixed function names, error types, dependency group, versions, optional Cargo alias, target methods, workflow setup commands, and behavioral checks. Each implementation task has tests, verification, and a commit.
3. **Type and compatibility consistency:** Kept existing exception adapter signatures, stable-versus-dev grammar differences, `SEMVER` imports, public Rust types, and CLI feature forwarding. Covered provenance-only dependency setup, not merely default development environments.
4. **Failure coverage:** Assigned fresh bootstrap, Unicode/large version inputs, disabled snapshots, early errors, and retained interior assertions from Review Focus to explicit tests/review steps. Sensitivity checks target actual implementation behavior rather than decorator/source text alone.
5. **Cost and proportionality:** Chose a small Python path and one Rust state-bearing component, reused existing properties, prohibited collection cloning and broad extras, separated independently testable commits, and retained strict audits, focused checks, disk monitoring, and cleanup. Formal verification infrastructure is not added without a specific unmet proof need.

## Execution handoff

Proposed method: native execution in this session, followed by an independent
whole-branch review. This avoids fresh implementation contexts for a small number
of related tasks while retaining the user's five self-reviews at every stage.
Implementation starts after the user reviews this plan and confirms the method.
