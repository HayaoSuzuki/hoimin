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

## Initial implementation completion

Tasks 1–4 delivered the initial implementation. Later design reviews reopen
acceptance criteria, including diagnostics and useful precision; the historical
checked boxes above do not establish that those findings are resolved.
Tasks 2–4 are committed together because candidate
production and automatic dependency validation share the new project index.
See `docs/superpowers/reviews/2026-10-05-issue-692.md` for review counterexamples,
verification results, and the pre-existing default-feature Clippy limitation.


## Formal-audit follow-up

The committed [correspondence worksheet and follow-up plan](../reports/2026-10-05-exception-hierarchy-lean-audit.md)
extend this plan with a Lean model, kernel-checked invariants, shortest broken-model
witnesses, a generated corpus, public/internal Rust adapters, and CI freshness and
sensitivity checks. The report records three design, plan, implementation and test
review passes for this additional work. Existing production behavior is unchanged.


## Focused formal-correspondence review follow-up

Extend the audit at the 256-class ancestry boundary and at package attributes
replaced by child-module imports. Retain failing old-model/old-implementation cases,
correct fuel accounting, add bounded possible/eager import summaries, and reject
colliding exported bindings before class resolution. Check deferred self-imports
as a positive control, actual import-table limits, generated expectations and the
full workspace regression suite. Review details and evidence are recorded in the
[formal audit](../reports/2026-10-05-exception-hierarchy-lean-audit.md).


## Explicit attribute-write review follow-up

Retain import-origin correlations before name invalidation, propagate known writes
to providers and package descendants, and keep alias summaries bounded. Add direct,
nested/private, relative, deletion, constructor and unrelated-provider controls.
Extend the Lean model with provider trust after writes through canonical aliases,
prove that a written alias invalidates its provider, and check generated expectations
against public plans. The audit records the initial failure and three further
implementation/test review passes; the existing snapshot corpus remains unchanged.

## Design-review follow-up (open)

The [original-design review](../reports/2026-10-05-exception-hierarchy-design-review.md)
reopens the following work. Documentation of a limitation does not resolve it.
These tasks are not implemented by the review commit. Retain the existing explicit
operator selection, fingerprint scope, candidate schema and deterministic ordering.

1. **Specify effects and trust before extending aliases.** Define separate class
   definition, exported attribute and use-site binding identities. List supported
   import/assignment aliases, writes and opaque escapes, including scope and order.
   Preserve unknown effects and rejection reasons in the extracted facts. Review
   normal uses, adversarial uses and extraction correspondence in three passes.
2. **Retain the runtime counterexample, then implement the bounded subset.** Start
   with the report's `other = e; other.Root = object` public-plan regression and
   independent Python control. Cover alias chains, rebinding, cycles and scope
   collisions with explicit limits. Add a reason when known facts require rejecting
   a provider; do not imply that arbitrary function/object aliases are solved.
   Keep positive controls for unrelated providers and attributes. Each new rule
   needs a failing behavioral regression before the repair.
3. **Make the correspondence boundary observable.** Model supported extraction
   events and alias propagation in Lean; prove the resulting invalidation invariant.
   Compare Rust-extracted facts with model inputs, then public candidates with
   generated expectations. Retain independent Python controls and a broken-model
   sensitivity check. Follow the existing Lean resource guard and serial commands.
4. **Preserve reasons and measure useful coverage.** Distinguish no related class,
   invisible destination, constructor mismatch, unknown binding and mutated provider
   while keeping diagnostics bounded. Use a declared positive-fixture matrix and
   unrelated-change controls before narrowing provider-wide invalidation. Do not
   restrict fingerprint inputs to a dependency closure as a shortcut; additions
   can alter resolution. Record fixture results separately from any real-project
   evaluation, and make no recovery-rate claim without the latter.
5. **Set lifecycle and resource acceptance conditions.** Before enabling concurrent
   loads, choose single initialization or first-success publication, and test real
   concurrent schedules plus cancellation. Check source, AST, transient summaries
   and retained entries separately; measure memory/time at resource boundaries
   before claiming a process-level bound. Current sequential cache proofs do not
   discharge concurrent obligations.

The implementation order is 1 → 2/3 → 4; 5 is required before increasing concurrency
or making stronger resource guarantees. Each implementation and test stage retains
the user's minimum of three self-review passes, with the reviewed claims and
counterexamples recorded, not just a pass count.

Progress: the [assignment-alias audit](../reports/2026-10-05-exception-alias-lean-audit.md)
implements the bounded name-assignment slice of tasks 2/3, including local class
writes, Lean path proofs, and comparisons of Rust-extracted facts. Task 1's broader
identity/opaque-effect representation and tasks 4/5 remain open. Function/container
alias inference is not included. Do not read completion of this slice as completion
of all original-design findings.

Plan self-review for these open tasks:

- Pass 1, scope: preserve the useful static feature; do not turn this into executing
  target projects or implementing all Python semantics.
- Pass 2, dependencies: establish extraction facts before proving alias rules;
  otherwise the current correspondence gap would recur.
- Pass 3, acceptance: include positive controls, reason distinctions and lifecycle
  conditions. Keep unimplemented tasks open even though the initial plan passed.

## Acceptance after the post-repair design review

The [new review](../reports/2026-10-05-exception-hierarchy-design-recheck.md) tests
`2f41888` and makes tasks 1/4 concrete. Complete these boundaries before widening
alias inference again; adding more syntax cases does not resolve the abstraction.

- Introduce an internal analysis outcome separate from the replacement vector.
  Preserve a reason and location when a scope is disabled. Public plan tests must
  distinguish disabled analysis from no related class and intentional bare-raise
  exclusion, without an unbounded message stream or new required candidate fields.
- Represent write-analysis bindings with module/scope/name identity. Resolve
  parameters, globals, nonlocals, closure references, private names and method
  scope rules before connecting aliases. Keep actual cross-scope writes rejected.
- Retain the report's local-parameter renaming comparison: both spellings must
  preserve `Child → Root` when the parameter is unrelated to the exception class.
  Compare model keys, Rust-extracted facts and the complete public candidate set.
- Record the supported-use matrix before broadening eligibility: plain/custom
  constructors, handler/raise, visible names and tuple/re-export exclusions.
  Treat intended policy exclusions separately from failed analysis. Do not claim
  real-project coverage from this fixture matrix alone.

Plan review 1: reason preservation must precede formatting diagnostics; inferring
reasons from an empty result would repeat the current defect. Plan review 2: scope
IDs require lexical reference resolution, not only numbering functions. Plan review
3: positive renaming controls and global/nonlocal rejection controls must accompany
the proof/model changes, so correspondence alone cannot mask a poor abstraction.
This review changes documentation only; all items in this section remain open.
