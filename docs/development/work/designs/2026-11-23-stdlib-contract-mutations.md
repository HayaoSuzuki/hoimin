# Standard-library contract mutations

## Purpose and adoption

Probe text repertoires, normalization, iterator completeness, encoding policies,
URL representation and numerical contracts through small source mutations.
Use the existing local plan/verify workflow and resource budgets. The user
authorized implementation of the recommendations and other worthwhile candidates;
the additional adoptions below are the implementer's decisions.

All new operators are default enabled and individually excludable. Python 3.14
and 3.15, including Windows, remain supported. No dependency, persisted schema,
network service or user-facing CI gate is added. Public evidence uses synthetic
examples only.

## Approaches

Extending the analyzer's large expression match with individual string tests
would duplicate signature and source-layout knowledge. A configurable generic
rewrite language would expose implementation complexity and require another
validation surface. Instead, use private typed domain modules behind the existing
processing-call context, trusted import resolver and deferred candidate sink.
This approach is the implementer's choice within the authorized scope.

Static API identity is required for imported functions. Preserve supported
module/direct aliases, and reject ambiguous, rebound or escaped imports using
the existing family-local proof. String and encoding method edits remain
syntax-directed, like the existing collection method operators: their names do
not prove a builtin receiver. State that limitation in the operator reference.
Do not pretend that annotations prove runtime receiver types.
An unaliased sibling submodule import also exposes the package root for writes
and escapes. Track that hazard without treating the sibling import as evidence
that the target submodule has been loaded.
An AST's normalized import name is not always a valid source spelling: Unicode
identifiers can normalize to keywords. Preferred destination names exclude hard
keywords using the existing parser classification. A proven alternative alias or
import fallback is required when the normalized name cannot be emitted.

## Adopted operators

| ID | Behavioral contract |
| --- | --- |
| `unicode_digit_predicate_swap` | Exchange no-argument `isdecimal`, `isdigit`, `isnumeric` with each of the other predicates. |
| `unicode_digit_ascii` | Narrow each of those predicates to ASCII. Evaluate the receiver once and original predicate once; the ASCII test follows a true predicate result. Empty strings stay rejected for builtin strings. |
| `unicode_normalization_form` | For a recognized `unicodedata.normalize` or `is_normalized` call with a known NFC/NFD/NFKC/NFKD form, change one axis: composition or compatibility. Decode literals rather than replacing arbitrary text. |
| `unicode_normalization_remove` | Remove a recognized `normalize` transformation while preserving callee and supplied argument evaluation in source order. Return the original text. Removing validation performed by normalize itself is intentional. |
| `string_casefold_lower` | Exchange no-argument `casefold` and `lower`. |
| `string_affix_strip` | Exchange one-argument `removeprefix`/`lstrip` and `removesuffix`/`rstrip`. Preserve receiver and argument evaluation. No no-argument strip edit. |
| `map_strict_toggle` | Add omitted `strict=True` or negate a nonliteral strict policy at its native truth-conversion time for builtin map with at least two iterables. Explicit Boolean literals remain owned by `boolean_literal`. |
| `batched_strict_toggle` | Apply the same policy to recognized `itertools.batched`, retaining laziness and the native timing of strict conversion. |
| `encoding_errors_replace` | Change the selected encode/decode errors policy to replace, including an omitted policy. Preserve explicit policy expression evaluation. |
| `encoding_errors_strict` | Change an explicit non-strict encode/decode errors policy to strict. |
| `url_quote_swap` | Exchange quote/quote_plus and unquote/unquote_plus for recognized urllib.parse calls with compatible signatures. Default safe differences are intentional. |
| `url_query_policy` | Toggle omitted/dynamic doseq on urlencode and keep_blank_values on parse_qs/parse_qsl. Explicit Boolean literals remain owned by boolean_literal. Preserve the option's native truth timing. |
| `math_isclose_tolerance` | Independently replace explicit rel_tol or abs_tol with 0.0 after evaluating its original expression, or add rel_tol=0.0 when omitted. Skip known-zero options and the omitted zero abs_tol. Tests cover near-zero and relative boundaries. |
| `math_precision_loss` | Replace expm1(x) with exp(x)-1 and log1p(x) with log(1+x), using trusted callable identity and one evaluation of x. |
| `statistics_sample_population` | Exchange variance/pvariance and stdev/pstdev. Preserve data and optional center evaluation, translate the xbar/mu keyword role, and validate compatible signatures. |
| `heap_pushpop_replace` | Exchange heappushpop and heapreplace with compatible two-argument calls. Changed empty-heap behavior is intentional. |
| `base64_validate_toggle` | Add omitted validate=True or negate a dynamic validate policy in b64decode. Validate compatible Python 3.14/3.15 signatures; do not inject a duplicate or contradict a 3.15 ignorechars-implied default. |
| `json_allow_nan_toggle` | Add omitted allow_nan=False or negate a dynamic allow_nan policy in json.dumps/dump. Explicit Boolean literals are handled by boolean_literal. Preserve native option conversion behavior. |

Strengthen existing `encoding_errors_ignore`: accept explicit strict, replace,
other literal/dynamic policies, positionally supplied errors and omitted errors,
as well as the existing surrogateescape case. The three encoding policy IDs
share recognition and binding. Support encode/decode methods and resolved
codecs.encode/decode. File APIs require different signatures and are deferred.
Skip a policy already equal to its destination. Other encoding options remain.

## Contracts and boundaries

Each candidate changes one behavioral choice. Preserve original expression scope,
callee/receiver/argument evaluation order and single evaluation except for the
named operation removed. Never move await, yield, assignment expressions or
zero-argument super into a generated lambda body. A wrapper may accept already
evaluated expressions as arguments. Document wrapper identity, traceback and
lifetime observability instead of promising exact implementation equivalence.

