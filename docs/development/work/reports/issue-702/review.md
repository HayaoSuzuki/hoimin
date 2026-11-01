# Issue 702 review and evidence

## Implementation self-review (five passes)

1. Atomic elements: names, single string/number/Boolean/None literals and signed
   numeric literals only. Bytes/fstrings, concatenation and compound expressions
   fail eligibility. Identical raw atoms and normalized names are skipped without
   claiming complete value-equivalence detection or Python type preservation.
2. Patch boundaries: tuple span encloses two AST element spans. Swapped text uses
   unchanged prefix, separator and suffix slices; commas inside strings are never
   interpreted. Added parentheses for keyword-adjacent unparenthesized tuples so a
   name destination cannot combine with return. No generated temporary evaluation.
3. Own scope: shared scanner's reject_value_return policy is true for body erasure
   and false for swap eligibility. Both reject own suspension, skip nested bodies,
   and inspect evaluated nested headers. Function entry/restoration tracks explicit
   ReturnScope states; async/generator bodies remain excluded.
4. Traversal/integration: extracted statement mutation collection to keep traversal
   control readable. An observed cancellation stops traversal afterward. Existing
   definition, exception and expression traversal retains order. New ID is opt-in,
   Behavioral-ranked, with unchanged default selection and existing candidate IDs.
5. Resources/selection: one source span per return, no extra AST or runtime evaluation;
   scanner/token traversal checks cancellation, shared bounded prefix and serialized
   size guard remain. Line/symbol selectors apply to the tuple's first source line.

## Test self-review (five passes)

1. RED: three public tests failed on unknown selector; all passed after implementation.
   Existing six function-body-erasure public tests passed after the shared scan change.
2. Syntax/source: literal expected replacements cover parentheses/trailing comma,
   multiline comments, strings containing commas, Unicode and keyword-adjacent string
   or signed number. Generated source compiles with CPython 3.14; repeated plans have
   identical candidate IDs/order. Normalized-name no-op and comprehension exclusions
   were added to the scope/atom matrix.
3. Scope: async and own yield/yield-from excluded; nested generator bodies do not
   exclude the parent; yield in a nested default does. A sync child inside async
   remains eligible. Zero/one/three elements, non-return tuples and impure atoms skip.
4. Meaning: saved-plan baseline passes; length-only assertion survives and ordered
   pair assertion kills. Real packaging component validation distinguishes positions
   and kills all three candidates after a suitable assertion is added.
5. Integration: default/exclusion, source-prefix cap, line/symbol and cancellation
   assertions supplement public cases. Shared body-erasure tests protect the new
   value-return policy. Full workspace and independent review cover other operators.

Independent review found no defects. Extra CLI probes compiled and returned reversed
pairs for parenthesized elements, keyword adjacency, multiline signed atoms, internal
comments, multiline strings, complex literals and explicit continuations. Clippy
requested avoiding a fourth Boolean collector field; explicit ReturnScope expresses
excluded/plain-sync states. Final targeted run passed after this representation change.

## Formal correspondence

ReturnTupleSwap.lean has five kernel-checked theorems: each position receives the
other element, involution, identical-element stability and changed result for distinct
elements. Three examples show pair reversal and why length cannot detect it, including
a broken identity implementation witness. Lean 4.32.2 exit 0 in 2.248s, external 20s
deadline. No axioms/sorry/native_decide.

Model-only: the abstract element domain need not be a Python runtime type; it can
represent tagged heterogeneous values. The proof does not establish Rust eligibility,
Python lexical reconstruction, name lookup or general equivalence. Public CPython
and scanner tests check that correspondence independently, not via generated oracles.

## Project probes

Packaging 26.3 version.py yields three tuple candidates. Import-only and initial
basic Version tests survive all three. copy.replace with pre/post/dev components
and exact resulting string kills all three. All candidates compile; passing baseline
and no timeout/error/inconclusive mutant result. Iniconfig selected module has none.
An initial probe mistakenly used Version.replace (absent in this environment);
baseline failed and was not counted. Corrected to the supported copy.replace API.
These are authored probes, not upstream suites or equivalence proofs. Exact commands,
counts/timings are in project-trials.json; temporary package copies were deleted.

Rust commands: cargo test --workspace --offline; cargo test -p hoimin-cli --test
return_tuple_swap --test function_body_erase --test rust_analyzer --offline;
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings;
cargo fmt --all -- --check. Debug info/incremental off, existing CPython 3.14 venv.
Formal: lake env lean ReturnTupleSwap.lean from formal/HoiminOracle under 20s timeout.

Workspace suite completed with exit 0. Final focused run: 3 tuple-swap public,
6 body-erasure public and 285 analyzer tests passed (3 ignored). The two additional
normalized-name/comprehension fixtures also passed in the final public rerun.
All-target/all-feature clippy, fmt, whitespace and 29-file OKF checks passed.

The commit hook caught a non-NFC character in the late normalized-name fixture.
Used the Rust Unicode escape for the same Python source character, preserving the
regression intent; repeated the public test and clippy before committing.
