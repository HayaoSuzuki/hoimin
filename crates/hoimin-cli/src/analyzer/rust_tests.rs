use super::{AnalyzeRequest, analyze_source};
use crate::analyzer::AnalyzerDiagnosticCode;
use camino::Utf8Path;
use hoimin_core::{
    ByteSpan, LineRange, MutationOperator, MutationOperatorSelection, MutationProfile,
};
use proptest::prelude::*;

fn analyze(source: &str) -> super::AnalyzerOutput {
    analyze_with(Utf8Path::new("pkg/sample.py"), &[], &[], 10_000, source)
}

fn analyze_types(source: &str) -> super::AnalyzerOutput {
    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::TypeNullableRemove,
        MutationOperator::TypeNullableAdd,
        MutationOperator::TypeListSequence,
        MutationOperator::TypeSetAbstractSet,
        MutationOperator::TypeMapping,
        MutationOperator::TypeIterableIterator,
        MutationOperator::TypeSequenceIterable,
    ] {
        operators.include(operator);
    }
    analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    )
}
fn analyze_with(
    path: &Utf8Path,
    lines: &[LineRange],
    symbols: &[String],
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    let operators = MutationOperatorSelection::default();
    analyze_source(
        &AnalyzeRequest {
            path,
            lines,
            symbols,
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates,
        },
        source,
    )
}

#[test]
fn omits_candidates_for_unselected_operators() {
    let mut operators = MutationOperatorSelection::default();
    operators.exclude(MutationOperator::BinaryAddSub);
    let output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        "result = left + right\n",
    );
    assert!(output.candidates.is_empty());
}

#[test]
fn type_annotations_emit_supported_candidates_in_source_order() {
    let source = "from typing import AbstractSet, Iterator, Mapping, Optional\nimport typing as t\nfrom collections.abc import Iterable, Sequence\n\nmodule_value: Optional[int]\n\nclass Model:\n    names: list[str]\n\n    def convert(self, name: str | None, age: t.Optional[int]) -> set[str]:\n        mapping: dict[str, int] = {}\n        values: Iterable[str] = []\n        ordered: Sequence[str] = []\n        return set()\n";
    let output = analyze_types(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.starts_with("type_"))
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            ("Optional[int]", "int", "type_nullable_remove"),
            ("list[str]", "Sequence[str]", "type_list_sequence"),
            ("list[str]", "list[str] | None", "type_nullable_add"),
            ("str | None", "str", "type_nullable_remove"),
            ("t.Optional[int]", "int", "type_nullable_remove"),
            ("set[str]", "set[str] | None", "type_nullable_add"),
            ("set[str]", "AbstractSet[str]", "type_set_abstract_set"),
            ("dict[str, int]", "Mapping[str, int]", "type_dict_mapping"),
            (
                "dict[str, int]",
                "dict[str, int] | None",
                "type_nullable_add"
            ),
            ("Iterable[str]", "Iterator[str]", "type_iterable_iterator"),
            ("Iterable[str]", "Iterable[str] | None", "type_nullable_add"),
            ("Sequence[str]", "list[str]", "type_list_sequence"),
            ("Sequence[str]", "Sequence[str] | None", "type_nullable_add"),
            ("Sequence[str]", "Iterable[str]", "type_sequence_iterable"),
        ]
    );
}

#[test]
fn type_annotations_ignore_quoted_and_unrecognized_forms() {
    let source = "from typing import Annotated, Any, Callable, Optional, TypeVar\nfrom local import Optional as LocalOptional\n\nT = TypeVar('T')\nclass Sequence: pass\nquoted: 'Optional[int]'\nannotated: Annotated[list[str], 'meta']\nany_value: Any\ncallback: Callable[[str], int]\ngeneric: T\nuser_sequence: Sequence[str]\nlocal_optional: LocalOptional[int]\n";
    let output = analyze_types(source);
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("type_"))
    );
}

