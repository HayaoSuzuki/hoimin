# Issue 703 review and evidence

## Implementation self-review (five passes)

1. Value/token: AST StringLiteral, single part and decoded nonempty value gate the
   replacement. Raw/u/triple and surrogate escapes remain nonempty; escaped line
   joins can be empty. Whole literal span becomes "", retaining outside source bytes.
2. Roles: first string expression only in module/class/function bodies is excluded.
   Later standalone strings and first strings in conditional bodies stay eligible.
   Entire f/t expression intervals exclude nested string fields as well as text.
3. Types: shared annotation/PEP695 intervals and pattern state remain authoritative.
   Added selected-only conservative PEP613 marker import scan; aliases and module
   qualification handled. Found quoted-marker gap in self-review, added RED fixture
   then fixed. Independent review found parenthesized quoted markers; retained both
   regressions and normalized grouping/trivia without parsing or executing strings.
4. Resources: cancellation latched during both role/index passes; exclusion queries
   use the existing logarithmic containment index. Only selected operator builds it.
   Quoted marker normalization is linear in already-decoded input; no recursive parse.
   Candidate cap/serialized size limits remain shared.
5. Integration: new opt-in ID appended with Behavioral rank; defaults remain 43.
   Line/symbol/profile selection and stable IDs use the shared candidate path. Ordinary
   messages are not excluded based on callable spelling; Full profile includes print.

## Test self-review (five passes)

1. RED: three initial public tests failed on unknown selector; all passed after
   implementation. Quoted marker fixture separately failed then passed after fix.
2. Roles: module/class/function docs versus ordinary expressions; normal annotated RHS,
   forward annotations, Literal, PEP695 and PEP613 aliases, f/t fields, bytes and adjacent
   concatenation. Match pattern stays unchanged while a string in its guard is eligible.
3. Bytes/values: raw/u/triple, escapes/Unicode/surrogate and an escaped-line-join empty
   value; all nine UTF8/BOM/Latin1 × LF/CRLF/CR combinations. Duplicate strings have
   distinct spans; applying either edit changes exactly one value and compiles in 3.14.
4. Effectiveness: saved plan baseline passes; str-type assertion survives, exact content
   assertion kills. Real package probes add public-export/encoding and normalization
   checks after initial basic probes miss candidates; no equivalence inference.
5. Limits/selection: opt-in/exclusion, bounded source prefix, line/symbol and cancellation
   tests complement deterministic full candidate arrays. Reviewer’s two parenthesized
   quoted marker cases failed before the normalization fix and now pass independently.

Independent review found the parenthesized quoted PEP613 marker gap. Both forms
('(TA)' and '(ty . TypeAlias)') now yield zero candidates, independently verified
alongside CPython get_type_hints resolving them to TypeAlias. Other compact syntax,
ordinary statement and nested interpolation probes passed. Conservative marker
shadowing/normalization can over-exclude; it is documented rather than called exact
Python type resolution.

## Formal correspondence

StringLiteralEmpty.lean contains five kernel-checked theorems: erased result empty,
nonempty content changes, and exclusion gates for empty/nonruntime/concatenated inputs.
Three examples include an identity-edit counterexample. Lean 4.32.2 exited 0 in 2.159s
under external 20s deadline, no axioms/sorry/native_decide.

Model-only: the proof assumes the runtime/single/nonempty classification and models
string contents. It does not prove Rust scope indexing, parser decoding or lexical
source replacement. CPython value/byte tests and role fixtures check correspondence
separately. No generated oracle or complete equivalence claim.

## Real-package probes

Iniconfig 2.3.0: eight candidates; import/basic parsing probes survive all. Added
export-list, TypeVar-name and file/default-encoding assertions kill all eight.
Packaging 26.3: ninety candidates all compile; execute top ten. Import/basic Version
probes survive these ten; checking normalize_pre outputs kills all ten. The other
eighty candidates were not executed. Exact commands/counts/timings are recorded in
project-trials.json. All recorded baselines pass; no timeout/error/inconclusive
mutant outcome. Temporary copied packages were deleted. These authored probes are
not upstream test suites and do not establish equivalence for surviving candidates.

Workspace suite exit 0. Final focused run after reviewer fix: 4 public + 286 analyzer
passed, 3 ignored. Final all-target/all-feature clippy passed. Commands: cargo test
--workspace --offline; cargo test -p hoimin-cli --test string_literal_empty --test
rust_analyzer --offline; cargo clippy --workspace --all-targets --all-features
--offline -- -D warnings; cargo fmt --all -- --check. Debug info/incremental disabled.
Formal: lake env lean StringLiteralEmpty.lean in formal/HoiminOracle with 20s timeout.

Formatting, whitespace and OKF structure/YAML checks passed (29 files).
