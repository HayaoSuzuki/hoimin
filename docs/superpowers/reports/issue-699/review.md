# Issue 699 review and evidence

## Implementation self-review (five passes)

1. Scope ownership: reviewed nested function/class/lambda branches. Body skipped,
   evaluated headers/defaults inspected; deferred annotations/type aliases skipped.
   Own value returns and suspension stop eligibility even inside compound statements.
2. Source boundary: erase first non-docstring statement through last statement end.
   Decorators on a nested first definition are part of its statement range; top-level
   decorators/docstring/header and suffix comments remain outside replacement.
3. Candidate ownership: found anchor scope can name a nested first definition.
   Added add_candidate_in_scope using the erased function's own definition start;
   ordinary candidates pass their original range, preserving their lookup behavior.
4. Selection/resources: first erased line remains the line selector anchor, while
   owner symbol is outer function. Shared CandidateStore record-size rejection and
   candidate limits remain authoritative. Traversal polls cancellation.
5. Eligibility/defaults: explicit opt-in, Behavioral rank; async, dunder, value-return,
   generator and trivial stubs excluded. Nested eligible functions can be separate
   mutants; no inference of dynamic coverage or equivalence from survival.

## Test self-review (five passes)

1. RED: three new public tests failed on unknown selector before implementation.
2. Scope matrix: bare/None/value returns, async, yield/yield-from, special methods,
   no-op bodies, nested returns/generators/lambda and nested-default yield are explicit.
3. Span/encoding: direct original span expectations, preserved prefix/docstring,
   single-line and semicolon suites, Unicode and all nine encoding/newline pairs.
4. Integration: line selection only at first erased statement; nested symbol cannot
   select outer body; explicit default exclusion/cancellation; >2 MiB serialized
   body fails with the existing analyzer.store diagnostic rather than partial edit.
5. Meaning: weak state fixture survives, strong state observation kills; added a
   cap/min fixture where body erasure survives yet collection_min_max is killed,
   so coarse erasure cannot substitute for fine-grained mutation.

Initial focused checks: 5 public tests and 282 analyzer tests passed (3 ignored).
Independent review found no concrete defects. Additional reviewer probes covered
nested decorator/class-base suspension, decorated definitions, class methods,
nested async/nonlocal and exception-handler returns, and docstring continuations.
Optional recommendation: keep decorator/base-suspension probes as permanent fixtures.

## Formal correspondence

| Premise / observation | Lean | Rust evidence | Mode |
| --- | --- | --- | --- |
| Async/special/valued/suspension/no-op classification | five Bool inputs | function_body scanner | model-only |
| Header/docstring and suffix | abstract token lists | CPython span tests | model-only |
| Nested header versus body suspension | separate Bool inputs | scope matrix and reviewer probes | model-only |

Five kernel-checked theorems prove three gate exclusions and preserved header/suffix
in the abstract replacement model; four fixed examples include a deliberately
broken nested-scope skip witness. List token 0 represents pass abstractly; the model
does not prove Python lexical composition or scanner classification. No native
corpus correspondence claimed. Atomicity/replay are inapplicable to this pure model.
Lean 4.32.2 exit 0 without warnings in 2.471s, external 20s deadline, no costly search.

## Project probes

project-trials.json records packaging 26.3's deprecated Version._version setter:
one candidate survives import-only; checking a real setter update kills it.
CPython syntax valid, passing baseline, no timeout/error/inconclusive outcome.
Iniconfig __init__.py yields no eligible functions, which illustrates the deliberate
restriction on dunder/value-return functions. These are authored probes, not upstream
suites. No equivalence, dynamic coverage, pseudo-tested status or subsumption claim.

Reproduce with #697's package-probe commands, operator function_body_erase, candidate
limit 1000, top 10, and the exact test commands/files in project-trials.json.
Temporary copied packages were removed. Rust checks use debug info/incremental off
and the existing Python 3.14 .venv; commands: cargo test --workspace --offline;
cargo test -p hoimin-cli --test function_body_erase --test rust_analyzer --offline;
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings;
cargo fmt --all -- --check. Formal: lake env lean FunctionBodyErase.lean in
formal/HoiminOracle, under a 20-second subprocess deadline.

Final workspace suite: **2622 passed, 0 failed, 22 ignored**, 131 suite summaries.
Final focused run: 6 public and 282 analyzer tests passed, 3 ignored. Clippy (all
targets/features), format and OKF (29 files) passed. Clippy requested a semicolon
on a unit-return assignment arm; fixed without changing behavior.