#[test]
fn type_annotations_reject_disallowed_nested_types_and_object_nullable_addition() {
    let source = "from typing import Annotated, Any, Callable, Literal, Optional, Protocol, TypeVar\nimport typing as t\n\nT = TypeVar('T')\nAlias = str\nobject_value: object\noptional_object: Optional[object]\nobject_union: object | None\nobject_list: list[object]\noptional_any: Optional[Any]\ncallable_value: Callable[[str], int] | None\noptional_type_var: Optional[T]\naliased: Alias | None\nitems: list[Any]\nannotated: Optional[Annotated[list[str], 'meta']]\nliteral: Literal[\"x\"] | None\nqualified_literal: t.Literal[\"x\"] | None\nprotocol: Protocol | None\nqualified_protocol: t.Protocol | None\n";
    let output = analyze_types(source);
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("type_"))
    );
}

#[test]
fn type_annotations_resolve_unaliased_and_aliased_collections_abc_modules() {
    let source = "import collections.abc\nimport collections.abc as cabc\n\nfirst: collections.abc.Iterable[str]\nsecond: collections.abc.Sequence[str]\nthird: cabc.Iterable[str]\nfourth: cabc.Sequence[str]\n";
    let output = analyze_types(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.operator.starts_with("type_") && candidate.operator != "type_list_sequence"
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            (
                "collections.abc.Iterable[str]",
                "collections.abc.Iterator[str]",
                "type_iterable_iterator"
            ),
            (
                "collections.abc.Iterable[str]",
                "collections.abc.Iterable[str] | None",
                "type_nullable_add"
            ),
            (
                "collections.abc.Sequence[str]",
                "collections.abc.Sequence[str] | None",
                "type_nullable_add"
            ),
            (
                "collections.abc.Sequence[str]",
                "collections.abc.Iterable[str]",
                "type_sequence_iterable"
            ),
            (
                "cabc.Iterable[str]",
                "cabc.Iterator[str]",
                "type_iterable_iterator"
            ),
            (
                "cabc.Iterable[str]",
                "cabc.Iterable[str] | None",
                "type_nullable_add"
            ),
            (
                "cabc.Sequence[str]",
                "cabc.Sequence[str] | None",
                "type_nullable_add"
            ),
            (
                "cabc.Sequence[str]",
                "cabc.Iterable[str]",
                "type_sequence_iterable"
            ),
        ]
    );
}
#[test]
fn type_annotations_emit_reverse_collection_and_iterable_mutations() {
    let source = "from typing import AbstractSet, Iterable, Iterator, Mapping, Sequence\n\nforward_list: list[str]\nreverse_sequence: Sequence[str]\nforward_set: set[str]\nreverse_set: AbstractSet[str]\nforward_dict: dict[str, int]\nreverse_mapping: Mapping[str, int]\nforward_iterable: Iterable[str]\nreverse_iterator: Iterator[str]\n";
    let output = analyze_types(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "type_list_sequence"
                    | "type_set_abstract_set"
                    | "type_dict_mapping"
                    | "type_iterable_iterator"
                    | "type_sequence_iterable"
            )
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            ("list[str]", "Sequence[str]", "type_list_sequence"),
            ("Sequence[str]", "list[str]", "type_list_sequence"),
            ("Sequence[str]", "Iterable[str]", "type_sequence_iterable"),
            ("set[str]", "AbstractSet[str]", "type_set_abstract_set"),
            ("AbstractSet[str]", "set[str]", "type_set_abstract_set"),
            ("dict[str, int]", "Mapping[str, int]", "type_dict_mapping"),
            ("Mapping[str, int]", "dict[str, int]", "type_dict_mapping"),
            ("Iterable[str]", "Iterator[str]", "type_iterable_iterator"),
            ("Iterator[str]", "Iterable[str]", "type_iterable_iterator"),
        ]
    );
}

