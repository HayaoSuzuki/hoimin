# Issue 698 self-review and validation

## Implementation self-review (five passes)

1. Clause coverage: if.test and each Some(elif.test) collected independently; else
   has no test. Existing visitor still visits clause bodies and other operators.
2. Eligibility: bool literals excluded; shared cancellable checker rejects nested
   named/await/yield/yield-from, including lambdas; no source-text classification.
3. Span/side effects: only test range replaced, so body, colon and outer delimiters
   remain. Original condition callee/arguments are not evaluated, by design.
4. Filtering/ordering: add_candidate retains profile/line/symbol and prefix checks;
   collecting elif before body is safe because CandidatePrefix orders by source span.
5. Registration/cost: appended variant, HighValueControl category, explicit registry;
   default set untouched. Extra condition scans only run when operator selected.

## Test self-review (five passes)

1. RED: three public tests failed on unknown selector before implementation.
2. Syntax coverage: if/elif/else, Unicode CRLF, parentheses/comments and explicit line
   continuation compile as actual CPython 3.14 code after each candidate is applied.
3. Exclusions: tests distinguish bool literal from nonliteral conditions and cover
   other predicate forms plus nested binding/suspension nodes.
4. Behavioral observation: weak test observes enabled branch only (1 kill/1 survive);
   strong test observes disabled branch too (2 kills); passing baseline required.
   Added direct execution asserting zero predicate calls and the forced branch value.
5. Contract integration: repeated plans require same ordered descriptors; added
   focused-main-guard, opt-in, candidate-limit and cancellation probes.

Lean ConditionConstant.lean: five kernel-checked theorems for exclusions, selected
branch and removed condition effects; three finite eligibility examples and one
broken-safety-gate witness. Exit 0 in 2.334s under external 20s deadline. Model-only:
no claim that the flags prove Rust classification or that arithmetic outcomes model
all Python control effects. Atomicity/replay families do not apply to this pure model.
Initial focused run: three public tests and 280 analyzer tests passed (3 ignored).

## Independent review correction

P2: bare True/False could merge with the if/elif keyword in valid `if[]:` source.
Added five compact syntax regressions, observed CPython SyntaxError RED (`ifFalse:`),
then parenthesized constants. This is a production fix; the initial green syntax
examples did not cover keyword adjacency. Final suite must be rerun after this fix.

Adjacency correction GREEN: all four public tests passed, including the five new
compact-if/elif sources; side-effect and saved-plan behavior also pass. The final
all-target/all-feature clippy run after the correction passed.

Real-package authored probes (project-trials.json): iniconfig 2.3.0 __init__.py
has 10 candidates, all survive import-only, 3 killed / 7 survived with a parsed-value
assertion. Packaging 26.3 version.py has 138 candidates; only ranked top 10 executed,
all survive both small probes. All 148 candidates compile under CPython 3.14.
All baselines pass; no timeout/error/inconclusive outcomes. Equivalence is not
classified. These are selected local package copies, not upstream test suites.

Reproduction: use operator condition_constant and candidate limit 1000 in the
package-probe commands described for #697, compile all candidate-applied byte
strings, verify ranked top 10, and use FILE/TEST_COMMAND from project-trials.json.
Temporary copied packages are removed after each probe. No claim of subsumption.

Verification commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0`, the existing .venv Python 3.14 environment, and:
`cargo test --workspace --offline`; `cargo clippy --workspace --all-targets
--all-features --offline -- -D warnings`; `cargo fmt --all -- --check`;
`cd formal/HoiminOracle && lake env lean ConditionConstant.lean` (20s deadline).

After the adjacency fix, full workspace suite: **2,618 passed, 0 failed, 22 ignored**,
132 suite summaries. Format and all-target/all-feature clippy passed. OKF safe YAML
and reserved-file checks: 29 passed. Independent P2 finding is fixed and verified.
