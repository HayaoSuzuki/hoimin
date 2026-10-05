# Exception hierarchy implementation plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan inline.

**Goal:** Generate conservative user-defined exception mutations from project ASTs.

**Architecture:** Immutable project summaries resolve class identity and ancestry.
An AST visitor emits existing single-span candidates through bounded selection.
Automatic fingerprint inputs keep cross-file plans and resumed runs consistent.

**Tech Stack:** Rust, existing Ruff parser/AST, existing portable filesystem reader.

**Spec:** `docs/superpowers/specs/2026-10-05-issue-692-exception-hierarchy.md`

## Global constraints

- Remain on `investigate/user-defined-exception-mutations`; no dependency additions.
- Preserve default operators; `exception_hierarchy` is explicit-only.
- 4096 input files; 16 MiB/file; 64 MiB total; 256 resolution steps.
- No target-module execution; preserve selection, cancellation and bounded prefixes.
- Complete and record three reviews each of design, plan, implementation and tests.

## Review focus

- Imports outside mutation filters must resolve and participate in fingerprints (T3).
- Parameters, captures and later assignments must suppress aliases in their scope (T2).
- Cyclic imports and renamed bases must not invent class identities (T2).
- Constructors inherited through multiple user classes must remain restricted (T2).
- Adding a module can alter import precedence without changing a selected file (T3).

## Task 1: operator contract

Files: `crates/hoimin-core/src/config.rs`, config tests, CLI ranking and tests.
Interface: `MutationOperator::ExceptionHierarchy`, serialized as `exception_hierarchy`.

- [x] Add selector/default exclusion assertions; run them and observe failure.
- [x] Register the operator in all/from_name, ranking and CLI help via existing tables.
- [x] Run config and ranking tests; preserve `exception_ops` and `exception_risky`.
- [x] Commit the operator contract with its tests.

## Task 2: static hierarchy and candidate generation

Files: new `crates/hoimin-cli/src/analyzer/exception_hierarchy.rs` and adjacent tests;
`analyzer/rust.rs`, `analyzer/protocol.rs`, `analyzer/mod.rs`.

Interfaces: `ExceptionIndex` built from `(path, source)` summaries and ordered import
roots; `ExceptionIndex::replacements(path, reference, offset, raised, excluded)` returns
deterministic identity-distinct visible replacements. `excluded` contains the lexical
scope's possibly rebound roots. `analyze_source_with_exceptions` takes an optional borrowed index;
the existing source-only entry point remains a wrapper.

- [x] Add failing exact-pair tests for the AppError/MissingError/ConflictError example
  in handlers and raise; include a custom constructor handler-only case.
- [x] Implement class/import summaries, conservative bindings and bounded resolution.
- [x] Add failing tests then support cross-file absolute/relative aliases and module
  attributes; reject unresolved re-exports, cycles, shadowed or dynamic names.
- [x] Integrate replacement enumeration with the existing AST producer and diagnostics.
- [x] Add scope, span, parseability, constructor, group/termination and limit tests.
- [x] Run hierarchy and existing exception tests. Review implementation three times
  (identity/binding, mutation semantics, integration/resource behavior) and commit.

## Task 3: project lifecycle and fingerprints

Files: hierarchy project loader; `analyzer/mod.rs`, `plan.rs`, `shell.rs`,
`fingerprint_inputs.rs`, portable reader if bounded reading needs an entry point.

Interfaces: project options derive from `RunConfig`; `resolve_config` returns the
union of explicit and automatic fingerprint records. A cached Arc index is shared
by blocking analyses; construction is cancellation-aware and never caches failure.

- [x] Add failing plan tests using a selected service file and unselected errors
  module, relative imports, explicit excludes, and custom import roots.
- [x] Load bounded project summaries using the same discovery inputs as fingerprints.
  Add cancellation and a result limit to filesystem discovery, and bound reads before
  allocation; post-read length checks alone do not establish a memory bound.
- [x] Thread project options through plan/verify discovery and the run handler.
- [x] Add auto inputs to preparation and every input recheck, including workspace
  copies. Test changed, added and deleted dependencies and unchanged-file selection.
- [x] Test discovery/run candidate parity and cancellation; commit integration.

## Task 4: behavioral verification and documentation

Files: hierarchy tests, plan/integration tests, README and `docs/development.md`;
review record `docs/superpowers/reviews/2026-10-05-issue-692.md`.

- [x] Test actual Python handler semantics and constructor rejection using controlled
  fixtures, never import user projects during analysis.
- [x] Review tests three times: assertions against spec, adversarial input gaps,
  and full regression evidence. Add missing counterexamples before fixes.
- [x] Document invocation, supported cases, conservative skips, roots, limits and
  automatic fingerprints; record whether a Lean proof adds useful evidence.
- [x] Run `cargo fmt --all -- --check`, `cargo test --workspace`, and
  `cargo clippy --workspace --all-targets -- -D warnings`; inspect results.
- [x] Obtain a fresh whole-branch review, address material findings with regressions,
  then commit final tests/docs and report results and limitations.

## Plan self-review

1. Spec coverage: mapped every boundary to Tasks 1–4. Corrected replacement interface
   to receive lexical exclusions explicitly; offset alone cannot describe function locals.
2. Resource review: added discovery cancellation and bounded reads before allocation.
   A file-count check after an unbounded walk would not satisfy the design's limits.
3. Integration review: automatic inputs must be recomputed during verify and run
   rechecks, and checked against copied workspace records. Tests must cover both
   dependency content changes and additions, not only selected source changes.

## Completion

All tasks are complete. Tasks 2–4 are committed together because candidate
production and automatic dependency validation share the new project index.
See `docs/superpowers/reviews/2026-10-05-issue-692.md` for review counterexamples,
verification results, and the pre-existing default-feature Clippy limitation.
