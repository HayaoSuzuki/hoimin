# Issue 701 review and evidence

## Implementation self-review (five passes)

1. Target: AST AugAssign plus Name excludes attributes, subscripts and ordinary
   assignments. Parenthesized names remain eligible. Comment/string spelling cannot
   enter this statement path.
2. Token: Ruff's as_augmented_assign_operator must equal the AST operator. Search
   is restricted to target-end/RHS-start; replace that token alone and return once.
   Spaces, line continuations, comments and semicolons remain source bytes.
3. Semantics: = preserves RHS text but removes old-value read and in-place dispatch.
   README and design explicitly include this behavior; no numeric-only restriction.
4. Integration: appended enum/selector ID, Arithmetic ranking, operator inventories
   and README count. Default 43 runtime IDs unchanged; existing augmented_add_sub
   remains a separate candidate at the same source span.
5. Limits: cancellation is checked on each searched token and latched in collector;
   shared add_candidate handles line/symbol/profile and bounded prefix retention.
   No AST reconstruction, target duplication or additional source execution.

## Test self-review (five passes)

1. RED: all three initial public tests failed on unknown selector. Implementation
   then passed all three, confirming tests exercised the new operator path.
2. Operator completeness: each of 13 operators has exact original/replacement and
   byte-offset assertions. Includes parenthesized target, multiline RHS, Unicode,
   CRLF, comments containing += and semicolon return; mutants compile in CPython 3.14.
3. Exclusions/coexistence: attributes, subscripts, normal assignment and text are
   excluded; simultaneous legacy selection yields both = and -= in the function.
   Repeated plan candidate arrays include stable IDs and order.
4. Behavioral pair: saved-plan verify passes baseline; one-element total survives
   (0 killed/1 survived), distinct two-element total kills (1 killed/0 survived).
   Separate list-alias execution observes lost in-place mutation and changed identity.
5. Selection/resource: internal test exercises default exclusion, explicit exclude,
   first-source prefix/truncation, line/symbol selectors and cancellation. Full suite
   and independent review complement these targeted cases.

## Formal correspondence

AugmentedToAssignment.lean contains five kernel-checked theorems about integer
accumulator replacement: forgetting old state, equality for one update from zero,
original two-update sum, replacement's last-value result, and inequality whenever
the first update is nonzero. Concrete 3/5 examples
show the missing update and distinguish a broken implementation that retains +.
Lean 4.32.2 exited 0 in 2.601 seconds under a 20-second external deadline.

Model-only: integers model the aggregation fault, not arbitrary Python numeric
protocols, exceptions, destructor timing or in-place dispatch. Token selection is
checked by AST/token tests; CPython source and list-alias tests cover implementation
correspondence separately. No claim of proving Rust or of Lean-generated test oracles.
No axioms/sorry/native_decide. Crash recovery and interleavings are inapplicable.

Validation commands: cargo test --workspace --offline; cargo clippy --workspace
--all-targets --all-features --offline -- -D warnings; cargo fmt --all -- --check.
Debug info/incremental disabled; public Python uses the existing CPython 3.14 venv.
Formal: lake env lean AugmentedToAssignment.lean in formal/HoiminOracle, under
external 20-second subprocess timeout.

Independent review found no defects. Additional public CLI probes passed for
explicit continuations, nested target parentheses, compact syntax, lambda/yield
RHS and Unicode names. A custom __iadd__ probe confirmed one RHS evaluation while
in-place dispatch disappears. Clippy requested only inline format arguments in a
test; corrected and the final all-target/all-feature check passed.

Project probes: packaging 26.3 version.py has six candidates, all compile. Import-only
checks survive all six. Basic version checks also initially survived all six;
adding the composite pre/post/dev/local string assertion kills four and leaves two
unclassified survivors. Iniconfig's selected module has zero eligible statements.
Exact commands/counts/timings are in project-trials.json. These authored probes are
not upstream suites or evidence of equivalence; no timeout/error/inconclusive
mutant outcomes. Temporary copied packages were deleted automatically.

Final focused run: 4 public tests and 284 analyzer tests passed (3 ignored). OKF
structure/YAML: 29 files passed; formatting and whitespace checks passed.

Full workspace test suite completed successfully (exit 0). Final targeted tests
and all-target/all-feature clippy also passed after the test-format-only cleanup.
