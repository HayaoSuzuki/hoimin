# Python Behavior Mutation Implementation Plan

> Use superpowers:subagent-driven-development to implement and independently review each domain.

**Goal:** Implement every proposed additional and strengthened mutation through PR and final-head CI.

**Architecture:** Reuse typed selection and bounded collector admission. Domain
modules own Python semantics; one integration owner edits the central visitor.

**Tech Stack:** Rust, Ruff Python AST/tokens, rstest, proptest, CPython 3.14/3.15.

**Spec:** ../designs/2026-11-23-python-behavior-mutations.md

## Global constraints

- Fifteen new stable IDs, appended and default enabled; all individually opt out.
- Five self-review passes each for design, plan, implementation and tests.
- One scenario per test invocation, parameterization, properties and AAA.
- Typed internal dispatch, shared CLI/web documentation, Windows verification.
- Static discovery; no new dependency and no private project artifacts.
- Base 11891252 (merged PR #1208); create the PR against main.

## Review focus

- Attribute/subscript targets with effectful descriptors: evaluate once, correct order.
- Removed expressions containing walrus/yield/free reads: retain lexical facts.
- Custom __index__, __exit__/__aexit__, and formatting: preserve non-target effects.
- Regex references/lookbehinds and interpolated fields: generate valid syntax.
- Operator selected alone, tight candidate budgets and deep grouping: apply all guards.

## Task 1: Assignment and result semantics

Files: new analyzer/rust/assignment_results.rs and its focused tests; new public
CPython witness integration. Root owns rust.rs wiring and registry.

Implements augmented_to_binary, assignment_target_swap, return_value_none,
identity_equality_swap, else_suite_remove. Helpers consume source/AST/tokens,
DelimiterDepth, selection and cancellation and emit typed span edits through a
stop-aware callback. Prove alias changes, target/RHS order, per-return effects,
comparison-chain behavior and else lexical retention. Include sync/async returns,
inline suites, nested targets and binding/suspension effects.

- [x] Red public discovery and semantic regressions.
- [x] Implement, test, five implementation/test reviews and report wiring.
- [x] Independent spec/quality review and fix findings.

## Task 2: Regex and formatting

Files: analyzer/rust/regex.rs and regex/*; formatted_conversions.rs and focused
tests. Root owns registry and collector wiring.

Implements three regex IDs and fstring_format_option_swap. Extend existing trusted
pattern source mapping and grammar, not a second regex parser. Test text/bytes,
flags, escapes, nested alternatives, quantifiers, lookbehind/backrefs and invalid
combinations. Format tests include alignment/sign/grouping/type and dynamic fields.

- [x] Red tests, then implement using typed variants and bounded emission.
- [x] CPython witnesses, properties and five implementation/test reviews.
- [x] Independent spec/quality review and fix findings.

## Task 3: Boundaries, calls and expression coverage

Files: new analyzer/rust/dynamic_boundaries.rs; existing expression_behavior.rs,
control_structure.rs where it owns return tuple logic, focused tests. Root owns
central collector wiring; coordinate old helper replacement explicitly.

Implements zip_strict_toggle, range_bound_neighbor, compare_direction; extends
index/slice neighbors, return_tuple_swap and condition_clause_delete. Resolve
builtins statically; preserve __index__ and evaluation order. Test named/computed
positions, reverse steps, three-element and effectful tuple returns, while and
ordinary Boolean values, standalone selection and max-candidate prefixes.

- [x] Red tests, implement each semantic unit, retain old simple outputs where possible.
- [x] Properties, CPython witnesses, five implementation/test reviews.
- [x] Independent spec/quality review and fix findings.

## Task 4: Context managers, yield and integration

Files: new analyzer/rust/lifecycle_mutations.rs; rust.rs; core config/selection
tests; CLI ranking/inventory; shared operator docs and generated reference.

Implements context_exit_omit, context_exit_result_negate and yield_statement_remove.
Use native special-method lookup through a local proxy; descriptor witnesses
ruled out the initial standard-library stack approach. Test entry/exit failure, sync/
async, multiple managers, target assignment failure, cancellation, generator
close/send/throw, closure facts and inline/nested suites. Integrate all producer
callbacks, registry, default/opt-out tests, ranking and formal coverage inventory.

- [x] Red public discovery suite for all fifteen IDs.
- [x] Lifecycle implementation and behavioral witnesses, five reviews per stage.
- [x] Integrate agents, run focused tests, regenerate documentation.
- [x] Full workspace, Clippy, docs and Python 3.15 witnesses; fix actual failures.
- [x] Whole-branch review and final design score.

Publish the reviewed change as a pull request against `main`. The pull request
carries the final-head CI status; successful CI is required before completion.

## Five plan reviews

1. Coverage maps every domain to one task; lifecycle is a required task.
2. Registry and central collector have one owner; domain file ownership is explicit.
3. New ID identity is established before typed implementations; observable red tests
   precede behavior, and missing imports/build failures are not semantic evidence.
4. Every resource/scope failure mode has a domain test obligation, including single
   operator selection and effects hidden by plain value assertions.
5. Local, complete workspace and CI evidence are distinguished; a successful
   targeted rerun must not be described as an initially clean broad run.

## Five implementation reviews

1. Registry and dispatch: all fifteen IDs are appended, default enabled and
   individually excluded. Exhaustive Rust matches, independent inventory tests,
   the shared manual and the generated CLI reference describe the same set.
2. Evaluation and ownership: augmented assignment captures each operand once;
   tuple swaps preserve original load order. Dynamic index conversion remains
   deferred and retryable. Held exceptions must not introduce extra references
   to receivers or exit decisions through generated helper frames.
3. Scope and protocol: dead syntax preserves bindings and generator classification.
   Generated helper snapshots have explicit timing and independent storage.
   Context omission preserves descriptor lookup and other managers' unwinding;
   suppression truth is requested after method cleanup, only on exceptional exit.
4. Grammar and bounds: lexer indentation handles physical continuations; delimiter
   admission accounts for generated syntax. Regex width propagation retains
   competing assertion identities and limits scratch allocations. Candidate
   deduplication includes edit length as well as start, operator and replacement.
5. Integration: operand and suite selection are separate from edit coordinates.
   Plan ranking uses resolved target restrictions for both covering edits and
   disjoint fallthrough insertions. Saved-plan verification rediscovers the
   requested candidates under those restrictions.
   Existing schema and identity rules remain unchanged. Runtime limitations are
   documented without claiming a formal proof of arbitrary Python behavior.

## Five test reviews

1. Red evidence: public discovery initially failed for all fifteen new IDs.
   Review regressions were reproduced before corrections, including ordered
   effects, retained exceptions, multiline selection and competing regex bounds.
   Compilation errors were corrected before treating a run as semantic evidence.
2. Case independence: named parameterized invocations isolate the scenario being
   exercised. Runtime witnesses arrange a source program, apply one selected
   edit and assert its effects. Explicit-return witnesses select the exact return
   expression instead of relying on another candidate's ordering.
3. Properties: bounded-prefix, parseability, tuple evaluation, format spans,
   regex alternatives and sparse constraint arithmetic are checked with generated
   inputs. Constraint-map properties use an independent contribution-array oracle;
   line-range properties compare indexed queries with a linear predicate.
4. Runtime boundaries: witnesses exercise CPython 3.14 and 3.15 on Windows,
   including custom descriptors, `__index__` retries, asynchronous cancellation,
   truth-test failures, cleanup ordering and generator send/throw/close behavior.
5. Regression scope: focused passes, full-workspace results and CI results are
   recorded separately below. Failed older inventory or emitted-source assertions
   are checked against the intended added behavior before their expectations change.
   Build artifacts are cleaned between verification phases to control disk use.

## Corrected-snapshot verification

The following checks ran on Windows after the final correction wave. The initial
broad run had sixteen failures in five CLI targets; the corrected run has none.
Transport and model fixtures now select their intended operators explicitly.
Tests of default enumeration include the added operators. The six new runtime
and saved-plan regressions failed before their fixes and passed afterward.

| Check | Result |
| --- | --- |
| `cargo test --locked -j 2 --workspace --no-fail-fast -- --test-threads=4` | Passed, including doctests and existing oracle integrations; existing ignored fixtures retained |
| `cargo clippy --locked -j 2 --keep-going --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| New runtime witnesses, CPython 3.14.9 | 110 passed within the workspace run |
| Same runtime witnesses, CPython 3.15.1 | 110 passed: 34 assignment, 26 boundary, 35 lifecycle, 15 regex/format |
| CLI-reference generator tests and `--check` | 10 passed; generated reference current |
| Documentation and knowledge-reference pytest suites | 51 passed |
| Documentation site build | Passed |
| Knowledge structure and references | 65 Markdown files passed YAML/reserved-file checks; local references resolved; both new design-source hashes matched |

These checks validate the supported source constructions and their admission
rules. They do not establish a formal proof of arbitrary Python runtime behavior.

The final scoped review approved all four production corrections, the sixteen
integration-fixture corrections and shared helper placement. It found no new
defect in the correction and scored the design 10/10 across eight diagnostics.
