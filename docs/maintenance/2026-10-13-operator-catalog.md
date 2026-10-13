# Operator catalog ownership (Issue #856)

Base: `6c638da243865273c55886739e9bbf77803f8fcd` on main.
Branch: `refactor/operator-catalog-856`; worktree: `.worktrees/issue-856`.
Origin fetch and push: `https://github.com/HayaoSuzuki/hoimin.git`.
This is separate from #857: operator selection and result status encoding have
independent owners, tests and compatibility conditions. No shared framework is
needed. The merged #859 means the current catalog has 73 operators, not the 72
counted when #856 was filed.

## Design: five self-reviews

1. Followed `MutationOperator` through Serde, `as_str`, `from_name`, `all` and
   CLI `valid_names`; current correspondence is correct but independently listed.
2. Compared a local declaration macro, a handwritten table with a separate enum,
   and a dependency/code generator. The local macro removes all three inventory
   copies without a dependency; explicit variants still support exhaustive matches.
3. Kept historical `all_legacy`, current defaults and family memberships as
   independent selection policies. The catalog does not decide defaults.
4. Preserved declaration order, `Ord`/BTreeSet output, every wire name and the
   saved-input-only `type_mapping` alias. Canonical lookup stays strict.
5. Applied software-design-philosophy to the catalog boundary: before 5/8 rows
   (6.25/10), with information hiding, ownership of a design decision and interface
   intent failing. One local declaration and explicit lookup/alias comments address
   these rows; a separate name/member slice owns family expansion and advertising.
   This assessment is scoped to the catalog, not all of config.rs.

## Plan: five self-reviews

1. Freeze the existing 73 variant/name pairs independently in tests before editing
   production code; assert both wire encoding and actual lookup/selection behavior.
2. Exercise advertised membership, declaration-order serialization, the saved alias,
   strict invalid-name rejection and all eight families (including existing tests).
3. Check the characterization tests on unchanged production, then temporarily
   remove an inventory entry to confirm detection before the refactor.
4. Generate enum/Serde/canonical names/lookup/inventory from a local macro; use a
   plain shared family slice for the two callers. Preserve public method signatures.
5. Run focused and full workspace/contract checks, strict lint, docs and independent
   review before pushing only the dedicated branch and creating a PR against main.

## Compatibility and validation scope

Consulted `docs/compatibility.md`, the OKF overview and update workflow,
`docs/knowledge/design/selection-plan-verify.md`, and `docs/development.md`.
The version floor remains 0.5.0: CLI behavior, operator IDs, default selection,
ordering, fingerprints, schemas and distribution contracts are unchanged. No
migration is needed. This internal ownership cleanup does not change existing OKF
contract guarantees; no new concept or historical-source rewrite is required.

The finite name table and family mapping need exhaustive fixed expectations,
not randomized coverage. Existing property tests and Lean-backed selection tests
remain in the workspace suite. No new proptest or Lean model is warranted for
this representation-only change; passing Rust tests do not prove OS behavior.

Builds use two workers/test threads, no incremental/debug artifacts, and a
worktree-owned target. A sampled guard checks 8 GiB process-group RSS, 12 GiB
target size, at least 20 GiB disk free and a 40-minute command deadline. These are
sampled observations, not hard OS limits. The two issue builds run sequentially.


## Implementation: five self-reviews

1. Compared all 73 declaration entries with the pre-change enum and canonical
   names, including the latest stdlib conversion operator. Their order is intact.
2. The local macro derives Serde, exhaustive `as_str`, strict `from_name` and
   the private inventory from those entries. The historical alias is metadata
   on TypeMapping only, so CLI lookup cannot accidentally accept it.
3. Reviewed each of the eight family member sequences against the former match.
   Both listing and expansion read `SELECTOR_FAMILIES`; neither controls defaults.
4. Checked the unchanged `all_legacy` and Default implementations, selection
   serialization, public signatures, error construction and sort order.
5. Final scoped philosophy score: 8/8 (10/10), no failing diagnostic rows.
   The declaration documents ordering/identity and the family slice documents
   selection policy; no reusable framework, new dependency or build step is added.

## Initial verification

Before production edits, the new six fixed-contract tests plus the existing
operator selection and plan configuration targets passed 43 tests. A temporary
omission of StdlibConversionCallRemove from the old inventory caused two of the
six contract tests to fail (advertised names and canonical lookup), while four
passed. That mutation was restored before implementing the refactor. The same
43 focused tests passed afterward. This is characterization and a deliberate
mutation check for a behavior-preserving refactor, not a claim that main had a
missing-operator defect.


## Verification: five self-reviews

1. The fixed expectations detected a deliberately omitted registration before the
   refactor and passed both before and after the ownership change (43 focused tests).
2. `cargo test --locked --workspace --all-features` passed 2,825 tests with 29
   ignored in 151 top-level summaries on macOS, including core/CLI contracts,
   CLI configuration, default selection, saved plan and fingerprint checks.
   Nested subprocess summaries are excluded from this count.
3. Read the resource guard result: exit 0, 369.1 seconds, peak process-group RSS
   1,727,496,192 bytes and no guard trigger. This is a local run, not hosted CI.
4. Independent read-only review found no actionable issues. It compared all 73
   names/order and eight member sequences, and confirmed `all_legacy`/Default
   are byte-for-byte unchanged. It did not execute tests.
5. Rechecked the final public boundary: only config ownership and contract tests
   change; no analyzer replacement, candidate ID, persisted schema, version floor
   or policy is modified. Source diff/format and the remaining quality checks
   must pass before committing and publishing the dedicated branch.

Strict all-target/all-feature workspace Clippy and generated CLI reference
`--check` passed. Formatting and whitespace checks passed. The independent
review's scoped philosophy assessment also passed all eight diagnostic rows.
