# Behavioral mutation research: issues 1125–1131

## Decisions

The evaluation uses base main `42130ce3` on 2026-10-28. The same branch implements
seven restricted additions after comparison and reconsideration, enabled by default with
individual opt-out. No backward-compatibility implementation is added.

| Issue | Decision | Reason and revisit trigger |
| --- | --- | --- |
| [1125](https://github.com/HayaoSuzuki/hoimin/issues/1125) | Limited adoption: `continue_remove` | Same-iteration fallthrough is distinct from stopping later iterations. Tail continues can be equivalent; no claim that all survivors are actionable. |
| [1126](https://github.com/HayaoSuzuki/hoimin/issues/1126) | Limited adoption: `dataclass_factory_share` | Ordinary top-level dataclass fields with known empty builtin factories and a guarded class namespace. |
| [1127](https://github.com/HayaoSuzuki/hoimin/issues/1127) | Limited adoption: `sort_key_swap` | Proven builtin sorted with a two-read key lambda; grouping and evaluation counts retained, order intentionally exchanged. |
| [1128](https://github.com/HayaoSuzuki/hoimin/issues/1128) | Limited adoption: `lambda_capture_late` | Private getter/sentinel wrapper preserves explicit callback arguments while omitted captures become late-bound. |
| [1129](https://github.com/HayaoSuzuki/hoimin/issues/1129) | Limited adoption: `comparison_not_implemented` | Exposes reflected comparison delegation while retaining same-type behavior. Restrict to ordinary synchronous direct rich-comparison methods and proven builtin references. |
| [1130](https://github.com/HayaoSuzuki/hoimin/issues/1130) | Limited adoption: `exception_cause_remove` | Isolates a public diagnostic-cause requirement; named explicit causes only, replaced with None. A private-chain control correctly survives. |
| [1131](https://github.com/HayaoSuzuki/hoimin/issues/1131) | Limited adoption: `encoding_errors_ignore` | Trusted codecs aliases or nominal simple encode/decode methods with an explicit surrogateescape errors literal. |

The maintainer first requested proceeding with 1125/1129/1130, then authorized
implementation of the other four and specified the same PR. PR 1134 now implements
all seven with restricted applicability and individual default-on/opt-out control.
The [four-operator implementation](four-implementation.md) supersedes the temporary
open/undecided status in the [reconsideration](reconsideration.md). Original
comparison measurements below are preserved as historical evidence, not rerun
results for the expanded default set.

## Evidence and comparison

The [fixed methodology](methodology.md), [stdlib-only runner](experiment.py),
[CPython 3.14.9 results](results-python314.json) and
[CPython 3.15.0b1 beta results](results-python315.json) preserve independently
stated requirements, source, weak/strong assertions, all executed descriptors,
verdicts, counts, timing, revision, binary hash and cleanup. Both versions agree
on candidate counts and verdicts. These are independently written examples;
no book code or text is reproduced. They do not establish real-project efficacy,
universal equivalence, Linux/Windows results or measured memory improvement.

Each table cell is **pool / executed / useful / runtime-error candidates**.
Useful requires weak survival followed by an intended assertion failure.
Unexecuted candidates are not classified. Two candidates exposing the same
requirement are counted as two leads, not two independent test defects.

| Fixture | Nearest existing family | Full existing defaults | Proposed prototype |
| --- | --- | --- | --- |
| 1125 continue_rejection | 1 / 1 / 0 / 0 | 16 / 2 / 0 / 0 | 1 / 1 / 1 / 0 |
| 1125 continue_tail_equivalent | 1 / 1 / 1 / 0 | 5 / 2 / 1 / 1 | 1 / 1 / 0 / 0 |
| 1126 factory_isolation | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 1 / 1 / 1 / 0 |
| 1126 factory_read_only_control | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 1 / 1 / 0 / 0 |
| 1127 sort_priority | 2 / 2 / 2 / 0 | 6 / 2 / 0 / 1 | 1 / 1 / 1 / 0 |
| 1127 sort_correlated_control | 2 / 2 / 0 / 0 | 6 / 2 / 0 / 1 | 1 / 1 / 0 / 0 |
| 1128 lambda_capture | 0 / 0 / 0 / 0 | 6 / 2 / 1 / 1 | 1 / 1 / 1 / 0 |
| 1128 lambda_explicit_argument | 0 / 0 / 0 / 0 | 8 / 2 / 1 / 1 | 1 / 1 / 0 / 1 |
| 1129 comparison_delegation | 0 / 0 / 0 / 0 | 1 / 1 / 0 / 0 | 1 / 1 / 1 / 0 |
| 1129 comparison_no_peer_control | 0 / 0 / 0 / 0 | 1 / 1 / 0 / 0 | 1 / 1 / 0 / 0 |
| 1130 cause_diagnostic | 1 / 1 / 0 / 1 | 5 / 2 / 0 / 1 | 1 / 1 / 1 / 0 |
| 1130 cause_private_control | 1 / 1 / 0 / 1 | 5 / 2 / 0 / 1 | 1 / 1 / 0 / 0 |
| 1131 encoding_roundtrip | 4 / 2 / 0 / 2 | 4 / 2 / 0 / 2 | 2 / 2 / 2 / 0 |
| 1131 encoding_ascii_control | 4 / 2 / 0 / 1 | 4 / 2 / 0 / 1 | 2 / 2 / 0 / 0 |

The continue comparator is `break_continue`; statement deletion remains related
but changes standalone calls, not control transfer. For the remaining direct
families see `INCUMBENTS` in the runner. The full-default prefix is a separate
control and is not a census of all default efficacy. Empty pools consume no
execution budget. Existing order is public plan order, prototype order is AST
walk order. Sources are ASCII by construction; production UTF-8 handling is
tested separately.

All 28 original weak/strong checks pass per interpreter. Syntax/startup and
runtime failures are not credited as useful behavior checks. Notably the
explicit-argument lambda control raises TypeError after removing its parameter,
and string-empty encoding mutants fail through lookup/runtime errors rather
than the intended losslessness assertion. Read-only factories, correlated sort
keys, tail continues, unsupported peers, private causes and ASCII-only payloads
illustrate contract/domain-dependent survival.

## Applicability and name resolution

- Continue removal has no symbol identity requirement. It preserves syntax in
  finally/with/loop-else/nested loops but can activate later side effects. At most
  one candidate per continue; no claim that a tail continue is useful.
- Dataclass sharing must preserve class construction and avoid `field(default=[])`.
  Only builtin list/dict/set factory values could be considered, with proven
  dataclasses `field`/decorator identities, alias/rebinding/shadowing handling,
  and exclusion of arbitrary factories. Existing `CallableImports` rejects class
  lookups to avoid metaclass-supplied names; do not bypass that safety merely to
  make this example eligible. A lambda-local shared parameter can avoid leaked
  module names but does not solve callable identity or factory timing.
- Sort priority swapping would require an identified sorting API and a two-part
  key. Swapping calls, properties or custom subscripts changes evaluation order
  and can execute user code; no general purity inference is available. Existing
  `return_tuple_swap` does not cover this lambda, but container deletion already
  finds the requirement here. This overlap does not prove universal redundancy.
- The original parameter-removal prototype needs the same-name outer loop binding,
  nested-scope/comprehension rules and a zero-argument call contract. The revised
  private-sentinel/getter method preserves explicit overrides and does not need
  that zero-argument-only restriction; see the reconsideration. Defaults evaluated
  early versus free variables resolved late is the intended difference; losing
  valid positional/keyword calls is an additional, potentially misleading change.
- Comparison replacement checks builtin `NotImplemented`, excludes uncertain or
  shadowed names (including wildcard/dynamic namespace changes), non-methods,
  arithmetic methods, decorators, async and generator functions and nested-function
  returns. All six rich-comparison names are recognized. No operand-type inference
  or runtime monkey-patching guarantee is claimed.
- Cause suppression replaces only a Name in explicit `from cause`. It retains
  parentheses/comments and the original raised error expression; calls, attributes,
  compound causes and existing `from None` are excluded. `__cause__` becomes None;
  implicit `__context__` still exists, with `__suppress_context__` true. Only a
  diagnostic API requirement makes a cause assertion appropriate.
- Encoding-policy loss needs a recognized decode/encode API and an explicit
  losslessness contract. Do not mutate unrelated `errors` arguments or arbitrary
  string literals. Positional signature differences, aliasing and receiver type
  uncertainty were initial implementation concerns. Trusted codecs calls or an
  explicitly nominal method rule are reconsidered below, without target imports
  or a universal receiver-type proof requirement.

## Implemented contract and verification

The first three additions brought the catalog to 97 IDs and 78 defaults. Those IDs were appended to
`MutationOperator`, independently selectable/excludable and assigned existing
ranking categories. No selector family, schema, runtime manager, report or
scheduler contract changes. No dependency is added. Continue and cause producers
reuse the existing statement collector; comparison methods reuse the existing
own-function scan and name-resolution index. All share candidate selection,
payload accounting, bounded admission and cancellation. Only tiny replacements
are allocated; no source-wide candidate copy or new state store is introduced.

Analyzer tests cover positive/negative scope, shadowing, trivia/UTF-8, limits,
selection and cancellation. Actual CLI plan/verify cases require original
baseline success, weak survival and strong kills, complete counts, no errors and
unchanged originals. The complete operator opt-out test now includes these IDs.
See [review and validation](self-review.md) for actual final command results and
remaining environment limits. Formal proofs are retained; these three producers
are explicitly deferred from the original Lean cross-producer corpus, with
separate executable coverage rather than a claim of new formal proof.

The subsequent [four-operator implementation](four-implementation.md) brings the
current catalog to 101 IDs and 82 defaults in the same PR.

## Reproduce

Build a binary from the recorded base revision, then run from the repository root:

```sh
python3 -B docs/superpowers/reports/issues-1125-1131/experiment.py \
  --hoimin target/debug/hoimin --output /tmp/hoimin-1125-research.json
```

The JSON records the actual interpreter. Replaying with the implemented branch
changes the full-default pool; do not overwrite the base evidence as if it were
identical. Public product tests can be rerun with
`cargo test --locked -p hoimin-cli --test default_operator_selection` and optional
`HOIMIN_OPERATOR_TEST_PYTHON` pointing to another supported interpreter. The beta
run is additional evidence and is not advertised as a stable Python release.
