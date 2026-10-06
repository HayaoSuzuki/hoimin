# Issue 696 review and validation

Design and plan each have five separately recorded review passes in their own
files. These are self-reviews, not independent reviewer approvals.

## Implementation self-review

1. Eligibility pass: traced visit_stmt; only Expr(Call) enters the gate. Assign,
   return and definition nodes only recurse normally. No candidate from annotations.
2. Nested-expression pass: walked RemovalCheck, including lambda bodies and keyword
   arguments. Named/Await/Yield/YieldFrom stop eligibility; cancellation stops descent.
3. Edit pass: checked statement range and single `pass` replacement; indentation,
   semicolons and comments remain outside the edit. Encoded byte tests exercise the
   public decoder/encoder boundary, rather than assuming UTF-8 offsets.
4. Integration pass: compilation exposed exhaustive ranking dispatch and the full
   suite exposed ranking inventory expectations. Added Behavioral classification,
   inventory registration and README count. Default sets remain unchanged.
5. Cost/compatibility pass: eligibility traversal runs only when selected; shared
   bounded CandidatePrefix, profile and make_candidate paths remain authoritative.
   Enum variant is appended, existing names and ID hashing are unchanged.

## Test self-review

1. RED evidence: both new public tests failed on the unregistered selector before
   implementation; the failure was not an import or fixture failure.
2. Exclusion pass: covered await, yield, lambda-yield, named binding, annotations,
   alias RHS, assignment and return; added yield-from as its own exclusion case.
3. Syntax pass: CPython 3.14 compiles single-suite, inline-suite, semicolon,
   multiline/comment, Unicode and class-suite mutations; added outer parentheses.
4. Observation pass: saved-plan verify checks baseline success plus both killed and
   survived counts. Repeated plan candidate arrays must match, and original source
   must remain intact. Weak save test survives; observing saved value kills.
5. Environment/boundary pass: full-suite failures in abstract_set_provider were
   missing worktree .venv, not semantic failures. Linked the existing interpreter.
   Added line/symbol/limit/cancellation probes and nine encoding/newline cases.

## Execution evidence

Baseline analyzer: 277 passed, 3 ignored. New public tests: four passed before the
final extra syntax cases. Lean 4.32.2 `lake env lean StatementDelete.lean`: exit 0,
3.626 seconds, external 20-second deadline, no unbounded tactics or enumeration.
Three theorems, five boundary examples and one broken-gate witness; model-only.
All-target/all-feature clippy: exit 0. Full suite final result is recorded below.

The small fixture demonstrates a missing state update. Multi-project effectiveness
and equivalence rates are not established by this fixture; no equivalence or
paper-level effectiveness claim is made.

Final workspace suite: **2,609 passed, 0 failed, 22 ignored** across 130 test/doc
suite summaries. Targeted rerun after extra boundary tests: analyzer 279 passed,
3 ignored; public statement deletion 4 passed. Independent review found no
critical/important/minor defects; optional recommendation: assert complete mutated
text in addition to the byte-range, compile, and behavior checks.

## Real-package probes

`project-trials.json` records installed packaging 26.3 and iniconfig 2.3.0 copied
to disposable roots. These are small authored probes, not upstream test suites.
Both baselines pass. Packaging has 2 candidates: import-only survives both;
checking `_TrimmedRelease('1.0.0').release` kills the missing superclass
initialization (1 killed, 1 survived). Iniconfig exceptions has 1 survivor under
both probes. All 3 unique mutations compile under CPython 3.14; no timeouts/errors.

Manual survivor inspection: packaging's warnings.warn is in the Python <3.13
fallback, unreachable in this 3.14 probe (environment-specific equivalence).
Iniconfig's repeated Exception initialization preserves the observed args/str
for the supplied positional constructor, but arbitrary subclasses and invocation
forms were not classified. Do not label either universally equivalent.
The first iniconfig __init__ trial had zero eligible calls. `_parse.py` aborted
at `fact_index.rs`'s duplicate unary-not assertion; that input is an infrastructure
failure and excluded from the mutation score. No broad effectiveness rate follows
from these selected files and probes.

## Reproduction

From this worktree, use Python 3.14 via `.venv` (local verification linked the main
checkout's pre-existing environment). Rust commands used
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.

- `cargo test --workspace --offline`
- `cargo test -p hoimin-cli --test statement_delete --test rust_analyzer --offline`
- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`
- `cargo fmt --all -- --check`
- `cd formal/HoiminOracle && lake env lean StatementDelete.lean` (20s external deadline)
- OKF: safe YAML/frontmatter and reserved-file validation, 29 Markdown files passed.

For package trials: copy the named installed package to a temporary root, run
`hoimin plan --root ROOT --file FILE --operators statement_delete --max-candidates 10
--min-free-space 1B --allow-best-effort-memory --baseline-timeout 10s
--mutant-timeout 10s --total-timeout 60s -- PYTHON314 -B -c TEST_COMMAND`, save the
JSON, then `hoimin verify PLAN --top 10 --format json`. FILE and TEST_COMMAND are
recorded verbatim in project-trials.json. Compile each candidate-applied byte string
with Python 3.14 before interpreting its outcome. Temporary roots were removed.