Boolean option changes are not eager `not value` argument rewrites: native APIs
can defer, repeat or skip truth conversion. Share an adapter only where its
protocol preserves those observations. The original option must be available
for repeated truth checks and not retained by a failed helper frame longer than
necessary. Literal policies avoid wrappers. Tests determine API-specific timing
on both supported CPython versions before the implementation is accepted.
For JSON, an explicit `cls` must be absent or literal None: a custom encoder can
inspect a policy object without truth-testing it, so a transparent Boolean adapter
cannot establish the required contract. Standard encoder identity is subject to
the same documented external-monkey-patching limit as other imported APIs.

Validate call binding, including positional-only slots, duplicate/unknown
keywords and omitted defaults. Reject ambiguous star/double-star expansion for
fixed-signature operations. Variadic map may preserve explicit starred iterables
only if the minimum arity and keyword identity are established. Supporting aliases
must not turn failed import evidence into a nominal method match.

Perform cancellation, selection, profile and candidate-ranking checks before
source materialization. Pass edits through the existing deferred sink. Its
retained-payload reservation occurs after rendering, so one temporary replacement
is outside the retained-byte budget. Keep rendering proportional to the edited
call/expression, with no candidate matrix or retained temporary collection; do not
claim the retained budget caps total transient memory. Preserve delimiter guards
and candidate caps. Line selection anchors on an explicit edited option, or the call when an
option is inserted. Preserve comments, multiline expressions and CRLF; avoid
reconstructing unrelated arguments. New operators use typed enum dispatch after
source identifier recognition. No public rewrite DSL is introduced.
The producer supplies the selection offset separately from the physical edit
range, so inserting an option does not accidentally select a later delimiter
line. The collector retains ownership of candidate admission.

## Deferred candidates

Datetime/timedelta edits need reliable receiver evidence to avoid same-named
custom APIs; broadening that analysis is not justified by these additions.
Regex replacement groups need a separate replacement-template parser and group
identity contract. Comparison-chain deletion, slice omission, sorting/fallback
extensions and cancellation shielding change evaluation or control-flow contracts
that deserve independent design. Struct format mutation needs size/arity checks.
Path commonprefix is deprecated in Python 3.15 and cross-platform path semantics
need dedicated witnesses. zip_longest changes stream termination and may expose
unbounded inputs. JSON parse hooks have observable hook effects and are not merely
numeric type labels. These candidates remain possible, not rejected in principle.

## Verification

Use one scenario per test invocation, named rstest cases, explicit Arrange/Act/
Assert, and properties for signature/admission invariants. Public discovery tests
must demonstrate the missing behavior before production implementation. Execute
the emitted candidates in CPython, not handwritten surrogate transformations.
Cover Unicode Nd versus ASCII, superscripts/fractions, normalization axes, sharp
s, repeated prefixes, unequal iterator lengths, incomplete batches, malformed
byte streams, URL blanks/lists, floating boundaries and cancellation of analysis.
Verify positive behavior differences and preservation of effects/import scope.

Run Windows workspace tests, all-target/all-feature Clippy, formatting, generated
CLI reference checks, docs tests/site build and affected OKF validation. Re-run
runtime witnesses with CPython 3.15. The final commit must pass development CI.
Only one Cargo queue runs, with incremental compilation and dev/test debug info
disabled. Clean completed package outputs between major checks and all target
artifacts after validation. No work-record ZIP is created.

## Design self-reviews

1. Scope review: all six recommended groups have concrete IDs; additional API
   pairs have small witnesses. Deferred complex candidates have explicit reasons.
2. Identity review: imported functions use existing conservative family proofs;
   nominal method matching is documented rather than mislabeled type inference.
3. Evaluation review: dynamic truth conversion cannot be moved earlier; original
   operands remain outside generated scopes. Shared adapter semantics need tests.
4. Resource review: deferred sink owns replacement admission; no candidate matrix
   or whole-source rescanning is introduced by a per-call producer.
5. Integration review: registry, defaults, ranking, CLI/web reference and saved-plan
   rediscovery must all see the same IDs; no schema change is necessary.

Initial software-design-philosophy assessment: 8.75/10. Seven diagnostics have a
concrete design answer; interface documentation and ownership are specified, but
module simplicity remains provisional until the shared option adapter and domain
implementations are reviewed. Resolve that remaining diagnostic through the
implementation review rather than assigning an unearned final score.

Independent domain reviews subsequently passed all eight diagnostics after the
following corrections: domain-owned delimiter requirements, independent edit and
selection positions, and source-safe imported destination names. Signature and
truth-conversion decisions remain inside domain modules. The shared destination
resolver returns a borrowed proof that is rendered only after candidate
admission, while its existing string-returning callers retain their interfaces.
Whole-branch validation and its final review are recorded in the implementation
plan.

## Sources

- [Python built-in strings](https://docs.python.org/3/library/stdtypes.html#text-sequence-type-str)
- [Unicode data](https://docs.python.org/3/library/unicodedata.html)
- [Built-in map](https://docs.python.org/3/library/functions.html#map)
- [itertools](https://docs.python.org/3/library/itertools.html)
- [Codec policies](https://docs.python.org/3/library/codecs.html#error-handlers)
- [URL processing](https://docs.python.org/3/library/urllib.parse.html)
- [Mathematics](https://docs.python.org/3/library/math.html)
- [Statistics](https://docs.python.org/3/library/statistics.html)
- [Heap queues](https://docs.python.org/3/library/heapq.html)
- [Base64](https://docs.python.org/3/library/base64.html)
- [JSON](https://docs.python.org/3/library/json.html)
