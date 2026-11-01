# Issue 986: single if/elif visitation

The approved Issue and task brief freeze the intended behavior. This record
tracks task-specific design, tests and self-reviews; shared analyzer contracts
and OKF evidence are maintained by the integration coordinator.

## Requirements and design reviews

1. Reproduction: the selected `boolean_literal` family has no candidates, yet
   `elif not` panics while building facts. Preserve the assertion and fix traversal.
2. Ownership: the pinned Ruff ordinary visitor visits each elif condition twice;
   a local statement walker can own that defect without a dependency/vendor edit.
3. Dispatch: re-exporting `Visitor` preserves its upstream default. Explicitly
   route statement-capable visitors, including `NamedBindingInvalidator` and
   `FactorySafetyScan`; keep expression-only visitors proportional.
4. Semantics: preserve initial test/body, clause ordering, scope overrides and
   callback behavior. Check delegated suites and the separate source-order path.
5. Independence: use source-token positions and hand-declared name sequences,
   public plan success, candidate descriptors/order and name-resolution evidence.
   Release success is required; no claim of an existing release candidate loss.

## Plan reviews

1. Scope inventory found `stdlib_conversions.rs`, omitted from the original file
   list. Include it and inventory all ordinary Visitor implementations.
2. Integration tests compile `rust.rs` directly. Their analyzer shims must expose
   the same boundary to avoid a production-only traversal fix.
3. RED will exercise node visits independently of candidate deduplication and the
   exact public plan failure. Test scaffolding initially re-exports upstream.
4. GREEN will override only statement If, retaining upstream walking elsewhere;
   do not clone an AST framework or introduce a second visitor trait.
5. Verification uses one owned Cargo process, task tests followed by related
   analyzer suites, release regressions, formatting and serialized commit hooks.
   Full branch suites and independent review belong to the coordinator.

## Design assessment

Initial design: 7.5/10. The interface-comment and newcomer-boundary diagnostics
fail because upstream default dispatch remains implicit. Document the explicit
`visit_stmt` requirement at the shared boundary.
The resulting boundary hides the pinned dependency defect and requires no new
generic framework or configuration.

## Implementation reviews

1. Borrow/ownership review: the shared walker borrows Ruff nodes; it creates no
   AST copies, indices, deduplication state or allocations. Existing scope and
   cancellation callbacks retain ownership of their state.
2. Edge review: zero elif clauses, no `not`, repeated `not`, nested else/while,
   and final else all have independent visitation expectations. Only the extra
   upstream elif pre-visit is omitted.
3. Dispatch review: upstream If calls the clause walker directly, rather than
   `visit_elif_else_clause`; the local implementation preserves that hook policy.
   All other statement kinds still use upstream dispatch.
4. Consumer review: imports now use the shared boundary, including expression-only
   consumers and oracle tests. The two reachable upstream-default statement
   visitors now explicitly dispatch locally. `ImplicitExceptionEntry` deliberately
   skips bodies; other default visitors only enter through expressions.
5. Test/complexity review: direct-include integration shims expose the same module.
   Candidate expectations are built from literal source locations and public ID
   identity rules, independently of analyzer walking. Source offsets are 18/48/71;
   candidate columns are zero-based 12/13/11.
   An initial test incorrectly treated them as one-based; that mismatch was
   corrected to the existing public contract after an informative test failure. The design is
   10/10 on the eight skill diagnostics: one documented defect boundary, existing
   callback interface, local implementation change and no new framework.

## Verification reviews

1. Execution review: upstream-only traversal failed both ordered-position tests;
   the public plan reproduced exit 2 with `duplicate unary-not operator start`.
   After repair, analyzer tests passed (378 passed, 13 existing ignored), and the
   direct-include Rust analyzer passed (340 passed, 3 existing ignored). A later
   generated-form addition was verified by the focused six-test elif filter.
2. Domain review: visitation covers 40 generated clause/not/nesting combinations
   and six delegated statement contexts. Analyzer acceptance independently covers
   40 combinations with initial/elif `not` varied independently. These are bounded
   grammar cases, not a universal AST proof or memory/RSS performance claim.
3. Sensitivity review: temporarily removing only the factory scan's statement
   override failed its finite cancellation-work budget (9 polls versus 8).
   Restoring that override passed. Candidate deduplication cannot mask the
   independent ordered node positions; the facts assertion remains unchanged.
4. Platform review: source-order traversal already visits elif once and is left
   unchanged, with a focused characterization test. Local execution is macOS;
   other platforms and whole-branch corpus checks are integration work. The
   historical release behavior had no confirmed panic or missing candidates.
5. Artifact review: an initial ID assertion did not compile because plan IDs are
   strings and the core helper returns `MutantId`; compare its `as_str()` value.
   The candidate-column test initially failed because of a one-based expectation;
   the documented zero-based contract corrected the test. Focused debug results
   then passed (6 library tests and 2 public plan tests). Release also passed the same 6 library and 2 public plan tests with the
   normal optimized profile. Commit-hook outcomes are recorded in the external
   `task-2-report.md` alongside command logs.

## Evidence locations

Task-local logs live under `/tmp/hoimin-issues-984-989/`: `task-2-red.log`,
`task-2-red-plan.log`, `task-2-red-default-path.log`,
`task-2-green-analyzer.log`, `task-2-green-rust-analyzer.log`,
`task-2-expected-column-correction.log`, `task-2-green-elif.log`,
and `task-2-release-elif.log`. The task report records exact commands and exits.

The initial `cargo fmt` invocation yielded a running session; a test build was
started before its session completion was observed. Formatting then exited 0.
No concurrent compiler build occurred; subsequent Cargo calls wait for complete
sessions. This process deviation does not change the test outcome.

Release verification completed with exit 0: 6 library tests and 2 public plan
tests passed, including all bounded generated forms, the zero-candidate minimum
and candidate descriptor/ID/order checks. The optimized build used normal thin
LTO and took 3m 36s in the owned target.

The first normal commit hook attempt found three test-only Clippy issues:
`format_push_string`, `format_collect`, and `assert_is_empty`. Fixture generation
was changed to `writeln!` into owned strings, and the empty candidate assertion
now uses `assert_eq!` for useful failure output. The production walker and tested
Python inputs did not change. The failed hook log is retained as
`task-2-commit-clippy-correction.log`; hooks are rerun normally after correction.