#[test]
fn type_annotations_require_resolvable_abstract_destinations() {
    let missing = analyze_types("value: list[str]\n");
    assert!(
        missing
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "type_list_sequence")
    );
    let direct = analyze_types("from typing import Sequence as Seq\nvalue: list[str]\n");
    assert!(direct.candidates.iter().any(|candidate| (
        candidate.original.as_str(),
        candidate.replacement.as_str(),
        candidate.operator.as_str()
    ) == (
        "list[str]",
        "Seq[str]",
        "type_list_sequence"
    )));
    let qualified = analyze_types("import typing as t\nvalue: list[str]\n");
    assert!(qualified.candidates.iter().any(|candidate| (
        candidate.original.as_str(),
        candidate.replacement.as_str(),
        candidate.operator.as_str()
    ) == (
        "list[str]",
        "t.Sequence[str]",
        "type_list_sequence"
    )));
}
#[test]
fn type_annotations_respect_line_and_symbol_filters() {
    let source = "from typing import AbstractSet, Mapping, Sequence\n\nclass Model:\n    field: list[str]\n\ndef convert(value: str) -> set[str]:\n    local: dict[str, int] = {}\n    return set()\n";
    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::TypeNullableAdd,
        MutationOperator::TypeListSequence,
        MutationOperator::TypeSetAbstractSet,
        MutationOperator::TypeMapping,
    ] {
        operators.include(operator);
    }
    let line_output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[LineRange { start: 4, end: 4 }],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert_eq!(
        line_output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.starts_with("type_"))
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec!["list[str]", "list[str]"]
    );
    let symbol_output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &["pkg.sample:convert".to_owned()],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert_eq!(
        symbol_output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.starts_with("type_"))
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec![
            "str",
            "set[str]",
            "set[str]",
            "dict[str, int]",
            "dict[str, int]"
        ]
    );
}
proptest! {
    #[test]
    fn arbitrary_python_input_has_ordered_in_bounds_candidates(source in ".{0,4096}") {
        let output = analyze(&source);
        let mut previous_end = 0;
        for candidate in output.candidates {
            let start = usize::try_from(candidate.span.start).expect("span start fits usize");
            let length = usize::try_from(candidate.span.length).expect("span length fits usize");
            let end = start.checked_add(length).expect("candidate span does not overflow");
            prop_assert!(previous_end <= start);
            prop_assert!(end <= source.len());
            prop_assert_eq!(&source[start..end], candidate.original);
            previous_end = end;
        }
    }
}

#[test]
fn emits_the_mvp_operator_replacements_in_source_order() {
    let source = "def f(a, b, xs, flag):\n    value = a == b and a not in xs and a is not b\n    value += a * b // 2 % 2\n    return not flag, +a, -b, True, False\n";
    let output = analyze(source);
    let pairs: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("==", "!=", "compare_eq_ne"),
            ("and", "or", "boolean_and_or"),
            ("not in", "in", "membership"),
            ("and", "or", "boolean_and_or"),
            ("is not", "is", "identity"),
            ("+=", "-=", "augmented_add_sub"),
            ("*", "/", "binary_mul_div"),
            ("//", "%", "binary_floor_mod"),
            ("%", "//", "binary_floor_mod"),
            ("not flag", "flag", "remove_not"),
            ("+", "-", "unary_sign"),
            ("-", "+", "unary_sign"),
            ("True", "False", "boolean_literal"),
            ("False", "True", "boolean_literal")
        ]
    );
}

#[test]
fn emits_complete_candidate_records_in_source_order() {
    let source =
        "top = left == right\ndef decide(flag, value):\n    return not flag or value + 1\n";
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.path.as_str(),
                candidate.span,
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
                candidate.line,
                candidate.column,
                candidate.symbol.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        candidates,
        vec![
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 11,
                    length: 2,
                },
                "==",
                "!=",
                "compare_eq_ne",
                1,
                11,
                None,
            ),
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 56,
                    length: 8,
                },
                "not flag",
                "flag",
                "remove_not",
                3,
                11,
                Some("decide"),
            ),
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 65,
                    length: 2,
                },
                "or",
                "and",
                "boolean_and_or",
                3,
                20,
                Some("decide"),
            ),
            (
                "pkg/sample.py",
                ByteSpan {
                    start: 74,
                    length: 1,
                },
                "+",
                "-",
                "binary_add_sub",
                3,
                29,
                Some("decide"),
            ),
        ]
    );
}
#[test]
fn removes_not_across_its_complete_ast_operand_range() {
    let output = analyze("result = not (left == right and ready)\n");
    let candidate = output
        .candidates
        .iter()
        .find(|candidate| candidate.operator == "remove_not")
        .expect("not candidate");
    assert_eq!(candidate.original, "not (left == right and ready)");
    assert_eq!(candidate.replacement, "(left == right and ready)");
    assert_eq!(candidate.span.length, candidate.original.len() as u64);
}

