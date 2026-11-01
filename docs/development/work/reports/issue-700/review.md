# Issue 700 review and evidence

## Implementation self-review (five passes)

1. Eligibility and context: inspected class discovery, attributes and visitor gates.
   Only top-level directly based enums are indexed; annotation/pattern gates remain
   shared. The definition's own range is excluded, as are Store/Del attributes.
   Ordinary methods never enter member sets; unsupported decorators are diagnosed.
2. Binding identity: the original resolver records only builtin occurrences. Added
   an explicit extra-name path for this index, keeping existing builtin construction
   unchanged. Unique unconditional module binding plus lexical scope checks excludes
   local/parameter/comprehension shadowing and rebinding. Global declarations,
   direct aliases, namespace dictionary access and writes invalidate conservatively.
3. Value and spelling: use exact bounded integer/decoded string keys and distinct
   canonical groups. Independent review exposed surrogate decoding loss and NFKC
   keyword spellings. Reject U+FFFD values; retain original destination token text
   separately from normalized name lookup. ASCII StrEnum auto aliases collapse.
4. Resources: replaced repeated value comparisons with cached hash-map groups and
   alias lookup. Buffer at most max_candidates + 1 destinations per reference;
   existing prefix retention decides truncation, and scans/enumeration poll cancellation.
   Shared parser/resolution prepasses retain their existing cancellation boundaries.
5. Integration: appended operator enum, Behavioral rank and inventories; defaults
   remain 43 IDs. Checked diagnostics through Rust output, protocol validation,
   plan and run mapping. Only member-token replacement affects candidate identity;
   existing operators and ranking-rule version remain unchanged.

## Test self-review (five passes)

1. RED: both initial public tests failed on unknown enum_member_replace selector.
   After implementation the ordinary literal/alias/auto and scope matrices passed.
2. Value coverage: integer signs/hex, decoded strings, standard auto and StrEnum
   A/a aliases; singleton-alias-only definitions yield no mutation. Fixture execution
   under CPython 3.14 independently checks three distinct members and destinations.
3. Scope and source: return/assignment/comparison/argument Load contexts, Unicode
   comments, CRLF, comments before attribute tokens, local/parameter/global writes,
   comprehensions, annotations and patterns. Exact replacement tokens and repeat
   candidate arrays are asserted; generated mutants compile and execute.
4. Review regressions: a new test failed before fixes on global deletion. Added the
   independent review's surrogate, namespace-dictionary, module-alias and fullwidth
   identifier cases. All five public tests then passed, with 283 analyzer tests
   passing (3 ignored), including opt-in/exclusion, bounds, line/symbol and cancellation.
5. Effectiveness: a saved plan with two alternatives survives type-only assertions
   (0 killed, 2 survived) and is killed by identity assertions (2 killed, 0 survived).
   Real LibCST probes repeat the same contrast. No equivalence or coverage claim.

Independent review found four defects, all retained as regressions and fixed.
The follow-up review result and final workspace validation are recorded below.

## Formal correspondence

| Claim | Lean premise | Implementation evidence |
| --- | --- | --- |
| Same member values excluded | Natural-number value group IDs | exact-value/hash grouping and CPython cases |
| Destinations exist | membership in canonical list | source member index and executed fixture mutants |
| Alias inflation excluded | canonical list has no duplicates | hash-map grouping, A/a and explicit-alias tests |
| Count bounded | list filter cannot grow input | prefix retention plus per-reference max+1 buffer |

EnumMemberReplace.lean has four kernel-checked theorems and four examples, including
a broken spelling-only alias rule witness. Lean 4.32.2 exited 0 in 2.277 seconds
under an external 20-second deadline. Model-only: the proof assumes group identity
and canonical uniqueness; it does not prove the Python parser, Rust resolver,
Unicode decoding, source patching or runtime semantics. No sorry/axioms/native_decide.

## Real-package probes

project-trials.json records the exact files, commands and counts. LibCST's
codemod/_runner.py yields two replacements for SkipReason.OTHER. Import-only checks
survive both; an authored SkipFile/transform_module assertion kills both. Baselines
pass; all candidates compile, with no timeout/error/inconclusive mutant outcomes.
Pytest's expression module is conservatively skipped due to dynamic namespace use.
Packaging's _ranges.py hits the pre-existing duplicate unary-not index panic,
also observed before this operator in #696; this is an infrastructure failure,
not evidence of zero candidates or successful mutation testing.

Temporary project copies were deleted automatically. Reproduction uses public
plan with enum_member_replace, max-candidates 1000, top 10 verify, 10s baseline
and mutant timeouts, 60s total timeout, and exact commands in project-trials.json.
These are authored probes, not upstream project test suites. No claim that every
same-type member replacement is non-equivalent.

Validation commands: cargo test --workspace --offline; cargo test -p hoimin-cli
--test enum_member_replace --test rust_analyzer --offline; cargo clippy --workspace
--all-targets --all-features --offline -- -D warnings; cargo fmt --all -- --check.
Rust debug info and incremental compilation disabled. Formal command: lake env lean
EnumMemberReplace.lean in formal/HoiminOracle, with external 20s deadline.

Follow-up review found typed/walrus/unpacking module aliases bypassed the first
assignment-only guard. Added three object-based regression fixtures (RED), then
rejected bare imported-module loads independently of syntax. All five public tests
and 283 analyzer tests passed again (3 ignored); the reviewer independently verified
all three exclusions and diagnostics. Final clippy passed. OKF structure/YAML: 29
files checked. Formatting and whitespace checks passed. The global-declaration and
surrogate exclusions intentionally trade recall for certainty; documented in README.

Workspace suite completed with exit 0 after the four initial review fixes. The
subsequent alias-handling correction was checked by the final public/analyzer run:
5 public + 283 analyzer passed, 3 ignored. All-target/all-feature clippy passed on
the final implementation. Existing package panic remains an explicitly recorded
limitation; no source changes outside the issue's operator work were made to fix it.
