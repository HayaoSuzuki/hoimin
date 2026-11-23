# Python reference mutation implementation plan

Specification: [design](/docs/development/work/designs/2026-11-23-python-reference-mutations.md).

## Tasks

1. Literal implementation: integer representation, float neighbors, bytes emptying, every string segment, f-string conversion and format specification. Own domain modules and their tests. Supply collector integration instructions rather than concurrently editing the central visitor.
2. Dispatch implementation: exception groups, handler order and causes; match constraints and order. Own dispatch modules and tests; supply integration instructions.
3. Expression and deletion implementation: conditional expressions, await, dictionary unpacking and del. Own semantic modules and tests; supply integration instructions.
4. Integrate registry/default selection and collector, update affected existing tests, executable Python contract tests and generated documentation. Run full relevant Rust checks, Python checks, documentation generation/build and Windows validation.
5. Perform task reviews and whole-branch review, remedy findings, commit, create PR and check final-head CI.

Global constraints: default enabled with opt-out; no private project material; one scenario per test invocation; parameterized examples, bounded property tests and AAA; at least five distinct design, plan, implementation and test self-review passes. Record real red/green evidence and limitations. Do not merge.

## Plan review passes

1. Ownership: registry and central visitor have one integration owner. Domain implementers edit distinct files, avoiding conflicting patches.
2. Dependencies: typed variants are agreed before integration; modules may compile independently under test before collector wiring. Build errors are not substitute red-test evidence.
3. Verification: test semantic outcomes with CPython, syntax with Ruff, and formatting/selection/defaults with repository checks. Include exclusions and resource cancellation.
4. Coverage: every design row maps to a task; each existing behavior expansion updates its regression expectations. Preserve old candidate identities outside intentionally expanded sites.
5. Release: generated CLI/web references use their shared source; final PR records observable changes, verification and any supported-site limits. Local and final-head CI results must be distinguished.

## Execution record

Implementation is complete on `codex/python-reference-mutations`, based on
merged main after PR #1207. Fourteen new stable IDs extend eleven recommendation
domains; existing integer, string, conversion and exception operators also grow.

## Implementation self-review passes

1. Registry and dispatch: append IDs, retain prior serialization order, enable all
   new IDs by default, and extend the exhaustive typed ranking match. Default and
   exclusion tests cover each new ID independently.
2. Evaluation and scope: execute witnesses for lazy conditional branches, await
   operand effects, deletion binding classification and exception causes. The
   independent review exposed free-variable loss in format, pattern and conditional
   removal; retain unreachable syntax for those reads rather than treating capture
   assignments as the only lexical effects.
3. Source validity: preserve quote-token boundaries, literal types, grouping,
   adjacent clause comments and scope directives. Extend numeric delimiters checks
   to the actual CPython nesting boundary. Move pattern grouping with operands.
4. Selection and resource cost: early selection calls on unrelated syntax caused
   the existing operation-count tests to fail. Gate literal producers by eligible
   types and operators, and defer expression rendering until candidate admission.
   Share the builtin-name universe between direct tracking and wildcard invalidation.
5. Complexity: keep representation arithmetic, format parsing and dispatch rules
   in private semantic modules. Extract try traversal to own ordinary/group handler
   differences. No new dependency or generic rewriting framework is needed.

## Test self-review passes

1. Red evidence: all fourteen default discovery cases failed before implementation.
   Domain tests independently demonstrated missing numeric, segment, format,
   expression and dispatch candidates before their implementations.
2. Scenario isolation: parameterized rows each select one scenario. Split the
   touched legacy string cap/selection test into separate tests. New tests use AAA
   and include bounded properties for values, candidate prefixes and grouping.
3. Semantic oracles: CPython witnesses check original and mutated output, closure
   cells, generator classification, side effects and mapping protocol order. Syntax
   parsing alone is insufficient evidence for these invariants.
4. Boundary coverage: exercise large integers, decimal digit-limit growth, finite
   float endpoints, both Python 3.14 and 3.15, selected inner lines, shadowed names,
   cancellation, bounded candidate counts and delimiter depth.
5. Regression interpretation: update assertions only where expanded coverage
   intentionally changes candidates. Keep performance and source-validity checks
   strict. Windows saved-plan tests need normal process permissions: AppContainer
   provisioning errors occur before mutation execution and require a separate run.

## Review decisions

Dictionary precedence mutation validates each mapping independently before
recombining snapshots. It cannot preserve arbitrary custom key collision traces
against earlier display entries. The reference explicitly states that a later
operand can run before such a collision is reported; an executable witness records
this boundary. This keeps mixed-entry dictionaries supported but requires care when
interpreting failures involving custom keys.

The independent whole-branch review found no remaining actionable issue. The
subsequent full test run exposed a collector integration omission: operators using
delimiter admission must activate its shared index even when selected alone. The
index activation list now includes every new consumer; an independent reverse
lookup of all `can_enclose` and `room_before` call sites confirmed the list. Sixteen
public-API scenarios exercise single-operator selection at ordinary and boundary
depths; all forty reference-mutation integration tests pass.

The Windows `reduce` integration tests require `PYTHON` to name the repository
virtual environment explicitly on this host. Its PATH entry points to an
unusable WindowsApps alias. Rerunning the unchanged executable with that setting
passes all eighteen tests. No timeout or expected reduction result was weakened.

## Validation evidence

- The full Windows workspace run completed all 209 test targets with 17,586
  passing tests and 45 pre-existing ignored tests. Its 23 failures in eight targets
  identified stale inventory/fixture expectations, the delimiter-index omission,
  and the Python executable setting described above.
- After those corrections, all nine affected or adjacent CLI integration targets
  pass (657 tests), as does the isolated ranking inventory regression. This is a
  full run followed by targeted reruns, not a claim that the initial run was green.
- The final analyzer library passes all 5,222 tests (five pre-existing ignored),
  and the reference-mutation public API suite passes all 40 tests.
- CPython 3.15 passes 193 expression, numeric, string and control-contract
  scenarios; CPython 3.14 is the workspace virtual environment used above.
- The documentation suite passes 37 tests and the documentation site builds.

The pull request records final static checks and CI for its actual head commit.