#[test]
fn classifies_unary_signs_from_the_ast_not_preceding_tokens() {
    let output = analyze("result = left * -right + +other\n");
    let signs: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.original.as_str(), "+" | "-"))
        .map(|candidate| (candidate.original.as_str(), candidate.operator.as_str()))
        .collect();
    assert_eq!(
        signs,
        vec![
            ("-", "unary_sign"),
            ("+", "binary_add_sub"),
            ("+", "unary_sign"),
        ]
    );
}

#[test]
fn assigns_ast_scopes_to_decorators_async_definitions_and_not_following_code() {
    let source = "class Outer:\n    @decorator(left == right)\n    async def method(self):\n        return not (ready and enabled)\nafter = left == right\n";
    let output = analyze(source);
    let observed: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.symbol.as_deref(),
                candidate.line,
            )
        })
        .collect();
    assert_eq!(
        observed,
        vec![
            ("==", Some("Outer.method"), 2),
            ("not (ready and enabled)", Some("Outer.method"), 4),
            ("and", Some("Outer.method"), 4),
            ("==", None, 5),
        ]
    );
}

#[test]
fn emits_each_remaining_mvp_operator() {
    let source = "def f(a, b, xs):\n    a != b\n    a < b\n    a <= b\n    a > b\n    a >= b\n    a in xs\n    a is b\n    a or b\n    a + b\n    a - b\n    a / b\n    while a:\n        break\n        continue\n";
    let output = analyze(source);
    let pairs: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("!=", "=="),
            ("<", "<="),
            ("<=", "<"),
            (">", ">="),
            (">=", ">"),
            ("in", "not in"),
            ("is", "is not"),
            ("or", "and"),
            ("+", "-"),
            ("-", "+"),
            ("/", "*"),
            ("break", "continue"),
            ("continue", "break")
        ]
    );
}

#[test]
fn reports_unicode_code_point_columns_without_changing_byte_spans() {
    let source = "# 日本語\n値 = left == right  # adjacent comment\n";
    let candidate = analyze(source).candidates.remove(0);
    assert_eq!(
        candidate.span,
        ByteSpan {
            start: 23,
            length: 2
        }
    );
    assert_eq!((candidate.line, candidate.column), (2, 9));
    let mut changed = source.as_bytes().to_vec();
    let start = usize::try_from(candidate.span.start).unwrap();
    changed.splice(start..start + 2, candidate.replacement.bytes());
    assert_eq!(
        String::from_utf8(changed).unwrap(),
        "# 日本語\n値 = left != right  # adjacent comment\n"
    );
}

#[test]
fn assigns_nested_definition_symbols_and_init_module_selectors() {
    let source = "class Outer:\n    def method(self, left, right):\n        return left == right\n";
    let output = analyze_with(
        Utf8Path::new("pkg/sub/__init__.py"),
        &[],
        &["sub:Outer.method".to_owned()],
        10_000,
        source,
    );
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("Outer.method"));
}

#[test]
fn filters_candidates_by_line_or_symbol() {
    let source = "def first(a, b):\n    return a == b\n\ndef second(a, b):\n    return a == b\n";
    let output = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 2, end: 2 }],
        &["pkg.sample:second".to_owned()],
        10_000,
        source,
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| candidate.symbol.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("first"), Some("second")]
    );
}

#[test]
fn reports_invalid_syntax_without_candidates() {
    let output = analyze("def broken(:\n");
    assert!(output.candidates.is_empty());
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::InvalidSyntax
    );
    assert!(!output.truncated);
}

#[test]
fn truncates_after_selected_candidates_and_emits_limit_diagnostic() {
    let output = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        2,
        "a == b and c == d\n",
    );
    assert_eq!(output.candidates.len(), 2);
    assert!(output.truncated);
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
}
