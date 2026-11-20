# Research mutations implementation plan

> For agentic workers: use superpowers:subagent-driven-development. Execute sequential implementation tasks, with task review after each.

**Goal:** Implement all nine researched mutation issues with observable contracts and bounded Rust analysis.

**Architecture:** Private semantic modules emit source edits through the existing analyzer. Operator identity, selection, ranking and reporting remain shared contracts.

**Tech stack:** Rust 1.100, Ruff AST, rstest, proptest, CPython integration.

**Spec:** ../designs/2026-11-20-research-mutations.md and its three linked domain specifications.

## Global constraints

The design Global constraints apply verbatim to each task. Tasks run only in C:/Users/hayao/.codex/worktrees/research-mutations/hoimin, based on 4cf630f977dc266c6633969b57d0f9f251bb91d8. No edits to the original checkout. Build with -j 2 and test with --test-threads=2; use quiet test output and report command exit status. New tests use one invocation per case, AAA, named rstest and bounded proptest. Each implementer records five implementation and five test self-review passes with concrete findings and corrections.

## Review focus

1. Unicode flags and lexical source representation: Task 4 exercises complements, scoped overrides, implicit concatenation, escaped backslashes and bytes.
2. Observable argument/callee evaluation and async execution: Task 2 exercises ordered side effects, exceptions and immediate await.
3. Compound suite reachability and binding: Task 1 exercises nested patterns, wildcard guards, finally abrupt paths and ordering dependencies.
4. Import shadowing, dynamic escapes and signature mismatch: Tasks 2-4 add explicit rejection cases and keep old operators unchanged.
5. Resource and encoding boundaries: every task adds selected-site/cancellation/bounded generation tests; Task 5 exercises saved plans and representative encoded CLI roundtrips.

### Task 1: Structural control-flow mutations

Read control-design.md. Create rust/control_structure.rs (or focused finally/match/order modules if cohesive separation is necessary), rust/research_control_tests.rs and CLI behavioral contracts. Modify rust.rs traversal, core/config.rs IDs/defaults, CLI ranking and directly affected fixtures.

- [x] Write named failing tests for finally_unwrap, match arm deletion, guard alteration, OR deletion, statement_order_swap, including illegal reachability and binding rejection.
- [x] Run focused Rust tests and record RED before adding emitters.
- [x] Implement token-aligned structural edits, preserving suite bytes and multiline literals; visit module and nested suites without duplicate emission.
- [x] Add bounded property tests for parse-valid edits and executable normal/abrupt cleanup, pattern fallback and state/notification ordering.
- [x] Run focused tests and relevant existing analyzer/core suites; record five code and five test reviews, fix findings, commit task-only paths and report.

### Task 2: Calls, escaping, argument and result contracts

Read calls-design.md sections for #1192/#1195/#1196. Create rust/call_contracts.rs and tests; extend callable_imports.rs only as needed for guarded dotted imports, reuse optional_keywords.rs signature facts where sound. Modify IDs/defaults/ranking/integration.

- [x] Add failing tests for re/html/URL escape removal, positional/keyword/boundary swaps and typed call/await results.
- [x] Record focused RED.
- [x] Implement preserved evaluation order and scoped binding safeguards. No lambda scope relocation, duplicate evaluation or exception fallback.
- [x] Add finite runtime witnesses and bounded property cases for parse validity, alias/shadowing, spread rejection, defaults and source encodings.
- [x] Verify focused and impacted existing tests; record five code/test review passes, commit and report.

### Task 3: Collection processing and precision mutations

Read calls-design.md sections for #1198/#1199. Create rust/collection_processing.rs and rust/rounding.rs plus focused tests and runtime contracts. Reuse Task 2 callable identity helpers without changing established behavior. Modify IDs/defaults/ranking/integration.

- [x] Add failing cases for comprehension/filter selection, set union/intersection, sum/count, rounding functions/digits, Decimal modes/quantum/context and intermediate rounding.
- [x] Record focused RED.
- [x] Implement each semantic family with statically justified bindings and bounded emitters, preserving non-target argument effects.
- [x] Add meaningful bounded numeric/comprehension properties and executable ties, negatives, context and intermediate-rounding examples.
- [x] Verify focused and impacted tests; record five code/test reviews, commit and report.

### Task 4: Complete regex mutation domain

Read regex-design.md. Create rust/regex.rs with private scanner/source-literal modules if needed, plus regex_tests.rs and actual CPython contracts. Reuse guarded imports/provenance. Modify IDs/defaults/ranking/integration.

- [x] Add named failing cases for method, anchor and all ASCII/Unicode modes including compiled receivers and aliases.
- [x] Record focused RED.
- [x] Implement a bounded Python-regex scanner and source-edit mapping; handle global/external/scoped flags, complements, word boundaries, case folding, bytes and string syntax without pretending Rust regex syntax equals Python syntax.
- [x] Add bounded proptest compilation/runtime properties and the complete Unicode witness matrix from the domain design. Include specification-dependent ASCII original bug and Unicode intended behavior.
- [x] Verify focused and impacted tests; record five code/test reviews, commit and report.

### Task 5: Integration, documentation and final verification

Update user guides/operator documentation, generated CLI reference, default-selection fixtures and regression contracts. Run end-to-end plan/verify/report with representative new operators. Do not introduce new CLI surfaces.

- [x] Reconcile every issue acceptance item with implemented IDs and executable tests.
- [x] Verify default/exclusion selectors, stable serialization/ranking and candidate limits across mixed families.
- [x] Update reference and examples with precise static-analysis limitations and survivor interpretation.
- [x] Run formatting, clippy, workspace tests, contract features and frozen Python tests; investigate failures rather than weakening expectations.
- [ ] Perform final five implementation/test self-review passes, reassess all eight design diagnostics and obtain independent whole-branch review. Record remaining risks explicitly. Commit local work only.

## Plan self-review

1. Coverage: mapped all nine issues, separated regex from general calls so its character and flag rules cannot disappear into a generic helper. Added context precision and intermediate rounding explicitly.
2. Executability: each task has a RED/GREEN cycle and concrete behavioral matrix; internals may use existing emit signatures, no speculative framework or predetermined helper code.
3. Interfaces: all tasks share config/defaults/ranking and collector; execute sequentially. Reuse actual preceding-task signatures, not guessed parallel versions. No task consumes an undocumented new public API.
4. Verification gaps: added await, wildcard reachability, encoded sources, scoped regex flags, shadowing and source/lifetime effects to task owners.
5. Proportion and completion: five independently reviewable deliverables, setup integrated into owners, final task is cross-family integration rather than repeated per-family testing. Full suite repeated only after new changes justify it.


User clarification (2026-11-20): every new mutation operator in all nine issues is opt-out: enabled by default and individually disableable using existing options. No opt-in subset or experimental-default exception. Verify actual default planning and exclusion behavior, including the three earlier issues.
