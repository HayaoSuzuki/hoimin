# Issue #489: Case correspondence worksheet

The source-configurable premises and public observations were recorded before correspondence execution. Final evidence covers 41 cases: 39 strict matches and two model-only scope mismatches. A strict match requires original CPython 3.14 compilation, exact eligible/ineligible candidates through the real CLI plan subprocess, shared validator agreement and compilation of every individual mutant. Direct analyzer calls are additional observations. Compiler launch/version/deadline failures are infrastructure-error; rejected original source is invalid-fixture, never a successful empty candidate set.

Sources, sites, eligibility and byte-replacement expectations are defined once in `formal/HoiminOracle/ValidPythonAuditMain.lean`, generated into `corpus/valid-python.jsonl`. Source text supplies the configurable import, spelling, lexical-scope and syntax-position premises below; plan receives explicit operator and selection options. Finite witnesses use seed 489, two names, two visibility states, three exact key-alias pairs and at most three scope frames. The five layout/selection representatives include a real empty function, so symbol-miss exercises a valid symbol with zero applicable sites.

| Fixture | Producer / position | Configurable binding premise | Public observations | Mode |
| --- | --- | --- | --- | --- |
| annotation_generic_destination | annotation / annotation | generic-destination in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| match_key_unique | token / match-key | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| bom_first_line_unicode | token / expression | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline/mutant runtime observation | strict |
| annotation_import_source_shadow | annotation / annotation | source-module in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| except_tuple_ineligible | ast / except | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| selection_boundary | token / expression | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| token_expression | token / expression | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline/mutant runtime observation | strict |
| token_default | token / default | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| token_annotation | token / annotation | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| match_key_bool_integer | token / match-key | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| match_key_integer_bool | token / match-key | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| match_key_complex | token / match-key | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| match_value | token / match-value | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| match_guard | token / match-guard | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| unary_pattern | token / match-value | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| except_type | ast / except | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| except_star_type | ast / except-star | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| ast_expression | ast / expression | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline/mutant runtime observation | strict |
| ast_source_module | ast / expression | source-module in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| ast_destination_module | ast / expression | destination-module in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| ast_source_function | ast / expression | source-function in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| ast_destination_class | ast / expression | destination-class in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| ast_global | ast / expression | global in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline runtime observation | strict |
| ast_nonlocal | ast / expression | nonlocal in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline runtime observation | strict |
| ast_generic_destination | ast / expression | generic-destination in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| ast_generic_source | ast / expression | generic-source in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline runtime observation | strict |
| ast_generic_default | ast / default | generic-destination in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| ast_walrus | ast / expression | walrus in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline runtime observation | strict |
| ast_first_iterable | ast / first-iterable | walrus in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline/mutant runtime observation | strict |
| ast_comprehension_body | ast / comprehension-body | iteration-source in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline runtime observation | strict |
| ast_after_iteration | ast / expression | iteration-source in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline/mutant runtime observation | strict |
| ast_handler_target | ast / except | source-function in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| annotation_expression | annotation / annotation | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| annotation_builtin_source_boundary | annotation / annotation | source-module in emitted source | original/mutant compile; raw/plan metadata and validator; eligibility mismatch below | model-only |
| annotation_destination | annotation / annotation | destination-module in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| annotation_generic_source_boundary | annotation / annotation | generic-source in emitted source | original/mutant compile; raw/plan metadata and validator; eligibility mismatch below; baseline/mutant runtime observation | model-only |
| operator_expression | operator-import / expression | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline/mutant runtime observation | strict |
| operator_source_module | operator-import / expression | source-module in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| operator_source_function | operator-import / expression | source-function in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator | strict |
| operator_generic | operator-import / expression | generic-source in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline runtime observation | strict |
| operator_protocol_exception | operator-import / expression | unshadowed in emitted source | original/mutant compile; exact eligible/ineligible plan sites; byte/hash/location validator; baseline/mutant runtime observation | strict |

## Observed model-only mismatches

`annotation_builtin_source_boundary` supplies `from typing import Sequence; list = object; value: list[int]` on separate lines. The normalized source-shadow model predicts no candidate. Actual analyzer and public plan both emit `list[int] → Sequence[int]` at byte 49. The annotation resolver treats this builtin spelling differently from a shadowed imported source; this corpus does not generalize the imported-name rule to it.

`annotation_generic_source_boundary` supplies `from typing import Sequence` followed by `def subject[list](value: list[int]): return value`. The normalized generic-source model predicts no candidate, but analyzer and plan emit the same replacement at byte 53. Both original and mutant compile. The harness evaluates annotations: the original raises TypeError for the type parameter, while the mutant succeeds. This is a potential contract gap against the existing PEP 695 type-variable suppression design, not a fixed behavior or a strict correspondence result. Only these two fixture IDs may use model-only mode; unexpected mismatches in all other rows fail.

## Consumer limits and untested combinations

- Builtin pairs: BindingFlow resolves actual source/destination names; generic visibility maps a visible type parameter to a shadowed frame only in the modeled header/body site. This is not a full PEP695 interpreter.
- Operator imports: the fixture explicitly supplies a trusted import or one disqualifying shadow. The production operator-import resolver is independently conservative and module-wide; builtin flow-sensitive behavior is not assumed for it. A destination attribute like `operator.sub` is not a free destination variable. Unexercised directive/consumer pairs are emitted as uncovered, not borrowed from builtin results.
- Annotation imports: trusted typing imports and their source/destination shadows are checked through plan. Builtin-spelled annotation sources are a different resolver premise: the two mismatches below remain model-only. Existing AnnotationScope internal-fixture traces keep their original modes.
- Mapping keys: exact small integer/boolean/complex aliases only. Float rounding, NaN, dynamic attributes and arbitrary-precision complex conversion remain outside this model; existing individual Issue485 tests retain those responsibilities.
- Byte replacement: reuse CandidateSpan.replaceBytes and its prefix/replacement/suffix equation. Its legacy LF location interpretation is not reused for physical newline/Unicode/BOM assertions. Rust independently scans current source scalars and line terminators, and the shared production validator must agree.
- Layout: representative producers cover LF/CRLF/CR/mixed, BOM/no-final-newline, grouped expressions, Unicode/tab/comments and exception trailing commas. The adapter reports each missing axis pair, including impossible syntax pairs, as uncovered with no support claim; it does not silently infer eligibility there.
- Operator inventory: every canonical operator is registered as covered or deferred in `tests/fixtures/valid-python-operators.json`. Deferred entries retain existing focused tests and make no producer-crossing guarantee. New IDs fail registration until this scope decision is made.

CPython runtime checks deliberately distinguish an expected protocol ValueError (handled and observed by the fixture harness) from compiler failure. They do not classify every exception-producing mutant as invalid.
