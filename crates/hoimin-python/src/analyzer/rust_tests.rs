use super::{
    AnalyzeRequest, AnalyzerCandidate, AnnotationSiteTestSnapshot, BindingFlowTestMutation,
    BindingFlowTestSnapshot, CandidatePrefix, LineIndex, NameResolutionTestSnapshot,
    analyze_source, analyze_source_cancellable, annotation_retention_stats,
    annotation_site_test_snapshot, binding_flow_handler_exit_snapshot,
    binding_flow_handler_exit_snapshot_with_mutation, binding_flow_loop_head_snapshot,
    binding_flow_marker_snapshot, binding_flow_marker_snapshot_with_mutation,
    binding_flow_test_snapshot, candidate_work_stats, name_resolution_test_snapshot,
    reset_annotation_retention_stats, reset_candidate_work_stats,
};
use crate::AnalyzerDiagnosticCode;
use camino::Utf8Path;
use hoimin_core::{
    ByteSpan, LineRange, MutationOperator, MutationOperatorSelection, MutationProfile,
};
use proptest::prelude::*;
use ruff_python_parser::parse_module;
use std::fmt::Write as _;

#[path = "rust/base_order_tests.rs"]
mod base_order_tests;

#[path = "rust/inheritance_tests.rs"]
mod inheritance_tests;

#[path = "rust/slice_division_tests.rs"]
mod slice_division_tests;

#[path = "rust/behavioral_research_tests.rs"]
mod behavioral_research_tests;

#[path = "rust/dataclass_factory_tests.rs"]
mod dataclass_factory_tests;
#[path = "rust/followup_call_tests.rs"]
mod followup_call_tests;
#[path = "rust/lambda_capture_tests.rs"]
mod lambda_capture_tests;

const BINDING_FLOW_CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/binding-flow-joins.jsonl");

#[test]
fn elif_named_binding_preserves_local_builtin_shadowing() {
    let source = "def selected(flag, values, replacement):\n    if flag:\n        pass\n    elif not (list := replacement):\n        pass\n    return list(values)\n";

    let snapshot = name_resolution_test_snapshot(source, "list(values)", "list").unwrap();

    assert_eq!(snapshot.resolution, "shadowed");
    assert_eq!(snapshot.start, source.find("list(values)").unwrap());
}

proptest! {
    #[test]
    fn elif_not_forms_analyze_without_unselected_candidates(
        clauses in 0usize..=32,
        first_not in prop::sample::select(vec!["", "not "]),
        elif_not in prop::sample::select(vec!["", "not "]),
        nested in any::<bool>(),
    ) {
        let mut source = format!("if {first_not}first:\n    pass\n");
        for clause in 0..clauses {
            writeln!(source, "elif {elif_not}condition{clause}:\n    pass").unwrap();
        }
        if nested {
            source.push_str("else:\n    if inner:\n        pass\n    elif not other:\n        pass\n");
        }

        let output =
            analyze_with_only_operator(&source, MutationOperator::BooleanLiteral);

        prop_assert!(output.candidates.is_empty(), "{}", source);
        prop_assert!(output.diagnostics.is_empty(), "{}", source);
        prop_assert!(!output.truncated, "{}", source);
    }
}

#[test]
fn string_segments_respect_candidate_caps_and_target_selection() {
    let source =
        "def first(x):\n return f'prefix:{x}:suffix'\ndef second(x):\n return 'prefix' 'suffix'\n";

    let output = analyze_with_only_operator(source, MutationOperator::StringSegmentEmpty);

    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
    assert_eq!(output.candidates[0].original, "prefix:");
    let mut operators = MutationOperatorSelection::default();
    for operator in operators
        .names()
        .iter()
        .map(|name| MutationOperator::from_name(name).unwrap())
        .filter(|operator| *operator != MutationOperator::StringSegmentEmpty)
    {
        operators.exclude(operator);
    }
    let lines = [LineRange { start: 4, end: 4 }];
    let symbols = ["second".to_owned()];
    let request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &lines,
        symbols: &symbols,
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let selected = analyze_source(&request, source);
    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(selected.candidates[0].original, "'prefix'");
    assert!(!selected.truncated);
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
}
const ANNOTATION_SCOPE_CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/annotation-scope-correspondence.jsonl");
const EXCEPTION_MATCH_BINDING_CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../formal/HoiminOracle/corpus/exception-match-binding-correspondence.jsonl"
));
const BOUNDED_DISCOVERY_CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/bounded-candidate-discovery.jsonl");

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedCandidateInput {
    identity: u64,
    order_key: u64,
    producer: String,
    eligible: bool,
    emission_index: u64,
    path: String,
    span_start: u64,
    span_length: u64,
    original: String,
    replacement: String,
    operator: String,
    line: u32,
    column: u32,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedDiscoveryCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    limit: u64,
    token: Vec<BoundedCandidateInput>,
    ast: Vec<BoundedCandidateInput>,
    annotation: Vec<BoundedCandidateInput>,
    targets: Vec<Vec<BoundedCandidateInput>>,
    expected_identities: Vec<u64>,
    expected_truncated: bool,
    expected_sequences: Vec<u64>,
    expected_targets_read: u64,
    expected_spool_finished: bool,
    expected_candidate_limit_diagnostic: bool,
    expected_exit_code: u64,
}

fn bounded_discovery_case(id: &str) -> BoundedDiscoveryCase {
    BOUNDED_DISCOVERY_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<BoundedDiscoveryCase>(line).expect("valid Lean row"))
        .find(|item| item.id == id)
        .unwrap_or_else(
            // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
            #[cfg_attr(coverage_nightly, coverage(off))]
            || panic!("missing Lean bounded-discovery case {id}"),
        )
}

#[derive(serde::Deserialize)]
struct BindingFlowCorpusCase {
    id: String,
    mode: String,
    source: String,
    expected_fallthrough: Vec<Vec<String>>,
    expected_breaks: Vec<Vec<String>>,
    expected_continues: Vec<Vec<String>>,
    expected_terminates: Vec<Vec<String>>,
    expected_loop_head: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnotationScopeCorpusCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    observation_kind: String,
    source: String,
    marker: String,
    expected_facts: Vec<String>,
    expected_symbol: Option<String>,
    expected_scope: Option<String>,
    expected_resolution: Option<String>,
    expected_operator: Option<String>,
    expected_original: Option<String>,
    expected_replacement: Option<String>,
    expected_present: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExceptionMatchBindingCorpusCase {
    schema: u64,
    id: String,
    mode: String,
    family: String,
    observation_kind: String,
    source: String,
    marker: String,
    name: String,
    expected_facts: Vec<String>,
    expected_resolution: Option<String>,
    expected_exit_category: Option<String>,
    expected_present: bool,
    operator: Option<String>,
    original: Option<String>,
    replacement: Option<String>,
    symbol: Option<String>,
}

fn analyze(source: &str) -> super::AnalyzerOutput {
    analyze_with(Utf8Path::new("pkg/sample.py"), &[], &[], 10_000, source)
}

fn analyze_with_only_operator(
    source: &str,
    selected_operator: MutationOperator,
) -> super::AnalyzerOutput {
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        let operator = MutationOperatorSelection::parse_selector(&name).unwrap()[0];
        if operator != selected_operator {
            operators.exclude(operator);
        }
    }
    operators.include(selected_operator);
    analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/annotations.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 1,
        },
        source,
    )
}

#[test]
fn unselected_collection_literals_build_no_large_candidate_strings() {
    let depth = 64;
    let source = format!(
        "data = {}\"{}\"{}\nx = 1 + 2\n",
        "[".repeat(depth),
        "a".repeat(64_000),
        "]".repeat(depth)
    );
    reset_candidate_work_stats();

    let output = analyze_with_only_operator(&source, MutationOperator::BinaryAddSub);

    assert_eq!(candidate_work_stats(), (0, 1));
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].operator.as_str(), "binary_add_sub");
    assert_eq!(output.candidates[0].original, "+");
    assert!(!output.truncated);
}

#[test]
fn selected_collection_literal_still_builds_its_candidate() {
    reset_candidate_work_stats();

    let output = analyze("data = [item]\n");

    assert_eq!(candidate_work_stats().0, 1);
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "collection_list_tuple"
            && candidate.original == "[item]"
            && candidate.replacement == "(item,)"
    }));
}

#[test]
fn line_index_preserves_out_of_order_and_repeated_queries() {
    let source = "\u{feff}aβ😀\r\nxy\u{301}z\r最後\n";
    let index = LineIndex::new(source);
    let offsets = [24, 3, 16, 6, 18, 4, 4];

    let actual = offsets.map(|offset| index.line_and_column(offset));

    assert_eq!(
        actual,
        [(3, 2), (1, 0), (2, 3), (1, 2), (3, 0), (1, 1), (1, 1)]
    );
}

#[rstest::rstest]
#[case::value_24(24, (3, 2))]
#[case::value_3(3, (1, 0))]
#[case::value_16(16, (2, 3))]
#[case::value_6(6, (1, 2))]
#[case::value_18(18, (3, 0))]
#[case::value_4(4, (1, 1))]
fn line_index_answers_out_of_order_ascii_and_unicode_offsets(
    #[case] offset: usize,
    #[case] expected: (u32, u32),
) {
    let source = "\u{feff}aβ😀\r\nxy\u{301}z\r最後\n";
    let index = LineIndex::new(source);

    assert_eq!(index.line_and_column(offset), expected);
}

#[rstest::rstest]
#[case::value_8(8)]
#[case::value_32(32)]
#[case::value_128(128)]
fn straight_line_import_transfer_preserves_selected_candidates_and_skips_unselected(
    #[case] aliases: usize,
    #[values(0, 1, 2)] scenario_index: usize,
    #[values(0, 1)] nested_scenario_index: usize,
) {
    let annotations = [8, 32, 128].into_iter().nth(scenario_index).unwrap();

    let mut source = String::from("from typing import Sequence\n");
    for index in 0..aliases {
        writeln!(source, "import typing as t{index}").unwrap();
    }
    source.push_str(&"x: int\n".repeat(annotations));
    source.push_str("value: list[int]\n");

    let (operator, expected_candidates, expected_calls, expected_entries, expected_retention) = [
        (MutationOperator::BooleanLiteral, vec![], 0, 0, (0, 0)),
        (
            MutationOperator::TypeListSequence,
            vec![("list[int]", "Sequence[int]")],
            1,
            aliases + 1,
            (annotations + 1, 0),
        ),
    ]
    .into_iter()
    .nth(nested_scenario_index)
    .unwrap();

    super::IMPORT_CLONE_CALLS.set(0);
    super::IMPORT_CLONE_ENTRIES.set(0);
    reset_annotation_retention_stats();

    let output = analyze_with_only_operator(&source, operator);

    assert_eq!(output.diagnostics, Vec::new());
    assert!(!output.truncated);
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
            .collect::<Vec<_>>(),
        expected_candidates
    );
    assert_eq!(super::IMPORT_CLONE_CALLS.get(), expected_calls);
    assert_eq!(super::IMPORT_CLONE_ENTRIES.get(), expected_entries);
    assert_eq!(annotation_retention_stats(), expected_retention);
}

#[test]
fn annotation_import_snapshots_are_not_retained_per_site() {
    let count = 256;
    let imports = (0..count)
        .map(|index| format!("Optional as T{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let annotations = (0..count).fold(String::new(), |mut output, index| {
        writeln!(output, "x{index}: int").unwrap();
        output
    });
    let source = format!("from typing import {imports}\n{annotations}");

    reset_annotation_retention_stats();
    let unselected = analyze_with_only_operator(&source, MutationOperator::BooleanLiteral);

    assert_eq!(unselected.candidates, Vec::new());
    assert_eq!(annotation_retention_stats(), (0, 0));

    reset_annotation_retention_stats();
    let selected = analyze_with_only_operator(&source, MutationOperator::TypeNullableAdd);
    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(annotation_retention_stats(), (count, 0));
}

// These literal expectations detect missing callable families, incorrect pairs, and
// alias spellings independently of the production catalog.
type OperatorFunctionPair = (
    &'static str,
    &'static str,
    Option<(&'static str, &'static str)>,
);
const OPERATOR_FUNCTION_PAIRS: &[OperatorFunctionPair] = &[
    ("eq", "ne", Some(("__eq__", "__ne__"))),
    ("ne", "eq", Some(("__ne__", "__eq__"))),
    ("lt", "le", Some(("__lt__", "__le__"))),
    ("le", "lt", Some(("__le__", "__lt__"))),
    ("gt", "ge", Some(("__gt__", "__ge__"))),
    ("ge", "gt", Some(("__ge__", "__gt__"))),
    ("add", "sub", Some(("__add__", "__sub__"))),
    ("sub", "add", Some(("__sub__", "__add__"))),
    ("mul", "truediv", Some(("__mul__", "__truediv__"))),
    ("truediv", "mul", Some(("__truediv__", "__mul__"))),
    ("floordiv", "mod", Some(("__floordiv__", "__mod__"))),
    ("mod", "floordiv", Some(("__mod__", "__floordiv__"))),
    ("pow", "mul", Some(("__pow__", "__mul__"))),
    ("matmul", "mul", Some(("__matmul__", "__mul__"))),
    ("and_", "or_", Some(("__and__", "__or__"))),
    ("or_", "and_", Some(("__or__", "__and__"))),
    ("lshift", "rshift", Some(("__lshift__", "__rshift__"))),
    ("rshift", "lshift", Some(("__rshift__", "__lshift__"))),
    ("xor", "and_", Some(("__xor__", "__and__"))),
    ("neg", "pos", Some(("__neg__", "__pos__"))),
    ("pos", "neg", Some(("__pos__", "__neg__"))),
    ("abs", "neg", Some(("__abs__", "__neg__"))),
    ("index", "pos", Some(("__index__", "__pos__"))),
    ("inv", "pos", Some(("__inv__", "__pos__"))),
    ("invert", "pos", Some(("__invert__", "__pos__"))),
    ("not_", "truth", Some(("__not__", "truth"))),
    ("truth", "not_", None),
    ("is_", "is_not", None),
    ("is_not", "is_", None),
    ("is_none", "is_not_none", None),
    ("is_not_none", "is_none", None),
    ("iadd", "isub", Some(("__iadd__", "__isub__"))),
    ("isub", "iadd", Some(("__isub__", "__iadd__"))),
    ("imul", "itruediv", Some(("__imul__", "__itruediv__"))),
    ("itruediv", "imul", Some(("__itruediv__", "__imul__"))),
    ("ifloordiv", "imod", Some(("__ifloordiv__", "__imod__"))),
    ("imod", "ifloordiv", Some(("__imod__", "__ifloordiv__"))),
    ("ipow", "imul", Some(("__ipow__", "__imul__"))),
    ("imatmul", "imul", Some(("__imatmul__", "__imul__"))),
    ("iand", "ior", Some(("__iand__", "__ior__"))),
    ("ior", "iand", Some(("__ior__", "__iand__"))),
    ("ilshift", "irshift", Some(("__ilshift__", "__irshift__"))),
    ("irshift", "ilshift", Some(("__irshift__", "__ilshift__"))),
    ("ixor", "iand", Some(("__ixor__", "__iand__"))),
    ("concat", "iconcat", Some(("__concat__", "__iconcat__"))),
    ("iconcat", "concat", Some(("__iconcat__", "__concat__"))),
    ("countOf", "indexOf", None),
    ("indexOf", "countOf", None),
    ("getitem", "contains", Some(("__getitem__", "__contains__"))),
];

#[rstest::rstest]
#[case::eq_1(0)]
#[case::ne_2(1)]
#[case::lt_3(2)]
#[case::le_4(3)]
#[case::gt_5(4)]
#[case::ge_6(5)]
#[case::add_7(6)]
#[case::sub_8(7)]
#[case::mul_9(8)]
#[case::truediv_10(9)]
#[case::floordiv_11(10)]
#[case::mod_12(11)]
#[case::pow_13(12)]
#[case::matmul_14(13)]
#[case::and_15(14)]
#[case::or_16(15)]
#[case::lshift_17(16)]
#[case::rshift_18(17)]
#[case::xor_19(18)]
#[case::neg_20(19)]
#[case::pos_21(20)]
#[case::abs_22(21)]
#[case::index_23(22)]
#[case::inv_24(23)]
#[case::invert_25(24)]
#[case::not_26(25)]
#[case::truth_27(26)]
#[case::is_28(27)]
#[case::is_not_29(28)]
#[case::is_none_30(29)]
#[case::is_not_none_31(30)]
#[case::iadd_32(31)]
#[case::isub_33(32)]
#[case::imul_34(33)]
#[case::itruediv_35(34)]
#[case::ifloordiv_36(35)]
#[case::imod_37(36)]
#[case::ipow_38(37)]
#[case::imatmul_39(38)]
#[case::iand_40(39)]
#[case::ior_41(40)]
#[case::ilshift_42(41)]
#[case::irshift_43(42)]
#[case::ixor_44(43)]
#[case::concat_45(44)]
#[case::iconcat_46(45)]
#[case::countof_47(46)]
#[case::indexof_48(47)]
#[case::getitem_49(48)]
fn operator_function_pairs_cover_qualified_imported_and_higher_order_references(
    #[case] case_index: usize,
) {
    let &(original, replacement, alias) = &OPERATOR_FUNCTION_PAIRS[case_index];

    for (original, replacement) in std::iter::once((original, replacement)).chain(alias) {
        for module in ["operator", "op"] {
            let source = format!(
                "import operator as {module}\nresult = {module}.{original}(left(), right())\n"
            );

            let output = analyze(&source);

            assert_eq!(output.candidates.len(), 1, "{source}");
            let candidate = &output.candidates[0];
            assert_eq!(candidate.operator.as_str(), "operator_function");
            assert_eq!(candidate.original, original);
            assert_eq!(candidate.replacement, replacement);
            assert_eq!(
                usize::try_from(candidate.span.start).unwrap(),
                source.rfind(&format!(".{original}(")).unwrap() + 1
            );
            assert_eq!(
                usize::try_from(candidate.span.length).unwrap(),
                original.len()
            );
            apply_candidate_and_reparse(&source, candidate);
        }
        for usage in ["plus(left(), right())", "map(plus, values, values)"] {
            let source = format!("from operator import {original} as plus\nresult = {usage}\n");

            let output = analyze(&source);

            assert_eq!(output.candidates.len(), 1, "{source}");
            let candidate = &output.candidates[0];
            assert_eq!(candidate.operator.as_str(), "operator_function");
            assert_eq!(candidate.original, "plus");
            assert_eq!(
                candidate.replacement,
                format!("__import__('operator').{replacement}")
            );
            assert_eq!(
                usize::try_from(candidate.span.start).unwrap(),
                source.rfind("plus").unwrap()
            );
            assert_eq!(candidate.span.length, 4);
            apply_candidate_and_reparse(&source, candidate);
        }
    }
}

#[rstest::rstest]
#[case::contains("contains", "(lambda container, item, /: item not in container)")]
#[case::contains_2("__contains__", "(lambda container, item, /: item not in container)")]
#[case::setitem("setitem", "(lambda container, key, value, /: None)")]
#[case::setitem_4("__setitem__", "(lambda container, key, value, /: None)")]
#[case::delitem("delitem", "(lambda container, key, /: None)")]
#[case::delitem_6("__delitem__", "(lambda container, key, /: None)")]
#[case::call("call", "(lambda target, /, *args, **kwargs: None)")]
#[case::call_8("__call__", "(lambda target, /, *args, **kwargs: None)")]
fn operator_function_lambdas_replace_only_callable_references(
    #[case] name: &str,
    #[case] replacement: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    let (import, reference) = [
        ("import operator as op".to_owned(), format!("op.{name}")),
        (
            format!("from operator import {name} as action"),
            "action".to_owned(),
        ),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let source = format!("{import}\nresult = {reference}(*arguments(), **keywords())\n");

    let output = analyze(&source);

    assert_eq!(output.candidates.len(), 1, "{source}");
    let candidate = &output.candidates[0];
    assert_eq!(candidate.operator.as_str(), "operator_function");
    assert_eq!(candidate.original, reference);
    assert_eq!(candidate.replacement, replacement);
    assert_eq!(
        usize::try_from(candidate.span.start).unwrap(),
        source.rfind(&reference).unwrap()
    );
    assert_eq!(
        usize::try_from(candidate.span.length).unwrap(),
        reference.len()
    );
    let mutated = apply_candidate_and_reparse(&source, candidate);
    assert!(mutated.ends_with("(*arguments(), **keywords())\n"));
}

#[rstest::rstest]
#[case::op_other("op = other")]
#[case::del_op("del op")]
#[case::op_other_3("op += other")]
#[case::op_object_other("op: object = other")]
#[case::op_x_pair("op, x = pair")]
#[case::def_f_op_pass("def f(op): pass")]
#[case::def_f_op_pass_7("def f(*op): pass")]
#[case::def_f_op_pass_8("def f(**op): pass")]
#[case::def_f_op_pass_9("def f(*, op): pass")]
#[case::f_lambda_op_op("f = lambda op: op")]
#[case::def_op_pass("def op(): pass")]
#[case::class_op_pass("class op: pass")]
#[case::type_op_object("type op = object")]
#[case::def_f_op_pass_14("def f[op](): pass")]
#[case::class_c_op_pass("class C[*op]: pass")]
#[case::type_t_op_object("type T[**op] = object")]
#[case::import_other_as_op("import other as op")]
#[case::from_other_import_value_as_op("from other import value as op")]
#[case::import_operator_as_op("import operator as op")]
#[case::if_condition_import_operator_as_op("if condition:\n    import operator as op")]
#[case::def_f_import_operator_as_op("def f():\n    import operator as op")]
#[case::from_other_import("from other import *")]
#[case::for_op_in_values_pass("for op in values: pass")]
#[case::with_context_as_op_pass("with context() as op: pass")]
#[case::try_pass_except_exception_as_op_pass("try: pass\nexcept Exception as op: pass")]
#[case::match_value_case_op_pass("match value:\n    case op: pass")]
#[case::match_value_case_op_pass_27("match value:\n    case [*op]: pass")]
#[case::match_value_case_x_x_op_pass("match value:\n    case {'x': x, **op}: pass")]
#[case::values_x_for_op_in_items("values = [x for op in items]")]
#[case::values_x_for_op_in_items_30("values = {x for op in items}")]
#[case::values_x_x_for_op_in_items("values = {x: x for op in items}")]
#[case::values_x_for_op_in_items_32("values = (x for op in items)")]
#[case::value_op_other("value = (op := other)")]
#[case::def_f_global_op("def f():\n    global op")]
#[case::def_f_nonlocal_op("def f():\n    nonlocal op")]
#[case::op_add_other("op.add = other")]
#[case::del_op_add("del op.add")]
#[case::op_add_other_38("op.add += other")]
#[case::op_add_object("op.add: object")]
#[case::setattr_op_add_other("setattr(op, 'add', other)")]
#[case::delattr_op_add("delattr(op, 'add')")]
#[case::exec_code("exec(code)")]
#[case::globals_op_other("globals()['op'] = other")]
#[case::locals_update_values("locals().update(values)")]
#[case::vars_op_add_other("vars(op)['add'] = other")]
#[case::op_dict_add_other("op.__dict__['add'] = other")]
#[case::namespace_op_dict("namespace = op.__dict__")]
#[case::run_exec("run = exec")]
#[case::import_operator_as_other_other_sub_replacement(
    "import operator as other\nother.sub = replacement"
)]
#[case::def_f_import_operator_as_other_other_add_replacement(
    "def f():\n    import operator as other\n    other.add = replacement"
)]
#[case::other_op_other_add_replacement("other = op\nother.add = replacement")]
#[case::change_op("change(op)")]
#[case::import_builtins_builtins_import_replacement(
    "import builtins\nbuiltins.__import__ = replacement"
)]
#[case::builtins_import_replacement("__builtins__['__import__'] = replacement")]
fn operator_function_uncertain_bindings_and_namespaces_are_excluded_table_1(
    #[case] alteration: &str,
) {
    let source = format!("import operator as op\nresult = op.add(a, b)\n{alteration}\n");

    assert!(parse_module(&source).is_ok(), "{source}");
    assert!(
        analyze(&source)
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "operator_function"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::if_condition_import_operator_as_op_result_op_add_a_b(
    "if condition:\n    import operator as op\nresult = op.add(a, b)\n"
)]
#[case::def_f_import_operator_as_op_return_op_add_a_b(
    "def f():\n    import operator as op\n    return op.add(a, b)\n"
)]
#[case::from_operator_import_add_result_add_a_b("from .operator import add\nresult = add(a, b)\n")]
#[case::from_other_import_add_result_add_a_b("from other import add\nresult = add(a, b)\n")]
#[case::import_other_as_op_result_op_add_a_b("import other as op\nresult = op.add(a, b)\n")]
#[case::from_operator_import_add_as_plus_def_f_plus_return_plus_a_b(
    "from operator import add as plus\ndef f(plus): return plus(a, b)\n"
)]
#[case::from_operator_import_add_as_plus_import_custom_result_plus_a_b(
    "from operator import add as plus\n__import__ = custom\nresult = plus(a, b)\n"
)]
#[case::from_operator_import_add_as_plus_def_f_import_return_plus_a_b(
    "from operator import add as plus\ndef f(__import__): return plus(a, b)\n"
)]
fn operator_function_uncertain_bindings_and_namespaces_are_excluded_table_2(#[case] source: &str) {
    assert!(analyze(source).candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::attrgetter("attrgetter")]
#[case::itemgetter("itemgetter")]
#[case::methodcaller("methodcaller")]
#[case::length_hint("length_hint")]
#[case::truth("__truth__")]
#[case::is("__is__")]
#[case::is_not("__is_not__")]
#[case::is_none("__is_none__")]
#[case::is_not_none("__is_not_none__")]
#[case::countof("__countOf__")]
#[case::indexof("__indexOf__")]
#[case::and("__and___")]
#[case::not("__not___")]
fn operator_function_helpers_and_undocumented_dunders_are_excluded(#[case] name: &str) {
    let source = format!("import operator as op\nresult = op.{name}(value)\n");

    assert!(analyze(&source).candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::result_op_add_a_b_import_operator_as_op("result = op.add(a, b)\nimport operator as op\n")]
#[case::result_plus_a_b_from_operator_import_add_as_plus(
    "result = plus(a, b)\nfrom operator import add as plus\n"
)]
#[case::def_f_return_plus_a_b_from_operator_import_add_as_plus(
    "def f(): return plus(a, b)\nfrom operator import add as plus\n"
)]
#[case::import_operator_as_op_x_op_add_def_f_x_op_mul_op_sub_pass_type_al(
    "import operator as op\nx: op.add\ndef f(x: op.mul) -> op.sub: pass\ntype Alias = op.pow\n"
)]
fn operator_function_source_order_selection_profiles_and_annotations_table_1(#[case] source: &str) {
    assert!(analyze(source).candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::zero(0)]
#[case::limit_1(1)]
#[case::limit_2(2)]
#[case::limit_3(3)]
#[case::limit_4(4)]
#[case::at_capacity(5)]
fn operator_function_source_order_selection_profiles_and_annotations_prefix_boundary(
    #[case] limit: usize,
) {
    let source = "import operator as op\nfrom operator import add as plus\ndef calculate(a, b):\n    return op.add(a, b), plus(a, b)\ndef combine(values):\n    return map(op.add, values, values)\n";
    let full = analyze(source);

    assert_eq!(
        full.candidates
            .iter()
            .filter(|c| c.operator.as_str() == "operator_function")
            .count(),
        3
    );

    let bounded = analyze_with_profile(MutationProfile::Full, limit, source);
    assert_eq!(
        bounded.candidates,
        full.candidates[..limit.min(full.candidates.len())]
    );
    assert_eq!(bounded.truncated, limit < full.candidates.len());
    assert!(
        bounded
            .retention
            .producer_peaks
            .iter()
            .all(|peak| *peak <= limit + 1)
    );
    assert!(bounded.retention.merged_peak <= limit + 1);
}

proptest::proptest! {
    #[test]
    fn operator_function_source_order_selection_profiles_and_annotations_generated_prefix(limit in 0usize..128) {
    let source = "import operator as op\nfrom operator import add as plus\ndef calculate(a, b):\n    return op.add(a, b), plus(a, b)\ndef combine(values):\n    return map(op.add, values, values)\n";
    let full = analyze(source);

    assert_eq!(
        full.candidates
            .iter()
            .filter(|c| c.operator.as_str() == "operator_function")
            .count(),
        3
    );

    let bounded = analyze_with_profile(MutationProfile::Full, limit, source);
    proptest::prop_assert_eq!(&bounded.candidates[..], &full.candidates[..limit.min(full.candidates.len())]);
    proptest::prop_assert_eq!(bounded.truncated, limit < full.candidates.len());
    proptest::prop_assert!(
        bounded
            .retention
            .producer_peaks
            .iter()
            .all(|peak| *peak <= limit + 1)
    );
    proptest::prop_assert!(bounded.retention.merged_peak <= limit + 1);

    }
}

#[test]
fn operator_function_source_order_selection_profiles_and_annotations() {
    let source = "import operator as op\nfrom operator import add as plus\ndef calculate(a, b):\n    return op.add(a, b), plus(a, b)\ndef combine(values):\n    return map(op.add, values, values)\n";
    let full = analyze(source);

    assert_eq!(
        full.candidates
            .iter()
            .filter(|c| c.operator.as_str() == "operator_function")
            .count(),
        3
    );

    let selected = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 6, end: 6 }],
        &["combine".to_owned()],
        10,
        source,
    );
    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(selected.candidates[0].original, "add");
    assert_eq!(selected.candidates[0].replacement, "sub");
    assert_eq!(selected.candidates[0].line, 6);
    assert_eq!(selected.candidates[0].symbol.as_deref(), Some("combine"));
    let arid = "import operator as op\nprint(op.add)\nassert op.eq(a, b)\nresult = op.sub(a, b)\n";
    assert_eq!(
        analyze(arid)
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == "operator_function")
            .count(),
        3
    );
    let focused = analyze_with_profile(MutationProfile::Focused, 10, arid);
    assert_eq!(focused.candidates.len(), 1);
    assert_eq!(focused.candidates[0].original, "sub");
    let mut operators = MutationOperatorSelection::default();
    operators.exclude(MutationOperator::from_name("operator_function").unwrap());

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10,
        },
        source,
    );

    assert!(
        output
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "operator_function")
    );
}

#[rstest::rstest]
#[case::import_operator_result_operator_add_a_b("import operator\nresult = operator.add(a, b)\n")]
#[case::from_operator_import_add_result_add_a_b("from operator import add\nresult = add(a, b)\n")]
#[case::import_operator_as_op_from_operator_import_add_as_plus_plus_other(
    "import operator as op\nfrom operator import add as plus\nplus = other\nresult = op.add(a, b)\n"
)]
#[case::import_operator_as_op_import_custom_result_op_add_a_b(
    "import operator as op\n__import__ = custom\nresult = op.add(a, b)\n"
)]
#[case::from_operator_import_setitem_as_update_import_custom_result_updat(
    "from operator import setitem as update\n__import__ = custom\nresult = update(a, b, c)\n"
)]
#[case::import_operator_as_op_class_c_def_f_self_self_value_value_return(
    "import operator as op\nclass C:\n    def f(self):\n        self.value = value\n        return op.add(a, b)\n"
)]
fn operator_function_independent_bindings_and_import_builtin_guards(#[case] source: &str) {
    assert_eq!(
        analyze(source)
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == "operator_function")
            .count(),
        1,
        "{source}"
    );
}

#[test]
fn operator_function_index_traversal_observes_cancellation() {
    let source = format!(
        "import operator as op\n{}",
        "result = op.add(a, b)\n".repeat(100)
    );
    let module = parse_module(&source).unwrap();
    let probes = std::cell::Cell::new(0);

    let result = super::OperatorImports::build(module.syntax(), &|| {
        probes.set(probes.get() + 1);
        probes.get() >= 10
    });

    assert!(matches!(result, Err(super::AnalysisCancelled)));
    assert_eq!(
        probes.get(),
        10,
        "cancellation must latch on its first observation"
    );
}

#[rstest::rstest]
#[case::from_operator_import_add_as_op_c_op_lambda_a_b_a_b_class_c_def_ru(
    "from operator import add as __op\n_C__op = lambda a, b: a * b\nclass C:\n    def run(self): return __op(2, 3)\n"
)]
#[case::import_operator_as_op_class_other_staticmethod_def_add_a_b_return(
    "import operator as __op\nclass Other:\n    @staticmethod\n    def add(a, b): return a * b\n_C__op = Other\nclass C:\n    def run(self): return __op.add(2, 3)\n"
)]
#[case::from_operator_import_setitem_as_op_c_op_lambda_a_b_c_c_class_c_de(
    "from operator import setitem as __op\n_C__op = lambda a, b, c: c\nclass C:\n    def run(self): return __op(container, key, value)\n"
)]
fn operator_function_class_mangling_does_not_trust_unrelated_callables(#[case] source: &str) {
    assert!(parse_module(source).is_ok(), "{source}");

    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "operator_function"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::from_operator_import_add_as_class_class_c_def_init_self_a_b_self(
    "from operator import add as __class__\nclass C:\n    def __init__(self, a, b): self.value = a * b\n    def run(self): return __class__(2, 3).value\n"
)]
#[case::import_operator_as_class_class_c_staticmethod_def_add_a_b_return(
    "import operator as __class__\nclass C:\n    @staticmethod\n    def add(a, b): return a * b\n    def run(self): return __class__.add(2, 3)\n"
)]
fn operator_function_implicit_class_bindings_do_not_resolve_to_module_imports_table_1(
    #[case] source: &str,
) {
    assert!(parse_module(source).is_ok(), "{source}");

    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "operator_function"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::module("__module__")]
#[case::qualname("__qualname__")]
#[case::firstlineno("__firstlineno__")]
#[case::type_params("__type_params__")]
#[case::annotations("__annotations__")]
#[case::doc("__doc__")]
#[case::static_attributes("__static_attributes__")]
#[case::classcell("__classcell__")]
#[case::classdict("__classdict__")]
#[case::classdictcell("__classdictcell__")]
#[case::annotate_func("__annotate_func__")]
#[case::safe("__safe__")]
fn operator_function_implicit_class_bindings_do_not_resolve_to_module_imports_table_2(
    #[case] alias: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    let (import, reference) = [
        (
            format!("from operator import add as {alias}"),
            alias.to_owned(),
        ),
        (
            format!("import operator as {alias}"),
            format!("{alias}.add"),
        ),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let source = format!("{import}\nclass C:\n    result = {reference}(2, 3)\n");

    assert!(parse_module(&source).is_ok(), "{source}");
    assert!(
        analyze(&source)
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "operator_function"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::op("__op")]
#[case::class("__class__")]
#[case::safe("__safe__")]
fn operator_function_class_guard_preserves_independent_import_references_scenario_1(
    #[case] alias: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    let selected_operators: &[&str] = &["operator_function"];

    let (import, reference) = [
        (
            format!("from operator import add as {alias}"),
            alias.to_owned(),
        ),
        (
            format!("import operator as {alias}"),
            format!("{alias}.add"),
        ),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let source = format!(
        "{import}\nbefore = {reference}(2, 3)\nclass C: pass\ndef run(): return {reference}(2, 3)\n"
    );

    let output = analyze_selected(selected_operators, &source);

    assert_eq!(output.candidates.len(), 2, "{source}");
    for candidate in &output.candidates {
        assert_eq!(candidate.operator.as_str(), "operator_function");
        apply_candidate_and_reparse(&source, candidate);
    }
}

#[test]
fn operator_function_class_guard_preserves_independent_import_references() {
    let selected_operators: &[&str] = &["operator_function"];

    let source = "import operator as op\nfrom operator import add as plus\nclass C:\n    initial = op.__add__(2, 3)\n    def run(self):\n        def nested(): return plus(2, 3)\n        return op.add(2, 3)\n";

    let output = analyze_selected(selected_operators, source);

    assert_eq!(output.candidates.len(), 2);
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|c| (c.original.as_str(), c.replacement.as_str()))
            .collect::<Vec<_>>(),
        vec![("plus", "__import__('operator').sub"), ("add", "sub")]
    );
    for candidate in &output.candidates {
        assert_eq!(candidate.operator.as_str(), "operator_function");
        apply_candidate_and_reparse(source, candidate);
    }
}

#[rstest::rstest]
#[case::import_operator_as_op("import operator as op", "op", "op.add")]
#[case::from_operator_import_add_as_plus("from operator import add as plus", "plus", "plus")]
fn operator_function_class_namespace_lookups_are_excluded(
    #[case] import: &str,
    #[case] alias: &str,
    #[case] reference: &str,
) {
    let source = format!(
        "{import}\nclass Proxy:\n    @staticmethod\n    def add(left, right): return left * right\nclass Meta(type):\n    @classmethod\n    def __prepare__(mcls, name, bases): return {{'{alias}': Proxy}}\nclass Base(metaclass=Meta): pass\nclass Subject(Base):\n    result = {reference}(2, 3)\n    @decorate({reference})\n    def configured(self, action={reference}): return action\n    callback = lambda: {reference}(2, 3)\n    def method(self):\n        class Nested(Base):\n            result = {reference}(2, 3)\n        return {reference}(2, 3)\n"
    );

    assert!(parse_module(&source).is_ok(), "{source}");

    let output = analyze(&source);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "operator_function")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), 2, "{source}");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| source.lines().nth(candidate.line as usize - 1).unwrap())
            .collect::<Vec<_>>(),
        vec![
            format!("    callback = lambda: {reference}(2, 3)"),
            format!("        return {reference}(2, 3)")
        ],
        "{source}"
    );
}

#[test]
fn operator_function_class_comprehensions_use_implicit_function_scope() {
    let source = "import operator as op\nclass Subject:\n    values = [op.add(2, 3) for _ in (0,)]\n    direct = op.add(2, 3)\n    callback = lambda: op.add(2, 3)\n";
    let candidates = analyze(source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .filter(|candidate| candidate.operator.as_str() == "operator_function")
        .map(|candidate| (candidate.line, candidate.original, candidate.replacement))
        .collect::<Vec<_>>();

    assert_eq!(
        candidates,
        vec![
            (3, "add".to_owned(), "sub".to_owned()),
            (5, "add".to_owned(), "sub".to_owned()),
        ]
    );
}

#[test]
fn operator_function_class_comprehensions_cover_all_forms_and_nested_scopes() {
    let source = "import operator as op\nfrom operator import sub as minus\nclass Subject:\n    set_values = {op.mul(2, 3) for _ in (0,)}\n    dict_values = {op.pow(2, 3): minus(3, 2) for _ in (0,)}\n    generator_values = tuple(op.xor(2, 3) for _ in (0,))\n    filtered = [item for item in (1,) if op.truth(item)]\n    later = [op.add(left, right) for left in (1,) for right in op.concat((2,), (3,))]\n    nested = [[op.mul(left, right) for right in op.sub((2,), (1,))] for left in (1,)]\n";
    let candidates = analyze(source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .filter(|candidate| candidate.operator.as_str() == "operator_function")
        .map(|candidate| (candidate.line, candidate.original, candidate.replacement))
        .collect::<Vec<_>>();

    assert_eq!(
        candidates,
        vec![
            (4, "mul".to_owned(), "truediv".to_owned()),
            (5, "pow".to_owned(), "mul".to_owned()),
            (
                5,
                "minus".to_owned(),
                "__import__('operator').add".to_owned(),
            ),
            (6, "xor".to_owned(), "and_".to_owned()),
            (7, "truth".to_owned(), "not_".to_owned()),
            (8, "add".to_owned(), "sub".to_owned()),
            (8, "concat".to_owned(), "iconcat".to_owned()),
            (9, "mul".to_owned(), "truediv".to_owned()),
            (9, "sub".to_owned(), "add".to_owned()),
        ]
    );
}

#[rstest::rstest]
#[case::from_operator_import_add_as_plus_class_subject_values_plus_2_3_fo(
    "from operator import add as plus\nclass Subject:\n    values = [plus(2, 3) for plus in (lambda left, right: left * right,)]\n"
)]
#[case::from_operator_import_add_as_plus_class_subject_values_plus_2_3_fo_2(
    "from operator import add as __plus\nclass Subject:\n    values = [__plus(2, 3) for _ in (0,)]\n"
)]
#[case::import_operator_as_op_class_subject_values_op_add_2_3_for_in_0(
    "import operator as __op\nclass Subject:\n    values = [__op.add(2, 3) for _ in (0,)]\n"
)]
fn operator_function_class_comprehensions_preserve_identity_guards_and_first_iterable_scope(
    #[case] excluded: &str,
) {
    let source = "import operator as op\nclass Subject:\n    values = [op.sub(3, 2) for _ in op.add((0,), (1,))]\n    nested = [[op.mul(left, right) for right in op.sub((2,), (1,))] for left in op.add((0,), (1,))]\n";
    let candidates = analyze(source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .filter(|candidate| candidate.operator.as_str() == "operator_function")
        .map(|candidate| (candidate.line, candidate.original, candidate.replacement))
        .collect::<Vec<_>>();

    assert_eq!(
        candidates,
        vec![
            (3, "sub".to_owned(), "add".to_owned()),
            (4, "mul".to_owned(), "truediv".to_owned()),
            (4, "sub".to_owned(), "add".to_owned()),
        ]
    );

    assert!(
        analyze(excluded)
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "operator_function"),
        "{excluded}"
    );
}

#[rstest::rstest]
#[case::import_builtins_as_b_b_exec_code("import builtins as b\nb.exec(code)")]
#[case::op_setattr_add_replacement("op.__setattr__('add', replacement)")]
#[case::op_delattr_add("op.__delattr__('add')")]
#[case::op_getattribute_dict_add_replacement(
    "op.__getattribute__('__dict__')['add'] = replacement"
)]
#[case::from_operator_import_dict_as_namespace_namespace_add_replacement(
    "from operator import __dict__ as namespace\nnamespace['add'] = replacement"
)]
#[case::from_operator_import_setattr_as_write_write_add_replacement(
    "from operator import __setattr__ as write\nwrite('add', replacement)"
)]
#[case::from_builtins_import_exec_as_execute_execute_code(
    "from builtins import exec as execute\nexecute(code)"
)]
#[case::from_builtins_import_globals_as_namespace_namespace_op_other(
    "from builtins import globals as namespace\nnamespace()['op'] = other"
)]
#[case::from_sys_import_modules_as_loaded_loaded_operator_add_replacement(
    "from sys import modules as loaded\nloaded['operator'].add = replacement"
)]
#[case::import_builtins_as_b_run_b_exec_run_code("import builtins as b\nrun = b.exec\nrun(code)")]
#[case::import_builtins_as_b_b_globals_op_other("import builtins as b\nb.globals()['op'] = other")]
#[case::builtins_exec_code("__builtins__.exec(code)")]
#[case::import_sys_sys_modules_operator_add_replacement(
    "import sys\nsys.modules['operator'].add = replacement"
)]
#[case::import_sys_as_system_system_modules_builtins_import_replacement(
    "import sys as system\nsystem.modules['builtins'].__import__ = replacement"
)]
fn operator_function_qualified_dynamic_namespace_access_is_uncertain(#[case] alteration: &str) {
    let source = format!("import operator as op\nresult = op.add(a, b)\n{alteration}\n");

    assert!(
        analyze(&source)
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != "operator_function"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::contains("contains")]
#[case::setitem("setitem")]
#[case::delitem("delitem")]
#[case::call("call")]
fn operator_function_lambda_replacements_exclude_pattern_values_table_1(#[case] name: &str) {
    let source = format!("import operator as op\nmatch value:\n    case op.{name}: pass\n");

    let output = analyze(&source);
    assert_eq!(
        output.candidates,
        Vec::<super::payload::Charged<AnalyzerCandidate>>::new()
    );
}

#[rstest::rstest]
#[case::from_operator_import_add_as_plus_match_value_case_plus_pass(
    "from operator import add as plus\nmatch value:\n    case plus(): pass\n"
)]
#[case::import_operator_as_op_match_value_case_op_call_pass(
    "import operator as op\nmatch value:\n    case op.call(): pass\n"
)]
fn operator_function_lambda_replacements_exclude_pattern_values_table_2(#[case] source: &str) {
    assert!(analyze(source).candidates.is_empty(), "{source}");
}

#[test]
fn operator_function_lambda_replacements_exclude_pattern_values() {
    let source =
        "import operator as op\nmatch value:\n    case _ if op.contains(container, item): pass\n";

    let output = analyze(source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "op.contains");
    assert_eq!(
        output.candidates[0].replacement,
        "(lambda container, item, /: item not in container)"
    );
    apply_candidate_and_reparse(source, &output.candidates[0]);
}

fn apply_candidate_and_reparse(source: &str, candidate: &AnalyzerCandidate) -> String {
    let start = usize::try_from(candidate.span.start).expect("candidate start fits usize");
    let length = usize::try_from(candidate.span.length).expect("candidate length fits usize");
    let end = start
        .checked_add(length)
        .expect("candidate end does not overflow");
    let mut mutated = source.to_owned();
    mutated.replace_range(start..end, &candidate.replacement);
    assert!(
        parse_module(&mutated).is_ok(),
        "candidate {} produced invalid Python: {mutated}",
        candidate.operator.as_str()
    );
    mutated
}

fn assert_candidates_reparse(
    source: &str,
    candidates: &[super::payload::Charged<AnalyzerCandidate>],
) {
    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

fn assert_type_list_sequence_sites(source: &str, expected: &[(u64, u64, u32, Option<&str>, &str)]) {
    let output = analyze_types(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(actual, expected);
    assert_candidates_reparse(source, &output.candidates);
}

fn prefix_candidate(
    start: u64,
    replacement: &str,
    operator: MutationOperator,
) -> AnalyzerCandidate {
    AnalyzerCandidate {
        path: Utf8Path::new("pkg/sample.py").to_path_buf(),
        span: ByteSpan { start, length: 1 },
        original: "original".to_owned(),
        replacement: replacement.to_owned(),
        operator,
        line: 1,
        column: 0,
        symbol: None,
    }
}

fn candidate_starts(candidates: &[super::payload::Charged<AnalyzerCandidate>]) -> Vec<u64> {
    candidates
        .iter()
        .map(|candidate| candidate.span.start)
        .collect()
}

#[test]
fn candidate_operator_remains_typed_until_persistence() {
    let source = "result = left == right\n";

    let output = analyze(source);

    assert_eq!(output.candidates[0].operator, MutationOperator::CompareEqNe);
}

// These names are hand-ordered, independently of both production comparators.
// The enum declares CompareEqNe before BooleanLiteral before StatementDelete.
proptest! {
    #[test]
    fn candidate_prefix_uses_lexical_names_with_duplicates(
        order in prop::collection::vec(0usize..3, 0..30),
        limit in 0usize..5,
    ) {
        let names = ["boolean_literal", "compare_eq_ne", "statement_delete"];
        let operators = [MutationOperator::BooleanLiteral, MutationOperator::CompareEqNe, MutationOperator::StatementDelete];
        let mut prefix = CandidatePrefix::new(limit, super::payload::PayloadBudget::default());
        let emissions = order.into_iter().chain([2, 1, 0]);

        for index in emissions {
            prefix.push(prefix_candidate(1, "same", operators[index])).unwrap();
        }

        let result = prefix.finish().unwrap();

        let actual: Vec<_> = result.candidates.iter().map(|c| c.operator.as_str()).collect();

        prop_assert_eq!(actual, names[..3.min(limit + 1)].to_vec());
        prop_assert_eq!(result.overflowed, 3 > limit + 1);
    }
}

#[rstest::rstest]
#[case::value_0_1(0)]
#[case::value_1_2(1)]
#[case::value_2_3(2)]
#[case::value_3_4(3)]
#[case::usize_max_5(4)]
fn same_position_candidates_use_lexical_names_at_every_limit(#[case] scenario_index: usize) {
    let source = "from typing import Sequence\nvalue: list[int]\n";
    let expected = ["type_list_sequence", "type_nullable_add"];
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(operator);
        }
    }
    operators.include(MutationOperator::TypeListSequence);
    operators.include(MutationOperator::TypeNullableAdd);

    let limit = [0, 1, 2, 3, usize::MAX]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: limit,
    };

    let output = analyze_source(&request, source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .map(|c| c.operator.as_str())
        .collect();

    assert_eq!(actual, expected[..2.min(limit)], "limit {limit}");
    assert_eq!(output.truncated, limit < 2);
}

#[test]
fn candidate_prefix_retains_the_earliest_k_plus_one_unique_candidates() {
    let mut prefix = CandidatePrefix::new(2, super::payload::PayloadBudget::default());

    assert_eq!(prefix.capacity, 3);

    for candidate in [
        prefix_candidate(9, "nine", MutationOperator::CompareEqNe),
        prefix_candidate(1, "one", MutationOperator::CompareEqNe),
        prefix_candidate(5, "five", MutationOperator::CompareEqNe),
        prefix_candidate(3, "three", MutationOperator::CompareEqNe),
        prefix_candidate(1, "one", MutationOperator::CompareEqNe),
    ] {
        prefix.push(candidate).unwrap();
    }

    assert_eq!(prefix.identities.len(), 3);

    let result = prefix.finish().unwrap();

    assert_eq!(candidate_starts(&result.candidates), vec![1, 3, 5]);
    assert!(result.overflowed);
    assert_eq!(result.candidates.len(), 3);
}

#[test]
fn candidate_prefix_keeps_emission_order_for_equal_sort_keys() {
    let mut prefix = CandidatePrefix::new(2, super::payload::PayloadBudget::default());
    for candidate in [
        prefix_candidate(1, "first", MutationOperator::CompareEqNe),
        prefix_candidate(1, "second", MutationOperator::CompareEqNe),
        prefix_candidate(1, "third", MutationOperator::CompareEqNe),
    ] {
        prefix.push(candidate).unwrap();
    }

    let result = prefix.finish().unwrap();

    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| candidate.replacement.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "third"]
    );
    assert!(!result.overflowed);
}

#[test]
fn candidate_prefix_with_zero_limit_retains_one_earliest_candidate() {
    let mut prefix = CandidatePrefix::new(0, super::payload::PayloadBudget::default());

    assert_eq!(prefix.capacity, 1);
    prefix
        .push(prefix_candidate(3, "three", MutationOperator::CompareEqNe))
        .unwrap();
    prefix
        .push(prefix_candidate(3, "three", MutationOperator::CompareEqNe))
        .unwrap();
    prefix
        .push(prefix_candidate(1, "one", MutationOperator::CompareEqNe))
        .unwrap();

    let result = prefix.finish().unwrap();

    assert_eq!(candidate_starts(&result.candidates), vec![1]);
    assert!(result.overflowed);
    assert_eq!(result.candidates.len(), 1);
}

#[test]
fn candidate_prefix_saturates_capacity_at_usize_maximum() {
    let prefix = CandidatePrefix::new(usize::MAX, super::payload::PayloadBudget::default());

    assert_eq!(prefix.capacity, usize::MAX);
}

#[test]
fn candidate_prefix_matches_the_lean_out_of_order_duplicate_case() {
    let mut item = bounded_discovery_case("out_of_order_duplicate");
    // The abstract Lean fixture uses one arbitrary operator identity. Project it
    // onto one supported enum; equality and all rank/descriptor fields are retained.
    for input in &mut item.token {
        assert_eq!(input.operator, "operator");
        input.operator = "compare_eq_ne".to_owned();
    }
    assert_eq!(
        (item.schema, item.mode.as_str(), item.scenario.as_str()),
        (1, "internal-fixture", "candidate_prefix")
    );
    let mut prefix = CandidatePrefix::new(
        usize::try_from(item.limit).unwrap(),
        super::payload::PayloadBudget::default(),
    );
    for input in &item.token {
        assert!(input.eligible);
        assert!(matches!(input.producer.as_str(), "token" | "ast"));
        let _ = input.emission_index;
        prefix
            .push(prefix_candidate(
                input.order_key,
                &input.identity.to_string(),
                MutationOperator::CompareEqNe,
            ))
            .unwrap();
    }

    let result = prefix.finish().unwrap();

    let actual = result
        .candidates
        .iter()
        .map(|candidate| candidate.replacement.parse::<u64>().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(actual, item.expected_identities);
    for (actual, identity) in result.candidates.iter().zip(&item.expected_identities) {
        let expected = item
            .token
            .iter()
            .find(|candidate| candidate.identity == *identity)
            .expect("Lean retained descriptor");
        assert_analyzer_candidate_matches_lean(actual, expected);
    }
    assert_eq!(result.overflowed, item.expected_truncated);
    assert!(item.ast.is_empty() && item.annotation.is_empty() && item.targets.is_empty());
    assert_eq!(item.expected_sequences, vec![1, 2, 3]);
    assert_eq!(item.expected_targets_read, 0);
    assert!(!item.expected_spool_finished);
}

fn assert_analyzer_candidate_matches_lean(
    actual: &AnalyzerCandidate,
    expected: &BoundedCandidateInput,
) {
    assert_eq!(actual.path.as_str(), expected.path);
    assert_eq!(actual.span.start, expected.span_start);
    assert_eq!(actual.span.length, expected.span_length);
    assert_eq!(actual.original, expected.original);
    assert_eq!(actual.replacement, expected.replacement);
    assert_eq!(actual.operator.as_str(), expected.operator);
    assert_eq!(actual.line, expected.line);
    assert_eq!(actual.column, expected.column);
}

#[test]
fn real_three_producer_prefix_matches_the_lean_merge_projection() {
    let item = bounded_discovery_case("three_producer_merge");
    let operators: MutationOperatorSelection = serde_json::from_value(serde_json::json!([
        "compare_eq_ne",
        "collection_list_tuple",
        "type_list_sequence"
    ]))
    .unwrap();
    let source =
        "from typing import Sequence\na = left == right\nb = list(items)\nc: list[int] = value\n";
    let request = |max_candidates| AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("pkg/three.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates,
    };
    let complete = analyze_source(
        &request(item.token.len() + item.ast.len() + item.annotation.len()),
        source,
    );
    let complete_expected = item
        .token
        .iter()
        .chain(&item.ast)
        .chain(&item.annotation)
        .collect::<Vec<_>>();

    assert_eq!(complete.candidates.len(), complete_expected.len());
    for (actual, expected) in complete.candidates.iter().zip(&complete_expected) {
        assert_analyzer_candidate_matches_lean(actual, expected);
    }
    assert!(!complete.truncated);

    let bounded = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            max_candidates: usize::try_from(item.limit).unwrap(),
            ..request(0)
        },
        source,
    );
    let expected = item
        .token
        .iter()
        .chain(&item.ast)
        .chain(&item.annotation)
        .filter(|candidate| item.expected_identities.contains(&candidate.identity))
        .collect::<Vec<_>>();
    assert_eq!(bounded.candidates.len(), expected.len());
    for (actual, expected) in bounded.candidates.iter().zip(expected) {
        assert_analyzer_candidate_matches_lean(actual, expected);
    }
    assert_eq!(bounded.truncated, item.expected_truncated);
    assert_eq!(bounded.retention.producer_peaks, [1, 1, 1]);
    assert_eq!(
        bounded.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
    assert!(item.expected_candidate_limit_diagnostic);
    assert_eq!(item.expected_exit_code, 0);
    assert_eq!(item.token.len(), 1);
    assert_eq!(item.ast.len(), 1);
    assert_eq!(item.annotation.len(), 1);
}

#[test]
fn eligibility_and_zero_limit_rows_match_real_analyzer_outputs() {
    let selected_operators: &[&str] = &["boolean_literal", "binary_add_sub", "compare_eq_ne"];
    let eligibility = bounded_discovery_case("eligibility_before_capacity");
    let focused = analyze_selected_with_profile(
        selected_operators,
        MutationProfile::Focused,
        usize::try_from(eligibility.limit).unwrap(),
        "print(True)\nresult = 1 + 2\n",
    );

    assert_eq!(focused.candidates.len(), 1);
    let expected = eligibility
        .token
        .iter()
        .find(|candidate| {
            eligibility
                .expected_identities
                .contains(&candidate.identity)
        })
        .unwrap();
    assert_analyzer_candidate_matches_lean(&focused.candidates[0], expected);
    assert_eq!(focused.truncated, eligibility.expected_truncated);

    let zero = bounded_discovery_case("zero_limit");

    let output = analyze_selected_with_profile(
        selected_operators,
        MutationProfile::Full,
        usize::try_from(zero.limit).unwrap(),
        "value = left == right\n",
    );

    assert_eq!(output.candidates, Vec::new());
    assert_eq!(output.truncated, zero.expected_truncated);
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
    assert!(zero.expected_candidate_limit_diagnostic);
    assert_eq!(zero.expected_sequences, Vec::<u64>::new());
}

#[test]
fn exception_handler_type_tuples_do_not_emit_collection_candidates() {
    let source = concat!(
        "before_list = [before]\n",
        "before_tuple = (before,)\n",
        "try:\n    work()\n",
        "except (ValueError, TypeError):\n",
        "    handler_list = [handler]\n",
        "    handler_tuple = (handler,)\n",
        "    try:\n        work()\n",
        "    except ((KeyError, IndexError)):\n",
        "        nested_list = [nested]\n",
        "        nested_tuple = (nested,)\n",
        "try:\n    work()\n",
        "except* (ValueError, TypeError):\n",
        "    starred_list = [starred]\n",
        "    starred_tuple = (starred,)\n",
        "after_list = [after]\n",
        "after_tuple = (after,)\n",
    );

    let output = analyze(source);
    let collection_candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .collect();

    assert!(collection_candidates.iter().all(|candidate| {
        !matches!(
            candidate.original.as_str(),
            "(ValueError, TypeError)" | "(KeyError, IndexError)"
        )
    }));

    let actual: Vec<_> = collection_candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.span,
            )
        })
        .collect();
    let expected: Vec<_> = [
        ("[before]", "(before,)"),
        ("(before,)", "[before,]"),
        ("[handler]", "(handler,)"),
        ("(handler,)", "[handler,]"),
        ("[nested]", "(nested,)"),
        ("(nested,)", "[nested,]"),
        ("[starred]", "(starred,)"),
        ("(starred,)", "[starred,]"),
        ("[after]", "(after,)"),
        ("(after,)", "[after,]"),
    ]
    .into_iter()
    .map(|(original, replacement)| {
        (
            original,
            replacement,
            ByteSpan {
                start: source
                    .find(original)
                    .expect("literal fixture contains the expected source span")
                    as u64,
                length: original.len() as u64,
            },
        )
    })
    .collect();

    assert_eq!(actual, expected);

    for candidate in collection_candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[rstest::rstest]
#[case::try_work_except_tuple_valueerror_typeerror_handler_tuple_1_2_1(0)]
#[case::try_work_except_tuple_valueerror_typeerror_handler_tuple_1_2_2(1)]
fn exception_handler_type_builtin_calls_are_excluded_for_ordinary_and_starred_handlers(
    #[case] scenario_index: usize,
) {
    let (source, expected) = [
        (
            "try:\n    work()\nexcept tuple((ValueError, TypeError)):\n    handler = tuple((1, 2))\n",
            vec![
                (
                    "tuple",
                    "list",
                    ByteSpan {
                        start: 69,
                        length: 5,
                    },
                ),
                (
                    "(1, 2)",
                    "[1, 2]",
                    ByteSpan {
                        start: 75,
                        length: 6,
                    },
                ),
            ],
        ),
        (
            "try:\n    work()\nexcept* tuple((ValueError, TypeError)):\n    handler = tuple((1, 2))\n",
            vec![
                (
                    "tuple",
                    "list",
                    ByteSpan {
                        start: 70,
                        length: 5,
                    },
                ),
                (
                    "(1, 2)",
                    "[1, 2]",
                    ByteSpan {
                        start: 76,
                        length: 6,
                    },
                ),
            ],
        ),
    ].into_iter().nth(scenario_index).unwrap();

    let output = analyze(source);
    let collection_candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .collect();

    let actual: Vec<_> = collection_candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.span,
            )
        })
        .collect();

    assert_eq!(actual, expected, "source: {source:?}");
    for candidate in collection_candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the fixture records every curated safe exception replacement and exact span"
)]
fn exception_type_pair_candidates_are_curated_and_syntax_directed() {
    let source = concat!(
        "before_list = [before]\n",
        "try:\n    work()\n",
        "except ValueError:\n    pass\n",
        "except TypeError:\n    pass\n",
        "except KeyError:\n    pass\n",
        "except IndexError:\n    pass\n",
        "except AttributeError:\n    pass\n",
        "except FileNotFoundError:\n    pass\n",
        "except PermissionError:\n    pass\n",
        "except ConnectionError:\n    pass\n",
        "except TimeoutError:\n    pass\n",
        "except ImportError:\n    pass\n",
        "except ModuleNotFoundError:\n    pass\n",
        "except ZeroDivisionError:\n    pass\n",
        "except OverflowError:\n    pass\n",
        "after_tuple = (after,)\n",
    );

    let output = analyze(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.span,
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                "ValueError",
                "TypeError",
                ByteSpan {
                    start: 46,
                    length: 10,
                },
            ),
            (
                "TypeError",
                "ValueError",
                ByteSpan {
                    start: 74,
                    length: 9,
                },
            ),
            (
                "KeyError",
                "IndexError",
                ByteSpan {
                    start: 101,
                    length: 8,
                },
            ),
            (
                "KeyError",
                "AttributeError",
                ByteSpan {
                    start: 101,
                    length: 8,
                },
            ),
            (
                "IndexError",
                "KeyError",
                ByteSpan {
                    start: 127,
                    length: 10,
                },
            ),
            (
                "AttributeError",
                "KeyError",
                ByteSpan {
                    start: 155,
                    length: 14,
                },
            ),
            (
                "FileNotFoundError",
                "PermissionError",
                ByteSpan {
                    start: 187,
                    length: 17,
                },
            ),
            (
                "PermissionError",
                "FileNotFoundError",
                ByteSpan {
                    start: 222,
                    length: 15,
                },
            ),
            (
                "ConnectionError",
                "TimeoutError",
                ByteSpan {
                    start: 255,
                    length: 15,
                },
            ),
            (
                "TimeoutError",
                "ConnectionError",
                ByteSpan {
                    start: 288,
                    length: 12,
                },
            ),
            (
                "ImportError",
                "ModuleNotFoundError",
                ByteSpan {
                    start: 318,
                    length: 11,
                },
            ),
            (
                "ModuleNotFoundError",
                "ImportError",
                ByteSpan {
                    start: 347,
                    length: 19,
                },
            ),
            (
                "ZeroDivisionError",
                "OverflowError",
                ByteSpan {
                    start: 384,
                    length: 17,
                },
            ),
            (
                "OverflowError",
                "ZeroDivisionError",
                ByteSpan {
                    start: 419,
                    length: 13,
                },
            ),
        ]
    );
    for candidate in output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
    {
        apply_candidate_and_reparse(source, candidate);
    }

    let unsupported = concat!(
        "try:\n    work()\n",
        "except (ValueError, TypeError):\n    pass\n",
        "except module.Error:\n    pass\n",
        "except make_error():\n    pass\n",
    );
    assert!(
        analyze(unsupported)
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "exception_type_pair")
    );

    let starred = "try:\n    work()\nexcept* ValueError:\n    pass\n";
    assert!(parse_module(starred).is_ok(), "except* fixture parses");
    assert_eq!(
        analyze(starred)
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
            .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str(),))
            .collect::<Vec<_>>(),
        [("ValueError", "TypeError")]
    );

    let nested = concat!(
        "try:\n    work()\n",
        "except* ValueError:\n",
        "    try:\n        work()\n",
        "    except TypeError:\n        pass\n",
    );
    assert!(
        analyze(nested)
            .candidates
            .iter()
            .any(|candidate| candidate.original == "TypeError")
    );
}

#[test]
fn except_star_simple_exception_type_emits_safe_pair_candidate() {
    let source = concat!(
        "def handle_group():\n",
        "    try:\n",
        "        work()\n",
        "    except* ValueError as error:\n",
        "        recover(error)\n",
    );

    let candidates = analyze(source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.original, "ValueError");
    assert_eq!(candidate.replacement, "TypeError");
    assert_eq!(
        candidate.span,
        ByteSpan {
            start: 56,
            length: 10
        }
    );
    assert_eq!(candidate.line, 4);
    assert_eq!(candidate.symbol.as_deref(), Some("handle_group"));
    assert_eq!(
        apply_candidate_and_reparse(source, candidate),
        source.replacen("except* ValueError", "except* TypeError", 1)
    );
}

#[test]
fn except_star_exception_type_pairs_are_simple_scope_aware_and_nested() {
    let scope_source = concat!(
        "def source_shadowed(ValueError):\n",
        "    try:\n",
        "        work()\n",
        "    except* ValueError:\n",
        "        pass\n",
        "def destination_shadowed(TypeError):\n",
        "    try:\n",
        "        work()\n",
        "    except* ValueError:\n",
        "        pass\n",
        "def bound_target():\n",
        "    try:\n",
        "        work()\n",
        "    except* ValueError as TypeError:\n",
        "        recover(TypeError)\n",
        "def clean():\n",
        "    try:\n",
        "        work()\n",
        "    except* KeyError:\n",
        "        pass\n",
    );
    let scoped = analyze(scope_source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .map(|candidate| {
            (
                candidate.original,
                candidate.replacement,
                candidate.line,
                candidate.symbol,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        scoped,
        [
            (
                "KeyError".to_owned(),
                "IndexError".to_owned(),
                19,
                Some("clean".to_owned()),
            ),
            (
                "KeyError".to_owned(),
                "AttributeError".to_owned(),
                19,
                Some("clean".to_owned()),
            ),
        ]
    );

    let nested_source = concat!(
        "try:\n",
        "    work()\n",
        "except* (ValueError, TypeError):\n",
        "    try:\n",
        "        work()\n",
        "    except TypeError:\n",
        "        pass\n",
        "try:\n",
        "    work()\n",
        "except ValueError:\n",
        "    try:\n",
        "        work()\n",
        "    except* KeyError:\n",
        "        pass\n",
    );
    let nested_output = analyze(nested_source);
    let nested = nested_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        nested,
        [
            ("TypeError", "ValueError", 6),
            ("ValueError", "TypeError", 10),
            ("KeyError", "IndexError", 13),
            ("KeyError", "AttributeError", 13),
        ]
    );
    for candidate in nested_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
    {
        apply_candidate_and_reparse(nested_source, candidate);
    }
}

#[test]
fn except_star_exception_type_pairs_keep_shared_selection_and_profile_behavior() {
    let selected_operators: &[&str] = &["exception_type_pair"];
    let source =
        "def selected():\n    try:\n        work()\n    except* ValueError:\n        pass\n";
    let summarize = |output: &super::AnalyzerOutput| {
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
            .map(|candidate| {
                (
                    candidate.original.clone(),
                    candidate.replacement.clone(),
                    candidate.line,
                    candidate.symbol.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    let full =
        analyze_selected_with_profile(selected_operators, MutationProfile::Full, 10_000, source);
    let focused =
        analyze_selected_with_profile(selected_operators, MutationProfile::Focused, 10_000, source);

    assert_eq!(summarize(&full), summarize(&focused));
    assert_eq!(
        summarize(&full),
        [(
            "ValueError".to_owned(),
            "TypeError".to_owned(),
            4,
            Some("selected".to_owned()),
        )]
    );

    let selected = analyze_selected_with(
        selected_operators,
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 4, end: 4 }],
        &["pkg.sample:selected".to_owned()],
        10_000,
        source,
    );
    assert_eq!(summarize(&selected), summarize(&full));

    let bounded = analyze_selected_with_profile(
        selected_operators,
        MutationProfile::Full,
        1,
        "try:\n    work()\nexcept* KeyError:\n    pass\n",
    );
    assert_eq!(summarize(&bounded).len(), 1);
    assert!(bounded.truncated);

    let only_pair_output = analyze_selected(
        selected_operators,
        "try:\n    work(left == right)\nexcept* ValueError:\n    pass\n",
    );
    assert_eq!(summarize(&only_pair_output).len(), 1);
    assert_eq!(only_pair_output.candidates.len(), 1);

    let mut excluded_operators = MutationOperatorSelection::default();
    excluded_operators.exclude(MutationOperator::ExceptionTypePair);
    let excluded = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &excluded_operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert_eq!(summarize(&excluded), Vec::new());

    let arid_source = concat!(
        "if __name__ == \"__main__\":\n",
        "    try:\n",
        "        work()\n",
        "    except* ValueError:\n",
        "        pass\n",
    );
    assert_eq!(
        summarize(&analyze_selected_with_profile(
            selected_operators,
            MutationProfile::Full,
            10_000,
            arid_source,
        ))
        .len(),
        1
    );
    assert_eq!(
        summarize(&analyze_selected_with_profile(
            selected_operators,
            MutationProfile::Focused,
            10_000,
            arid_source,
        )),
        Vec::<(String, String, u32, Option<String>)>::new()
    );
}

#[test]
fn except_star_handlers_do_not_emit_explicit_risky_exception_candidates() {
    let source = concat!(
        "try:\n    work()\n",
        "except* ValueError:\n    pass\n",
        "except* Exception:\n    pass\n",
        "except* BaseException:\n    pass\n",
        "except* (ValueError,):\n    pass\n",
        "except* (ValueError, TypeError):\n    pass\n",
    );
    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::ExceptionBareToException,
        MutationOperator::ExceptionExceptionToBare,
        MutationOperator::ExceptionBaseBoundary,
        MutationOperator::ExceptionTupleAddPair,
        MutationOperator::ExceptionTupleRemoveMember,
    ] {
        operators.include(operator);
    }

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    let exception_candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("exception_"))
        .collect::<Vec<_>>();

    assert_eq!(exception_candidates.len(), 1);
    assert_eq!(
        exception_candidates[0].operator.as_str(),
        "exception_type_pair"
    );
    assert_eq!(exception_candidates[0].original, "ValueError");
    assert_eq!(exception_candidates[0].replacement, "TypeError");
    apply_candidate_and_reparse(source, exception_candidates[0]);
}

#[test]
fn raise_exception_type_pair_candidates_preserve_the_primary_expression() {
    let source = concat!(
        "def direct():\n    raise ValueError\n\n",
        "def constructed(message, code):\n",
        "    raise TypeError(message, code=code)\n\n",
        "def chained(key, cause):\n",
        "    raise KeyError(key) from cause\n",
    );

    let output = analyze(source);

    let actual = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.span,
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        [
            (
                "ValueError",
                "TypeError",
                ByteSpan {
                    start: 24,
                    length: 10,
                },
                2,
                Some("direct"),
            ),
            (
                "TypeError",
                "ValueError",
                ByteSpan {
                    start: 78,
                    length: 9,
                },
                5,
                Some("constructed"),
            ),
            (
                "KeyError",
                "IndexError",
                ByteSpan {
                    start: 144,
                    length: 8,
                },
                8,
                Some("chained"),
            ),
            (
                "KeyError",
                "AttributeError",
                ByteSpan {
                    start: 144,
                    length: 8,
                },
                8,
                Some("chained"),
            ),
        ]
    );

    for candidate in output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
    {
        let mutated = apply_candidate_and_reparse(source, candidate);
        assert!(mutated.contains("(message, code=code)"));
        assert!(mutated.contains("(key) from cause"));
    }
}

#[rstest::rstest]
#[case::valueerror_1(0)]
#[case::typeerror_2(1)]
#[case::keyerror_3(2)]
#[case::indexerror_4(3)]
#[case::attributeerror_5(4)]
#[case::filenotfounderror_6(5)]
#[case::permissionerror_7(6)]
#[case::connectionerror_8(7)]
#[case::timeouterror_9(8)]
#[case::importerror_10(9)]
#[case::modulenotfounderror_11(10)]
#[case::zerodivisionerror_12(11)]
#[case::overflowerror_13(12)]
fn raise_exception_type_pair_supports_every_curated_source_in_both_shapes(
    #[case] scenario_index: usize,
    #[values(0, 1)] nested_scenario_index: usize,
) {
    let (source_name, expected_replacements) = [
        ("ValueError", &["TypeError"][..]),
        ("TypeError", &["ValueError"][..]),
        ("KeyError", &["IndexError", "AttributeError"][..]),
        ("IndexError", &["KeyError"][..]),
        ("AttributeError", &["KeyError"][..]),
        ("FileNotFoundError", &["PermissionError"][..]),
        ("PermissionError", &["FileNotFoundError"][..]),
        ("ConnectionError", &["TimeoutError"][..]),
        ("TimeoutError", &["ConnectionError"][..]),
        ("ImportError", &["ModuleNotFoundError"][..]),
        ("ModuleNotFoundError", &["ImportError"][..]),
        ("ZeroDivisionError", &["OverflowError"][..]),
        ("OverflowError", &["ZeroDivisionError"][..]),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let source = [
        format!("raise {source_name}\n"),
        format!("raise {source_name}('message')\n"),
    ]
    .into_iter()
    .nth(nested_scenario_index)
    .unwrap();

    let output = analyze(&source);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .collect::<Vec<_>>();

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.replacement.as_str())
            .collect::<Vec<_>>(),
        expected_replacements,
        "source={source:?}",
    );
    for candidate in candidates {
        assert_eq!(candidate.original, source_name);
        assert_eq!(candidate.span.start, 6);
        assert_eq!(candidate.span.length, source_name.len() as u64);
        apply_candidate_and_reparse(&source, candidate);
    }
}

#[test]
fn raise_exception_type_pairs_skip_unsupported_and_non_primary_forms() {
    let source = concat!(
        "def reraised():\n    raise\n",
        "def qualified():\n    raise errors.ValueError\n",
        "def dynamic():\n    raise factory()\n",
        "def subscripted():\n    raise errors[kind]()\n",
        "def cause_only():\n    raise CustomError from ValueError\n",
        "def system_exit():\n    raise SystemExit(1)\n",
        "def keyboard_interrupt():\n    raise KeyboardInterrupt\n",
        "def generator_exit():\n    raise GeneratorExit()\n",
    );

    assert!(
        analyze(source)
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "exception_type_pair")
    );
}

#[test]
fn raise_exception_type_pairs_observe_extended_resolution_boundaries() {
    let exception_lines = |source: &str| {
        analyze(source)
            .candidates
            .into_iter()
            .map(super::payload::Charged::into_untracked)
            .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
            .map(|candidate| candidate.line)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        exception_lines("raise ValueError\nValueError = CustomValueError\nraise ValueError\n"),
        [1]
    );
    assert_eq!(
        exception_lines("from helpers import ValueError\nraise ValueError\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        exception_lines("from helpers import TypeError\nraise ValueError\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        exception_lines("def source(ValueError):\n    raise ValueError\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        exception_lines("def destination(TypeError):\n    raise ValueError\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        exception_lines(concat!(
            "def comprehension(errors):\n",
            "    captured = [TypeError for TypeError in errors]\n",
            "    raise ValueError\n",
        )),
        [3]
    );
    assert_eq!(
        exception_lines("from helpers import *\nraise ValueError\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        exception_lines(concat!(
            "raise ValueError\n",
            "exec('ValueError = CustomValueError')\n",
            "raise ValueError\n",
        )),
        [1]
    );
}

#[test]
fn raise_exception_type_pairs_preserve_normal_primary_and_cause_candidates() {
    let source = concat!(
        "def selected(items, causes):\n",
        "    raise ValueError(any(items)) from TypeError(all(causes))\n",
    );

    let output = analyze(source);
    let exception_pairs = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(exception_pairs, [("ValueError", "TypeError")]);

    let ordinary_pairs = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_any_all")
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(ordinary_pairs, [("any", "all"), ("all", "any")]);
    assert_candidates_reparse(source, &output.candidates);
}

#[test]
fn raise_exception_type_pairs_use_scope_aware_source_and_destination_resolution() {
    let source = concat!(
        "def clean():\n",
        "    raise ValueError\n",
        "def source_shadowed():\n",
        "    ValueError = CustomValueError\n",
        "    raise ValueError\n",
        "def destination_shadowed():\n",
        "    TypeError = CustomTypeError\n",
        "    raise ValueError\n",
        "def sibling_shadow():\n",
        "    TypeError = CustomTypeError\n",
        "def clean_after():\n",
        "    raise ValueError\n",
        "class Box:\n",
        "    ValueError = CustomValueError\n",
        "    def method(self):\n",
        "        raise ValueError\n",
        "def outer():\n",
        "    ValueError = CustomValueError\n",
        "    def nested():\n",
        "        raise ValueError\n",
    );

    let actual = analyze(source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
        .map(|candidate| (candidate.line, candidate.symbol))
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        [
            (2, Some("clean".to_owned())),
            (12, Some("clean_after".to_owned())),
            (16, Some("Box.method".to_owned())),
        ]
    );
}

#[test]
fn raise_exception_type_pairs_keep_shared_selection_and_profile_behavior() {
    let source = "def selected():\n    raise ValueError\n";
    let full = analyze_with_profile(MutationProfile::Full, 10_000, source);
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
    let summarize = |output: &super::AnalyzerOutput| {
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "exception_type_pair")
            .map(|candidate| {
                (
                    candidate.original.clone(),
                    candidate.replacement.clone(),
                    candidate.line,
                    candidate.symbol.clone(),
                )
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(summarize(&full), summarize(&focused));
    assert_eq!(
        summarize(&full),
        [(
            "ValueError".to_owned(),
            "TypeError".to_owned(),
            2,
            Some("selected".to_owned()),
        )]
    );

    let selected = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 2, end: 2 }],
        &["pkg.sample:selected".to_owned()],
        10_000,
        source,
    );
    assert_eq!(summarize(&selected), summarize(&full));

    let bounded = analyze_with_profile(MutationProfile::Full, 1, "raise KeyError(key)\n");
    assert_eq!(bounded.candidates.len(), 1);
    assert!(bounded.truncated);

    let mut operators = MutationOperatorSelection::default();
    operators.exclude(MutationOperator::ExceptionTypePair);
    let excluded = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert_eq!(
        excluded.candidates,
        Vec::<super::payload::Charged<AnalyzerCandidate>>::new()
    );
}

fn explicitly_selected_risky_exception_operators() -> MutationOperatorSelection {
    let mut operators = MutationOperatorSelection::default();
    for operator in [
        MutationOperator::ExceptionBareToException,
        MutationOperator::ExceptionExceptionToBare,
        MutationOperator::ExceptionBaseBoundary,
        MutationOperator::ExceptionTupleAddPair,
        MutationOperator::ExceptionTupleRemoveMember,
    ] {
        operators.include(operator);
    }
    operators
}

#[test]

fn exception_risky_candidates_require_explicit_selection_and_reparse_scenario_1() {
    let source = concat!(
        "before_list = [before]\n",
        "try:\n    work()\n",
        "except:\n    pass\n",
        "try:\n    work()\n",
        "except Exception:\n    pass\n",
        "try:\n    work()\n",
        "except BaseException:\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError,):\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError, TypeError,):\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError, # keep this comment\n",
        "        TypeError,):\n    pass\n",
        "after_tuple = (after,)\n",
    );
    let default_output = analyze(source);

    assert!(default_output.candidates.iter().all(|candidate| {
        !matches!(
            candidate.operator.as_str(),
            "exception_bare_to_exception"
                | "exception_exception_to_bare"
                | "exception_base_boundary"
                | "exception_tuple_add_pair"
                | "exception_tuple_remove_member"
        )
    }));
}

#[rstest::rstest]
#[case::systemexit_1(0)]
#[case::keyboardinterrupt_2(1)]
#[case::generatorexit_3(2)]

fn exception_risky_candidates_require_explicit_selection_and_reparse_scenario_2(
    #[case] scenario_index: usize,
) {
    let operators = explicitly_selected_risky_exception_operators();

    let termination = ["SystemExit", "KeyboardInterrupt", "GeneratorExit"]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let source = format!("try:\n    work()\nexcept ({termination}, ValueError):\n    pass\n");

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        &source,
    );
    let removals = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "exception_tuple_remove_member")
        .collect::<Vec<_>>();
    assert_eq!(removals.len(), 1);
    let mutated = apply_candidate_and_reparse(&source, removals[0]);
    assert!(
        !mutated.contains(&format!("except ({termination}")),
        "tuple removal generated an individual termination handler: {mutated}"
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the fixture covers every explicit risky exception operator and parseability"
)]
fn exception_risky_candidates_require_explicit_selection_and_reparse() {
    let source = concat!(
        "before_list = [before]\n",
        "try:\n    work()\n",
        "except:\n    pass\n",
        "try:\n    work()\n",
        "except Exception:\n    pass\n",
        "try:\n    work()\n",
        "except BaseException:\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError,):\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError, TypeError,):\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError, # keep this comment\n",
        "        TypeError,):\n    pass\n",
        "after_tuple = (after,)\n",
    );
    let default_output = analyze(source);

    assert!(default_output.candidates.iter().all(|candidate| {
        !matches!(
            candidate.operator.as_str(),
            "exception_bare_to_exception"
                | "exception_exception_to_bare"
                | "exception_base_boundary"
                | "exception_tuple_add_pair"
                | "exception_tuple_remove_member"
        )
    }));

    let operators = explicitly_selected_risky_exception_operators();

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    let risky_candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("exception_"))
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
                candidate.span,
            )
        })
        .collect();

    assert_eq!(
        risky_candidates,
        vec![
            (
                "except",
                "except Exception",
                "exception_bare_to_exception",
                ByteSpan {
                    start: 39,
                    length: 6,
                },
            ),
            (
                "Exception",
                "BaseException",
                "exception_base_boundary",
                ByteSpan {
                    start: 79,
                    length: 9,
                },
            ),
            (
                "Exception",
                "",
                "exception_exception_to_bare",
                ByteSpan {
                    start: 79,
                    length: 9,
                },
            ),
            (
                "BaseException",
                "Exception",
                "exception_base_boundary",
                ByteSpan {
                    start: 122,
                    length: 13,
                },
            ),
            (
                "(ValueError,)",
                "(ValueError, TypeError)",
                "exception_tuple_add_pair",
                ByteSpan {
                    start: 169,
                    length: 13,
                },
            ),
            (
                "(ValueError, TypeError,)",
                "( TypeError,)",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 216,
                    length: 24,
                },
            ),
            (
                "(ValueError, TypeError,)",
                "(ValueError, )",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 216,
                    length: 24,
                },
            ),
            (
                "(ValueError, # keep this comment\n        TypeError,)",
                "( # keep this comment\n        TypeError,)",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 274,
                    length: 52,
                },
            ),
            (
                "(ValueError, # keep this comment\n        TypeError,)",
                "(ValueError, # keep this comment\n        )",
                "exception_tuple_remove_member",
                ByteSpan {
                    start: 274,
                    length: 52,
                },
            ),
        ]
    );
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "exception_bare_to_exception"
            && candidate.original == "except"
            && candidate.replacement == "except Exception"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "exception_exception_to_bare"
            && candidate.original == "Exception"
            && candidate.replacement.is_empty()
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "exception_base_boundary"
            && candidate.original == "Exception"
            && candidate.replacement == "BaseException"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "exception_base_boundary"
            && candidate.original == "BaseException"
            && candidate.replacement == "Exception"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "exception_tuple_add_pair"
            && candidate.original == "(ValueError,)"
            && candidate.replacement == "(ValueError, TypeError)"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "exception_tuple_remove_member"
            && candidate.original == "(ValueError, TypeError,)"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator.as_str() == "exception_tuple_remove_member"
            && candidate.original.contains("# keep this comment")
    }));
    for candidate in output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("exception_"))
    {
        apply_candidate_and_reparse(source, candidate);
    }
    for candidate in output.candidates.iter().filter(|candidate| {
        candidate.operator.as_str().starts_with("exception_")
            && candidate.original.contains("# keep this comment")
    }) {
        let mutated = apply_candidate_and_reparse(source, candidate);

        assert!(mutated.contains("# keep this comment"));
    }

    let unsupported = concat!(
        "ValueError = Custom\n",
        "try:\n    work()\n",
        "except ValueError:\n    pass\n",
        "except module.Error:\n    pass\n",
        "except make_error():\n    pass\n",
        "except (ValueError, module.Error):\n    pass\n",
        "except* BaseException:\n    pass\n",
    );
    assert_eq!(
        analyze_source(
            &AnalyzeRequest {
                payload: super::payload::PayloadBudget::default(),
                path: Utf8Path::new("pkg/sample.py"),
                lines: &[],
                symbols: &[],
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 10_000,
            },
            unsupported,
        )
        .candidates,
        Vec::<super::payload::Charged<AnalyzerCandidate>>::new()
    );

    let shadowed = concat!(
        "TypeError = CustomError\n",
        "Exception = CustomException\n",
        "BaseException = CustomBaseException\n",
        "try:\n    work()\n",
        "except ValueError:\n    pass\n",
        "try:\n    work()\n",
        "except:\n    pass\n",
        "try:\n    work()\n",
        "except Exception:\n    pass\n",
        "try:\n    work()\n",
        "except (ValueError,):\n    pass\n",
    );
    let shadowed_output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        shadowed,
    );
    assert!(
        shadowed_output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.as_str().starts_with("exception_")),
        "unexpected shadowed exception candidates: {:#?}",
        shadowed_output.candidates
    );

    let handler_position = concat!(
        "try:\n    work()\n",
        "except Exception as error:\n    pass\n",
        "try:\n    work()\n",
        "except Exception:\n    pass\n",
        "except TypeError:\n    pass\n",
        "try:\n    work()\n",
        "except TypeError:\n    pass\n",
        "except Exception:\n    pass\n",
    );
    let positioned_output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        handler_position,
    );
    assert_eq!(
        positioned_output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "exception_exception_to_bare")
            .count(),
        1
    );
}

#[test]
fn annotations_do_not_emit_default_bitwise_mutations() {
    let output = analyze("value: Left | None\nresult = left & right\n");

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "bitwise_and_or")
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec!["&"]
    );
}

#[test]
fn annotations_suppress_all_default_token_mutations() {
    let source = concat!(
        "from typing import Annotated, Literal\n",
        "\n",
        "def choose(left: Literal[-1], enabled: Literal[True], size: Annotated[int, 1 + 2]) -> int:\n",
        "    runtime_sign = -value\n",
        "    runtime_flag = True\n",
        "    return left + right\n",
    );

    let output = analyze(source);

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.operator.as_str(),
                    "unary_sign" | "boolean_literal" | "binary_add_sub"
                )
            })
            .map(|candidate| {
                (
                    candidate.operator.as_str(),
                    candidate.original.as_str(),
                    candidate.line,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("unary_sign", "-", 4),
            ("boolean_literal", "True", 5),
            ("binary_add_sub", "+", 6),
        ]
    );
}

#[rstest::rstest]
#[case::type_list_int_result_list_items("type list = int\nresult = list(items)\n")]
#[case::match_value_case_list_pass_result_list_items(
    "match value:\n    case list:\n        pass\nresult = list(items)\n"
)]
fn type_alias_and_match_capture_bindings_shadow_collection_builtins(#[case] source: &str) {
    let output = analyze(source);

    assert!(
        output.candidates.is_empty(),
        "unexpected list candidate for {source:?}: {:#?}",
        output.candidates
    );
}

#[test]
fn structural_candidates_reject_bare_generators_and_preserve_commented_literals() {
    let source = concat!(
        "items.append(value for value in values)\n",
        "mapping.get(value for value in values)\n",
        "items.insert(0, (yield value))\n",
        "items.insert(0, (yield from values))\n",
        "items = [item, # keep this comment\n]\n",
        "other = [item # keep this comment\n,]\n",
        "yielding_value = [(yield value)]\n",
        "yielding = [(yield from values)]\n",
        "named = [item := value]\n",
    );

    let output = analyze(source);

    assert!(
        output.candidates.iter().all(|candidate| {
            !matches!(
                candidate.operator.as_str(),
                "collection_append_insert" | "structure_mapping_get_subscript"
            )
        }),
        "unexpected candidates: {:#?}",
        output.candidates
    );
    for candidate in output.candidates.iter().filter(|candidate| {
        candidate.operator.as_str().starts_with("collection_")
            || candidate.operator.as_str().starts_with("structure_")
    }) {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn annotations_and_star_imports_suppress_collection_candidates() {
    let annotation_output = analyze(
        "from typing import Callable\nhandler: Callable[[Left, Right], Result]\nvalue: tuple[Left, Right]\n",
    );

    assert_eq!(
        annotation_output.candidates,
        Vec::<super::payload::Charged<AnalyzerCandidate>>::new()
    );

    let star_import_output =
        analyze("from helpers import *\nresult = list(items)\nvalue = tuple(items)\n");
    assert_eq!(
        star_import_output.candidates,
        Vec::<super::payload::Charged<AnalyzerCandidate>>::new()
    );
}

#[test]
fn shadowed_collection_builtins_are_not_mutated_as_calls() {
    let source = "any = custom_any\nfrom helpers import all\ndef list(tuple):\n    min = custom_min\n    for max in items:\n        pass\n    with resource as sorted:\n        pass\n    try:\n        pass\n    except Error as reversed:\n        pass\n    if (frozenset := custom_frozenset):\n        return list(items), tuple(items), set(items), frozenset(items), min(items), max(items), sorted(items), reversed(items)\nclass set:\n    pass\n";

    let output = analyze(source);

    assert!(
        output.candidates.iter().all(|candidate| {
            !matches!(
                candidate.original.as_str(),
                "any"
                    | "all"
                    | "list"
                    | "tuple"
                    | "set"
                    | "frozenset"
                    | "min"
                    | "max"
                    | "sorted"
                    | "reversed"
            )
        }),
        "shadowed builtins must not emit name-replacement candidates: {:#?}",
        output.candidates
    );
}

#[test]
fn sibling_bindings_do_not_suppress_builtin_or_exception_pairs() {
    let source = concat!(
        "def convert(items):\n",
        "    return list(items)\n",
        "def handle():\n",
        "    try:\n",
        "        work()\n",
        "    except ValueError:\n",
        "        recover()\n",
        "def helper():\n",
        "    list = custom_list\n",
        "    tuple = custom_tuple\n",
        "    TypeError = CustomTypeError\n",
    );

    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "collection_list_tuple" | "exception_type_pair"
            )
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            ("list", "tuple", 2, Some("convert")),
            ("ValueError", "TypeError", 6, Some("handle")),
        ]
    );
}

#[test]
fn visible_source_or_destination_binding_suppresses_builtin_pairs() {
    let source = concat!(
        "def source_shadowed(items):\n",
        "    result = list(items)\n",
        "    list = custom_list\n",
        "    return result\n",
        "def clean(items):\n",
        "    return list(items)\n",
    );

    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .map(|candidate| (candidate.line, candidate.symbol.as_deref()))
        .collect::<Vec<_>>();

    assert_eq!(observed, [(6, Some("clean"))]);
}

#[test]
fn shadowed_destination_suppresses_an_otherwise_builtin_source() {
    let source = concat!(
        "def convert(items):\n",
        "    tuple = custom_tuple\n",
        "    return list(items)\n",
    );

    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "collection_list_tuple"),
        "shadowed replacement destination must suppress the candidate: {:#?}",
        output.candidates
    );
}

#[test]
fn loop_back_edge_bindings_suppress_builtin_pair_candidates() {
    let source_binding = analyze(concat!(
        "for item in values:\n",
        "    current = sorted(item)\n",
        "    sorted = fake_sorted\n",
    ));

    assert!(
        source_binding
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "structure_sorted_reversed"),
        "a source binding on the back edge must suppress the pair: {:#?}",
        source_binding.candidates
    );

    let destination_binding = analyze(concat!(
        "for item in values:\n",
        "    current = sorted(item)\n",
        "    reversed = fake_reversed\n",
    ));
    assert!(
        destination_binding
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "structure_sorted_reversed"),
        "a destination binding on the back edge must suppress the pair: {:#?}",
        destination_binding.candidates
    );

    let target_load = analyze(concat!(
        "for sink[sorted(item)] in values:\n",
        "    sorted = fake_sorted\n",
    ));
    assert!(
        target_load
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "structure_sorted_reversed"),
        "a repeated target load must observe body bindings on the back edge: {:#?}",
        target_load.candidates
    );

    let one_time_iterable = analyze(concat!(
        "for item in sorted(values):\n",
        "    sorted = fake_sorted\n",
    ));
    assert_eq!(
        one_time_iterable
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "structure_sorted_reversed")
            .map(|candidate| candidate.line)
            .collect::<Vec<_>>(),
        [1],
        "the for iterable is evaluated before, not on, the back edge"
    );

    let class_nested_function = analyze(concat!(
        "class Namespace:\n",
        "    for item in values:\n",
        "        def helper():\n",
        "            return sorted(item)\n",
        "        sorted = fake_sorted\n",
    ));
    assert_eq!(
        class_nested_function
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "structure_sorted_reversed")
            .map(|candidate| (candidate.line, candidate.symbol.as_deref()))
            .collect::<Vec<_>>(),
        [(4, Some("Namespace.helper"))],
        "a nested function does not close over its parent class namespace"
    );
}

#[test]
fn module_class_closure_and_directive_resolution_is_scope_aware() {
    let source = concat!(
        "early = list(items)\n",
        "list = custom_list\n",
        "late = list(items)\n",
        "def outer(items):\n",
        "    max = custom_max\n",
        "    def inner():\n",
        "        return max(items)\n",
        "    return inner\n",
        "all = custom_all\n",
        "def redirected(items):\n",
        "    global all\n",
        "    return all(items)\n",
        "def enclosing(items):\n",
        "    min = custom_min\n",
        "    def nested():\n",
        "        nonlocal min\n",
        "        return min(items)\n",
        "    return nested\n",
    );

    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "collection_list_tuple" | "collection_min_max" | "collection_any_all"
            )
        })
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list", "tuple", 1, None)]);

    let class_source = concat!(
        "class Box:\n",
        "    early = tuple(items)\n",
        "    tuple = custom_tuple\n",
        "    late = tuple(items)\n",
        "    def method(self, items):\n",
        "        return tuple(items)\n",
    );
    let class_output = analyze(class_source);
    let class_observed = class_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .map(|candidate| (candidate.line, candidate.symbol.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(class_observed, [(2, Some("Box")), (6, Some("Box.method"))]);
}

#[test]
fn comprehension_exception_target_and_wildcard_boundaries_are_conservative() {
    let source = concat!(
        "outer = [item for item in list(items)]\n",
        "inner = [list(item) for list in factories]\n",
        "before = tuple(items)\n",
        "try:\n",
        "    work()\n",
        "except Error as tuple:\n",
        "    inside = tuple(items)\n",
        "after = tuple(items)\n",
        "def local_target(items):\n",
        "    before = tuple(items)\n",
        "    try:\n",
        "        work()\n",
        "    except Error as tuple:\n",
        "        inside = tuple(items)\n",
        "    after = tuple(items)\n",
    );

    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            ("list", "tuple", 1, None),
            ("tuple", "list", 3, None),
            ("tuple", "list", 8, None),
        ]
    );

    let wildcard = analyze("from helpers import *\nresult = list(items)\n");
    assert_eq!(
        wildcard.candidates,
        Vec::<super::payload::Charged<AnalyzerCandidate>>::new()
    );
}

#[test]
fn class_exception_targets_do_not_leak_into_methods() {
    let source = concat!(
        "class Box:\n",
        "    try:\n",
        "        work()\n",
        "    except Error as list:\n",
        "        def class_handler(self, items):\n",
        "            return list(items)\n",
    );

    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list", "tuple", 6, Some("Box.class_handler"))]);

    let immediate_class = analyze(concat!(
        "try:\n",
        "    work()\n",
        "except Error as list:\n",
        "    class Immediate:\n",
        "        value = list(items)\n",
    ));
    assert!(
        immediate_class
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "collection_list_tuple")
    );
}

#[test]
fn imports_are_source_ordered_and_lambda_parameters_are_scope_local() {
    let import_source = concat!(
        "before = list(items)\n",
        "from helpers import list\n",
        "after = list(items)\n",
    );
    let import_output = analyze(import_source);
    let import_observed = import_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(import_observed, [("list", "tuple", 1)]);

    let lambda_output = analyze(concat!(
        "clean = lambda items: tuple(items)\n",
        "shadowed = lambda tuple, items: tuple(items)\n",
    ));
    let lambda_observed = lambda_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(lambda_observed, [("tuple", "list", 1)]);
}

#[test]
fn dynamic_binding_operations_fail_closed_after_their_possible_effect() {
    let source = concat!(
        "before = list(items)\n",
        "exec(\"list = custom_list\")\n",
        "after = list(items)\n",
    );

    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .map(|candidate| (candidate.original.as_str(), candidate.line))
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list", 1)]);

    let dynamic_namespace = analyze(concat!(
        "globals()[\"tuple\"] = custom_tuple\n",
        "result = list(items)\n",
    ));
    assert!(
        dynamic_namespace
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "collection_list_tuple")
    );
}

#[test]
fn collection_calls_and_literals_emit_exact_parseable_candidates() {
    let source = concat!(
        "any_result = any(items)\n",
        "all_result = all(items)\n",
        "list_result = list(values)\n",
        "tuple_result = tuple(values)\n",
        "empty_list_call = list()\n",
        "empty_tuple_call = tuple()\n",
        "set_result = set(values)\n",
        "frozen_result = frozenset(values)\n",
        "minimum = min(first, second, key=rank)\n",
        "maximum = max(values, default=fallback)\n",
        "items.append(value)\n",
        "items.insert(0, value)\n",
        "members.add(value)\n",
        "members.discard(value)\n",
        "members.remove(value)\n",
        "text.startswith(prefix)\n",
        "text.endswith(suffix, start, stop)\n",
        "text.split(separator, maxsplit=limit)\n",
        "text.rsplit(None, 1)\n",
        "many = [first, *rest, last,]\n",
        "one = [item]\n",
        "empty_list = []\n",
        "tuple_many = (first, *rest, last,)\n",
        "tuple_one = (item,)\n",
        "empty_tuple = ()\n",
        "bare_tuple = first, *rest,\n",
    );

    let output = analyze(source);
    let collection_candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("collection_"))
        .collect();

    let actual: Vec<_> = collection_candidates
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
        actual,
        vec![
            ("any", "all", "collection_any_all"),
            ("all", "any", "collection_any_all"),
            ("list", "tuple", "collection_list_tuple"),
            ("tuple", "list", "collection_list_tuple"),
            ("list", "tuple", "collection_list_tuple"),
            ("tuple", "list", "collection_list_tuple"),
            ("set", "frozenset", "collection_set_frozenset"),
            ("frozenset", "set", "collection_set_frozenset"),
            ("min", "max", "collection_min_max"),
            ("max", "min", "collection_min_max"),
            (
                "items.append(value)",
                "items.insert(0, value)",
                "collection_append_insert",
            ),
            (
                "items.insert(0, value)",
                "items.append(value)",
                "collection_append_insert",
            ),
            ("add", "discard", "collection_set_add_discard"),
            ("discard", "add", "collection_set_add_discard"),
            ("discard", "remove", "collection_set_remove_discard"),
            ("remove", "discard", "collection_set_remove_discard"),
            ("startswith", "endswith", "collection_string_starts_ends"),
            ("endswith", "startswith", "collection_string_starts_ends"),
            ("split", "rsplit", "collection_string_split_rsplit"),
            ("rsplit", "split", "collection_string_split_rsplit"),
            (
                "[first, *rest, last,]",
                "(first, *rest, last,)",
                "collection_list_tuple",
            ),
            ("[item]", "(item,)", "collection_list_tuple"),
            ("[]", "()", "collection_list_tuple"),
            (
                "(first, *rest, last,)",
                "[first, *rest, last,]",
                "collection_list_tuple",
            ),
            ("(item,)", "[item,]", "collection_list_tuple"),
            ("()", "[]", "collection_list_tuple"),
            ("first, *rest,", "[first, *rest,]", "collection_list_tuple",),
        ]
    );

    for candidate in collection_candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn split_rsplit_candidates_require_explicit_maxsplit() {
    let source = concat!(
        "text.split()\n",
        "text.split(',')\n",
        "text.rsplit()\n",
        "text.rsplit(',')\n",
        "text.split(',', 1)\n",
        "text.rsplit(',', 1)\n",
        "text.split(',', maxsplit=1)\n",
        "text.rsplit(maxsplit=1)\n",
    );

    let output = analyze(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_string_split_rsplit")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            ("split", "rsplit", 5),
            ("rsplit", "split", 6),
            ("split", "rsplit", 7),
            ("rsplit", "split", 8),
        ]
    );
}

#[rstest::rstest]
#[case::x_1_2_1(0)]
#[case::y_1_2_3_4_2(1)]
#[case::v_d_1_2_3(2)]
#[case::wrapped_1_2_4(3)]
fn tuple_to_list_preserves_parenthesized_boundary_elements_in_unparenthesized_tuples(
    #[case] scenario_index: usize,
) {
    let cases = [
        ("x = (1), (2)\n", "(1), (2)", "[(1), (2)]"),
        (
            "y = (1 + 2), (3 + 4)\n",
            "(1 + 2), (3 + 4)",
            "[(1 + 2), (3 + 4)]",
        ),
        ("v = d[(1), (2)]\n", "(1), (2)", "[(1), (2)]"),
        ("wrapped = ((1), (2))\n", "((1), (2))", "[(1), (2)]"),
    ];

    let (source, original, replacement) = cases.into_iter().nth(scenario_index).unwrap();

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_list_tuple")
        .collect();

    assert_eq!(
        candidates.len(),
        1,
        "unexpected candidates: {candidates:#?}"
    );
    assert_eq!(candidates[0].original, original);
    assert_eq!(candidates[0].replacement, replacement);
    apply_candidate_and_reparse(source, candidates[0]);
}

#[test]
fn collection_excludes_unsupported_call_and_literal_shapes() {
    let source = concat!(
        "list_comp = [item for item in items]\n",
        "set_comp = {item for item in items}\n",
        "left, right = values\n",
        "any()\n",
        "any(first, second)\n",
        "list(iterable=items)\n",
        "tuple(*items)\n",
        "set(iterable=items)\n",
        "frozenset(*items)\n",
        "items.insert(1, value)\n",
        "items.insert(index, value)\n",
        "items.sort(reverse=True)\n",
        "members.add(*values)\n",
        "members.discard(**options)\n",
        "members.remove(value, extra)\n",
        "text.startswith(*parts)\n",
        "text.endswith(**options)\n",
        "text.split(*parts, maxsplit=1)\n",
        "text.rsplit(None, 1, **options)\n",
        "set_literal = {first, second}\n",
    );

    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.as_str().starts_with("collection_")),
        "unexpected collection candidates: {:#?}",
        output.candidates
    );
}

#[test]
fn structure_calls_emit_exact_parseable_candidates() {
    let source = concat!(
        "appended = items.append(value)\n",
        "extended = items.extend([value])\n",
        "got = mapping.get(key)\n",
        "subscripted = mapping[key]\n",
        "sorted_items = items.sort()\n",
        "reversed_items = items.reverse()\n",
        "ordered = sorted(items)\n",
        "flipped = reversed(items)\n",
    );

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("structure_"))
        .collect();

    let actual: Vec<_> = candidates
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
        actual,
        vec![
            (
                "items.append(value)",
                "items.extend([value])",
                "structure_append_extend",
            ),
            (
                "items.extend([value])",
                "items.append(value)",
                "structure_append_extend",
            ),
            (
                "mapping.get(key)",
                "mapping[key]",
                "structure_mapping_get_subscript",
            ),
            (
                "mapping[key]",
                "mapping.get(key)",
                "structure_mapping_get_subscript",
            ),
            ("items.sort()", "items.reverse()", "structure_sort_reverse",),
            ("items.reverse()", "items.sort()", "structure_sort_reverse",),
            ("sorted", "reversed", "structure_sorted_reversed"),
            ("reversed", "sorted", "structure_sorted_reversed"),
        ]
    );

    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_replacements_preserve_nested_sources_and_reparse() {
    let source = concat!(
        "appended = items().append(\n",
        "    \"value\"  # retained\n",
        ")\n",
        "extended = items().extend([\n",
        "    \"value\"  # retained\n",
        "])\n",
        "combined = container.mapping.get((make_key(\"field\"))) + 1  # retained\n",
        "subscripted = container.mapping[(make_key(\"field\"))]\n",
        "sorted_result = sorted(produce_items())\n",
        "reversed_result = reversed(produce_items())\n",
    );

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("structure_"))
        .collect();

    let actual: Vec<_> = candidates
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
        actual,
        vec![
            (
                "items().append(\n    \"value\"  # retained\n)",
                "items().extend([\n    \"value\"  # retained\n])",
                "structure_append_extend",
            ),
            (
                "items().extend([\n    \"value\"  # retained\n])",
                "items().append(\n    \"value\"  # retained\n)",
                "structure_append_extend",
            ),
            (
                "container.mapping.get((make_key(\"field\")))",
                "container.mapping[(make_key(\"field\"))]",
                "structure_mapping_get_subscript",
            ),
            (
                "container.mapping[(make_key(\"field\"))]",
                "container.mapping.get((make_key(\"field\")))",
                "structure_mapping_get_subscript",
            ),
            ("sorted", "reversed", "structure_sorted_reversed"),
            ("reversed", "sorted", "structure_sorted_reversed"),
        ]
    );

    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_replacements_preserve_grouped_calls_and_mapping_delimiters() {
    let source = concat!(
        "sort_grouped = (items.sort)()\n",
        "reverse_grouped = (items.reverse)()\n",
        "nested_sort = ((items.sort))()\n",
        "got = (d\n    .data).get((first,\n    second))\n",
        "subscripted = (d\n    .data)[(first,\n    second)]\n",
        "commented = (d # [ receiver comment\n)[key]\n",
        "nested_receiver = obj.items[0].data[key]\n",
    );
    assert_structural_replacements(
        source,
        &[
            (
                "(items.sort)()",
                "(items.reverse)()",
                "structure_sort_reverse",
            ),
            (
                "(items.reverse)()",
                "(items.sort)()",
                "structure_sort_reverse",
            ),
            (
                "((items.sort))()",
                "((items.reverse))()",
                "structure_sort_reverse",
            ),
            (
                "(d\n    .data).get((first,\n    second))",
                "(d\n    .data)[(first,\n    second)]",
                "structure_mapping_get_subscript",
            ),
            (
                "(d\n    .data)[(first,\n    second)]",
                "(d\n    .data).get((first,\n    second))",
                "structure_mapping_get_subscript",
            ),
            (
                "(d # [ receiver comment\n)[key]",
                "(d # [ receiver comment\n).get(key)",
                "structure_mapping_get_subscript",
            ),
            (
                "obj.items[0].data[key]",
                "obj.items[0].data.get(key)",
                "structure_mapping_get_subscript",
            ),
            (
                "obj.items[0]",
                "obj.items.get(0)",
                "structure_mapping_get_subscript",
            ),
        ],
    );
}

#[test]
fn structure_replacements_preserve_grouped_collection_arguments() {
    let source = concat!(
        "appended = (items\n    .append)(value)\n",
        "extended = (items\n    .extend)([value])\n",
        "inserted = (items\n    .insert)(0, value)\n",
        "parenthesized_append = items.append((value))\n",
        "parenthesized_insert = items.insert((0), ((value)))\n",
        "commented_append = items.append((\n    value # kept\n))\n",
        "commented_insert = items.insert(\n    (0), # kept\n    ((value))\n)\n",
        "trailing_comma_extend = items.extend([value,],)\n",
        "grouped_extend = items.extend(([value,]))\n",
        "tuple_extend = items.extend([(value,),])\n",
        "commented_extend = items.extend([\n    value, # kept\n])\n",
    );
    assert_structural_replacements(
        source,
        &[
            (
                "(items\n    .append)(value)",
                "(items\n    .insert)(0, value)",
                "collection_append_insert",
            ),
            (
                "(items\n    .append)(value)",
                "(items\n    .extend)([value])",
                "structure_append_extend",
            ),
            (
                "(items\n    .extend)([value])",
                "(items\n    .append)(value)",
                "structure_append_extend",
            ),
            (
                "(items\n    .insert)(0, value)",
                "(items\n    .append)(value)",
                "collection_append_insert",
            ),
            (
                "items.append((value))",
                "items.insert(0, (value))",
                "collection_append_insert",
            ),
            (
                "items.append((value))",
                "items.extend([(value)])",
                "structure_append_extend",
            ),
            (
                "items.insert((0), ((value)))",
                "items.append(((value)))",
                "collection_append_insert",
            ),
            (
                "items.append((\n    value # kept\n))",
                "items.insert(0, (\n    value # kept\n))",
                "collection_append_insert",
            ),
            (
                "items.append((\n    value # kept\n))",
                "items.extend([(\n    value # kept\n)])",
                "structure_append_extend",
            ),
            (
                "items.insert(\n    (0), # kept\n    ((value))\n)",
                "items.append(\n    # kept\n    ((value))\n)",
                "collection_append_insert",
            ),
            (
                "items.extend([value,],)",
                "items.append(value,)",
                "structure_append_extend",
            ),
            (
                "items.extend(([value,]))",
                "items.append((value))",
                "structure_append_extend",
            ),
            (
                "items.extend([(value,),])",
                "items.append((value,))",
                "structure_append_extend",
            ),
            (
                "items.extend([\n    value, # kept\n])",
                "items.append(\n    value # kept\n)",
                "structure_append_extend",
            ),
        ],
    );
}

fn assert_structural_replacements(source: &str, expected: &[(&str, &str, &str)]) {
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "structure_append_extend"
                    | "structure_mapping_get_subscript"
                    | "structure_sort_reverse"
                    | "collection_append_insert"
            )
        })
        .collect();
    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(actual, expected);

    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_replacements_keep_crlf_unicode_boundaries() {
    let source = "é = (items.sort)()\r\nvalue = (mapping\r\n    .data)[key]\r\n";

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "structure_mapping_get_subscript" | "structure_sort_reverse"
            )
        })
        .collect();

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("(items.sort)()", "(items.reverse)()"),
            (
                "(mapping\r\n    .data)[key]",
                "(mapping\r\n    .data).get(key)",
            ),
        ]
    );

    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_candidates_skip_type_annotation_expressions() {
    let source = concat!(
        "value: mapping[key]\n",
        "fallback: mapping.get(key)\n",
        "def annotated(parameter: mapping[key]) -> mapping.get(key):\n",
        "    pass\n",
        "result = mapping[key]\n",
    );

    let output = analyze(source);

    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("structure_"))
        .collect();

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec!["mapping[key]"],
    );
    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn existing_structure_families_exclude_unsupported_shapes_and_receivers() {
    let source = concat!(
        "items.append(*values)\n",
        "items.append(value=value)\n",
        "items.extend(values)\n",
        "items.extend([first, second])\n",
        "items.extend([*values])\n",
        "items.extend([value], extra)\n",
        "items.extend(values=[value])\n",
        "mapping.get(key, fallback)\n",
        "mapping.get(key,)\n",
        "mapping.get(key,  # trailing comma\n",
        ")\n",
        "mapping.get(key=key)\n",
        "mapping.get(*keys)\n",
        "mapping.get(**options)\n",
        "factory().get(key)\n",
        "factory()[key]\n",
        "mapping[first, second]\n",
        "mapping[*keys]\n",
        "mapping[key:stop]\n",
        "mapping[key] = value\n",
        "del mapping[key]\n",
        "items.sort(key=rank)\n",
        "items.sort(reverse=True)\n",
        "items.reverse(*values)\n",
        "items.reverse(values=values)\n",
        "sorted(items, key=rank)\n",
        "sorted(iterable=items)\n",
        "reversed(items, extra)\n",
        "reversed(iterable=items)\n",
        "sorted(*items)\n",
    );
    // A variable-bound slice remains unsupported by numeric neighbors and
    // mapping-get replacement; the dedicated bound operators support it.
    let output = analyze_selected(
        &[
            "structure_append_extend",
            "structure_mapping_get_subscript",
            "structure_sort_reverse",
            "structure_sorted_reversed",
            "structure_index_neighbor",
            "structure_slice_neighbor",
        ],
        source,
    );

    assert!(
        output.candidates.is_empty(),
        "unexpected structural candidates: {:#?}",
        output.candidates
    );
}

#[test]
fn candidate_punctuation_queries_are_range_bounded() {
    use std::fmt::Write;

    const SAMPLES: usize = 64;
    let mut get_source = String::new();
    let mut list_source = String::new();
    let mut tuple_add_source = String::new();
    let mut tuple_remove_source = String::new();
    for index in 0..SAMPLES {
        writeln!(get_source, "got_{index} = mapping.get(key_{index})").unwrap();
        writeln!(list_source, "one_{index} = [item_{index}]").unwrap();
        tuple_add_source.push_str("try:\n    work()\nexcept (ValueError,):\n    pass\n");
        tuple_remove_source
            .push_str("try:\n    work()\nexcept (ValueError, TypeError):\n    pass\n");
    }

    assert_bounded_candidate_token_lookups(
        &analyze(&get_source),
        "structure_mapping_get_subscript",
        SAMPLES,
        SAMPLES,
        2,
    );
    assert_bounded_candidate_token_lookups(
        &analyze(&list_source),
        "collection_list_tuple",
        SAMPLES,
        SAMPLES,
        2,
    );
    assert_bounded_candidate_token_lookups(
        &analyze_with_extra_operators(
            &tuple_add_source,
            &[MutationOperator::ExceptionTupleAddPair],
        ),
        "exception_tuple_add_pair",
        SAMPLES,
        SAMPLES,
        5,
    );
    assert_bounded_candidate_token_lookups(
        &analyze_with_extra_operators(
            &tuple_remove_source,
            &[MutationOperator::ExceptionTupleRemoveMember],
        ),
        "exception_tuple_remove_member",
        SAMPLES * 2,
        SAMPLES * 2,
        5,
    );
}

fn analyze_with_extra_operators(
    source: &str,
    extra_operators: &[MutationOperator],
) -> super::AnalyzerOutput {
    let mut operators = MutationOperatorSelection::default();
    for operator in extra_operators {
        operators.include(*operator);
    }
    analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/generated.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    )
}

fn assert_bounded_candidate_token_lookups(
    output: &super::AnalyzerOutput,
    operator: &str,
    expected_candidates: usize,
    expected_lookups: usize,
    max_tokens_per_lookup: usize,
) {
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == operator)
            .count(),
        expected_candidates,
        "unexpected {operator} candidate count"
    );
    assert_eq!(
        output.candidate_token_lookups.lookups, expected_lookups,
        "{operator} did not route every punctuation query through the bounded accessor"
    );
    assert!(
        output.candidate_token_lookups.tokens_examined <= expected_lookups * max_tokens_per_lookup,
        "{operator} lookups examined unrelated module tokens: {:?}",
        output.candidate_token_lookups
    );
}

#[test]
fn bitwise_and_or() {
    let source = "and_result = left & right\nor_result = left | right\n";

    let output = analyze(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "bitwise_and_or")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![("&", "|", "bitwise_and_or"), ("|", "&", "bitwise_and_or"),]
    );
}

#[test]
fn bitwise_shift() {
    let source = "left_result = value << amount\nright_result = value >> amount\n";

    let output = analyze(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "bitwise_shift")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![("<<", ">>", "bitwise_shift"), (">>", "<<", "bitwise_shift"),]
    );
}

#[test]
fn structure_index_neighbor_mutates_decimal_load_indices_only() {
    let source = concat!(
        "zero = items[0]\n",
        "one = items[1]\n",
        "largest = items[18446744073709551615]\n",
        "negative = items[-1]\n",
        "expression = items[index + 1]\n",
        "hexadecimal = items[0x10]\n",
        "underscored = items[1_000]\n",
        "annotation: items[1]\n",
        "assigned[1] = value\n",
        "del deleted[1]\n",
    );

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "structure_index_neighbor")
        .collect();

    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();

    assert_eq!(
        actual,
        vec![
            ("0", "1"),
            ("1", "2"),
            ("1", "0"),
            ("18446744073709551615", "18446744073709551614"),
            ("-1", "0"),
            ("-1", "-2"),
        ]
    );
    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn structure_slice_neighbor_mutates_decimal_bounds_without_zero_steps() {
    let source = concat!(
        "all_bounds = items[1:3:1]\n",
        "empty_start_and_step = items[:3:]\n",
        "empty_bounds = items[:]\n",
        "expression = items[start:stop:step]\n",
        "negative = items[-1:-3:-1]\n",
        "hexadecimal = items[0x10:0x20:0x1]\n",
        "underscored = items[1_000:2_000:3_000]\n",
        "annotation: items[1:3:1]\n",
        "assigned[1:3:1] = values\n",
        "del deleted[1:3:1]\n",
    );

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "structure_slice_neighbor")
        .collect();

    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();

    assert_eq!(
        actual,
        vec![
            ("1", "2"),
            ("1", "0"),
            ("3", "4"),
            ("3", "2"),
            ("1", "2"),
            ("3", "4"),
            ("3", "2"),
            ("-1", "0"),
            ("-1", "-2"),
            ("-3", "-2"),
            ("-3", "-4"),
            ("-1", "-2"),
        ]
    );
    for candidate in candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn collection_literals_and_inner_token_candidates_remain_individually_parseable() {
    let source = "value = [a == b]\n";

    let output = analyze(source);
    let candidates: Vec<_> = output
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
        candidates,
        vec![
            ("[a == b]", "(a == b,)", "collection_list_tuple"),
            ("==", "!=", "compare_eq_ne"),
        ]
    );
    assert_candidates_reparse(source, &output.candidates);
}

#[test]
fn candidate_replacements_reparse_as_python() {
    let source = "result = left == right\n";

    let output = analyze(source);
    let candidate = output
        .candidates
        .iter()
        .find(|candidate| candidate.operator.as_str() == "compare_eq_ne")
        .expect("comparison candidate");

    assert_eq!(
        apply_candidate_and_reparse(source, candidate),
        "result = left != right\n"
    );
}

const TOKEN_OPERATOR_NAMES: &[&str] = &[
    "augmented_add_sub",
    "augmented_floor_mod",
    "augmented_mul_div",
    "binary_add_sub",
    "binary_floor_mod",
    "binary_mul_div",
    "bitwise_and_or",
    "bitwise_shift",
    "boolean_and_or",
    "boolean_literal",
    "break_continue",
    "compare_eq_ne",
    "compare_order",
    "identity",
    "membership",
    "remove_not",
    "unary_sign",
];

const NATIVE_PYTHON_OPERATOR_NAMES: &[&str] = &[
    "binary_power",
    "binary_matmul",
    "augmented_power",
    "augmented_matmul",
    "bitwise_xor",
    "bitwise_invert",
    "augmented_bitwise_and_or",
    "augmented_bitwise_xor",
    "augmented_bitwise_shift",
];

#[rstest::rstest]
#[case::def_calculate_a_b_return_a_b(
    "def calculate(a, b):\n    return a ** b\n",
    "binary_power",
    "**",
    "*"
)]
#[case::def_calculate_a_b_return_a_b_2(
    "def calculate(a, b):\n    return a @ b\n",
    "binary_matmul",
    "@",
    "*"
)]
#[case::def_calculate_a_b_return_a_b_3(
    "def calculate(a, b):\n    return a ^ b\n",
    "bitwise_xor",
    "^",
    "&"
)]
#[case::def_calculate_a_return_a("def calculate(a):\n    return ~a\n", "bitwise_invert", "~", "+")]
#[case::def_calculate_a_b_a_b_return_a(
    "def calculate(a, b):\n    a **= b\n    return a\n",
    "augmented_power",
    "**=",
    "*="
)]
#[case::def_calculate_a_b_a_b_return_a_6(
    "def calculate(a, b):\n    a @= b\n    return a\n",
    "augmented_matmul",
    "@=",
    "*="
)]
#[case::def_calculate_a_b_a_b_return_a_7(
    "def calculate(a, b):\n    a &= b\n    return a\n",
    "augmented_bitwise_and_or",
    "&=",
    "|="
)]
#[case::def_calculate_a_b_a_b_return_a_8(
    "def calculate(a, b):\n    a |= b\n    return a\n",
    "augmented_bitwise_and_or",
    "|=",
    "&="
)]
#[case::def_calculate_a_b_a_b_return_a_9(
    "def calculate(a, b):\n    a ^= b\n    return a\n",
    "augmented_bitwise_xor",
    "^=",
    "&="
)]
#[case::def_calculate_a_b_a_b_return_a_10(
    "def calculate(a, b):\n    a <<= b\n    return a\n",
    "augmented_bitwise_shift",
    "<<=",
    ">>="
)]
#[case::def_calculate_a_b_a_b_return_a_11(
    "def calculate(a, b):\n    a >>= b\n    return a\n",
    "augmented_bitwise_shift",
    ">>=",
    "<<="
)]
fn native_python_operator_syntax_uses_exact_ast_roles_and_reparses(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] original: &str,
    #[case] replacement: &str,
) {
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([operator])).unwrap();

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );

    assert_eq!(output.candidates.len(), 1, "operator: {operator}");
    let candidate = &output.candidates[0];
    assert_eq!(candidate.operator.as_str(), operator);
    assert_eq!(candidate.original, original);
    assert_eq!(candidate.replacement, replacement);
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[start..end], original);
    apply_candidate_and_reparse(source, candidate);
}

#[test]
fn native_python_operator_spellings_outside_operator_roles_are_ignored() {
    let source = concat!(
        "@decorate\n",
        "def collect(**kwargs):\n",
        "    value: Left @ Right\n",
        "    text = '** @ ^ ~ **= @= &= |= ^= <<= >>='\n",
        "    # ** @ ^ ~ **= @= &= |= ^= <<= >>=\n",
        "    return kwargs\n",
    );

    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !NATIVE_PYTHON_OPERATOR_NAMES.contains(&candidate.operator.as_str()))
    );
}

#[test]
fn native_python_operator_syntax_obeys_line_symbol_and_candidate_limits() {
    let source = concat!(
        "def selected(a, b):\n",
        "    return a ** b\n",
        "def ignored(a, b):\n",
        "    return a @ b\n",
    );
    let selected = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 2, end: 2 }],
        &["pkg.sample:selected".to_owned()],
        10_000,
        source,
    );

    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(selected.candidates[0].operator.as_str(), "binary_power");
    assert_eq!(selected.candidates[0].symbol.as_deref(), Some("selected"));

    let bounded = analyze_with(Utf8Path::new("pkg/sample.py"), &[], &[], 1, source);
    assert_eq!(bounded.candidates.len(), 1);
    assert_eq!(bounded.candidates[0].operator.as_str(), "binary_power");
    assert!(bounded.truncated);
}

#[test]
fn token_operator_candidates_cover_supported_ast_roles_and_reparse() {
    let source = concat!(
        "equal = left == right\n",
        "unequal = left != right\n",
        "ordered = first < second <= third > fourth >= fifth\n",
        "member = item in items\n",
        "not_member = item not in items\n",
        "same = left is right\n",
        "not_same = left is not right\n",
        "both = left and right\n",
        "either = left or right\n",
        "sum_value = left + right - extra\n",
        "positive = +value\n",
        "negative = -value\n",
        "product = left * right / divisor\n",
        "remainder = left // right % divisor\n",
        "bits = left & right | extra\n",
        "shifted = left << right >> extra\n",
        "value += increment\n",
        "value -= decrement\n",
        "value *= factor\n",
        "value /= divisor\n",
        "value //= divisor\n",
        "value %= modulus\n",
        "inverted = not value\n",
        "truth = True\n",
        "falsity = False\n",
        "while active:\n    break\n",
        "while pending:\n    continue\n",
    );

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| TOKEN_OPERATOR_NAMES.contains(&candidate.operator.as_str()))
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
            ("==", "!=", "compare_eq_ne"),
            ("!=", "==", "compare_eq_ne"),
            ("<", "<=", "compare_order"),
            ("<=", "<", "compare_order"),
            (">", ">=", "compare_order"),
            (">=", ">", "compare_order"),
            ("in", "not in", "membership"),
            ("not in", "in", "membership"),
            ("is", "is not", "identity"),
            ("is not", "is", "identity"),
            ("and", "or", "boolean_and_or"),
            ("or", "and", "boolean_and_or"),
            ("+", "-", "binary_add_sub"),
            ("-", "+", "binary_add_sub"),
            ("+", "-", "unary_sign"),
            ("-", "+", "unary_sign"),
            ("*", "/", "binary_mul_div"),
            ("/", "*", "binary_mul_div"),
            ("//", "%", "binary_floor_mod"),
            ("%", "//", "binary_floor_mod"),
            ("&", "|", "bitwise_and_or"),
            ("|", "&", "bitwise_and_or"),
            ("<<", ">>", "bitwise_shift"),
            (">>", "<<", "bitwise_shift"),
            ("+=", "-=", "augmented_add_sub"),
            ("-=", "+=", "augmented_add_sub"),
            ("*=", "/=", "augmented_mul_div"),
            ("/=", "*=", "augmented_mul_div"),
            ("//=", "%=", "augmented_floor_mod"),
            ("%=", "//=", "augmented_floor_mod"),
            ("not value", "value", "remove_not"),
            ("True", "False", "boolean_literal"),
            ("False", "True", "boolean_literal"),
            ("break", "continue", "break_continue"),
            ("continue", "break", "break_continue"),
        ]
    );
    for candidate in output
        .candidates
        .iter()
        .filter(|candidate| TOKEN_OPERATOR_NAMES.contains(&candidate.operator.as_str()))
    {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn match_boolean_patterns_mutate_without_admitting_other_pattern_tokens() {
    let selected_operators: &[&str] =
        &["boolean_literal", "collection_list_tuple", "bitwise_and_or"];
    let source = concat!(
        "def classify(value):\n",
        "    match value:\n",
        "        case True:\n            result = 'False'\n",
        "        case False:\n            result = 'True'\n",
        "        case [True, False]:\n            pass\n",
        "        case {'enabled': True}:\n            pass\n",
        "        case True | False:\n            pass\n",
        "        case None:\n            pass\n",
        "        case 'True':\n            pass\n",
        "        case _:\n            pass\n",
        "def capture(other):\n",
        "    match other:\n",
        "        case captured:\n            pass\n",
    );

    let output = analyze_selected(selected_operators, source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.span.length,
                candidate.line,
                candidate.column,
                candidate.symbol.as_deref(),
            )
        })
        .collect();

    assert_eq!(
        candidates,
        vec![
            ("True", "False", 4, 3, 13, Some("classify")),
            ("False", "True", 5, 5, 13, Some("classify")),
            ("True", "False", 4, 7, 14, Some("classify")),
            ("False", "True", 5, 7, 20, Some("classify")),
            ("True", "False", 4, 9, 25, Some("classify")),
            ("True", "False", 4, 11, 13, Some("classify")),
            ("False", "True", 5, 11, 20, Some("classify")),
        ]
    );
    assert_candidates_reparse(source, &output.candidates);
}

#[test]
fn pattern_literal_signs_are_not_candidates_and_expression_context_is_restored() {
    let source = concat!(
        "class Point:\n    __match_args__ = ('x',)\n",
        "def classify(subject, guard):\n",
        "    match -subject:\n",
        "        case -1:\n            return -guard\n",
        "        case -1.5:\n            pass\n",
        "        case -2j:\n            pass\n",
        "        case -3-4j | -3+4j:\n            pass\n",
        "        case (-5):\n            pass\n",
        "        case -6 | -7:\n            pass\n",
        "        case [-8, (-9.5)]:\n            pass\n",
        "        case Point(-10):\n            pass\n",
        "        case {-11: True}:\n            pass\n",
        "        case False if +guard:\n            return +subject\n",
        "    return -subject\n",
    );

    let output = analyze(source);
    let signs: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.original.as_str(), "+" | "-"))
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.operator.as_str(),
                candidate.line,
                candidate.column,
            )
        })
        .collect();

    assert_eq!(
        signs,
        vec![
            ("-", "unary_sign", 4, 10),
            ("-", "unary_sign", 6, 19),
            ("-", "binary_add_sub", 11, 15),
            ("+", "binary_add_sub", 11, 23),
            ("+", "unary_sign", 23, 22),
            ("+", "unary_sign", 24, 19),
            ("-", "unary_sign", 25, 11),
        ]
    );
    let booleans: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "boolean_literal")
        .map(|candidate| (candidate.original.as_str(), candidate.line))
        .collect();
    assert_eq!(booleans, vec![("True", 21), ("False", 23)]);
}

#[test]
fn augmented_and_pattern_candidates_share_line_symbol_and_focused_selection() {
    let source = concat!(
        "def selected(value, factor):\n",
        "    value *= factor\n",
        "    match value:\n",
        "        case True:\n            return value\n",
        "def ignored(value, factor):\n",
        "    value //= factor\n",
        "    match value:\n",
        "        case False:\n            return value\n",
    );
    let lines = [
        LineRange { start: 2, end: 2 },
        LineRange { start: 4, end: 4 },
    ];
    let symbols = ["pkg.sample:selected".to_owned()];
    let operators = MutationOperatorSelection::default();

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &lines,
            symbols: &symbols,
            operators: &operators,
            profile: MutationProfile::Focused,
            max_candidates: 10_000,
        },
        source,
    );

    let candidates: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.operator.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect();

    assert_eq!(
        candidates,
        vec![
            ("*=", "/=", "augmented_mul_div", 2, Some("selected")),
            ("*=", "=", "augmented_to_assignment", 2, Some("selected")),
            ("True", "False", "boolean_literal", 4, Some("selected")),
        ]
    );
}

#[rstest::rstest]
#[case::result_item_not_comment_in_items_1(0)]
#[case::result_left_is_comment_not_right_2(1)]
fn composite_comparisons_preserve_multiline_trivia_and_reparse(#[case] scenario_index: usize) {
    let (source, operator, span, original, replacement, mutated) = [
        (
            "result = (\n    item not  # comment\n    in items\n)\n",
            "membership",
            ByteSpan {
                start: 20,
                length: 21,
            },
            "not  # comment\n    in",
            "  # comment\n    in",
            "result = (\n    item   # comment\n    in items\n)\n",
        ),
        (
            "result = (\n    left is  # comment\n    not right\n)\n",
            "identity",
            ByteSpan {
                start: 20,
                length: 21,
            },
            "is  # comment\n    not",
            "is  # comment\n    ",
            "result = (\n    left is  # comment\n     right\n)\n",
        ),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let candidate = analyze(source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .find(|candidate| candidate.operator.as_str() == operator)
        .expect("composite comparison candidate");

    assert_eq!(candidate.span, span);
    assert_eq!(candidate.original, original);
    assert_eq!(candidate.replacement, replacement);
    assert_eq!(apply_candidate_and_reparse(source, &candidate), mutated);
}

#[test]
#[ignore = "benchmark harness; run explicitly in release mode"]
fn benchmark_candidate_line_positions() {
    use std::fmt::Write as _;

    let mut source = String::with_capacity(307_200);
    for index in 0..7_680 {
        writeln!(
            source,
            "result_{index:05} = left_{index:05} + right_{index:05}"
        )
        .expect("writing to String cannot fail");
    }

    assert_eq!(source.len(), 307_200);

    let started = std::time::Instant::now();

    let output = analyze(&source);
    let elapsed = started.elapsed();
    let candidates = std::hint::black_box(output).candidates.len();

    assert_eq!(candidates, 7_680);
    println!(
        "source_bytes={} candidates={candidates} elapsed_ms={}",
        source.len(),
        elapsed.as_secs_f64() * 1_000.0
    );
}

#[rstest::rstest]
#[case::single_1(0)]
#[case::multiline_2(1)]
#[ignore = "benchmark harness; run explicitly in release mode"]
fn benchmark_long_single_line_candidate_columns_scenario_1(#[case] scenario_index: usize) {
    const COUNT: usize = 32_000;
    let single = format!(
        "values = [{}]\n",
        (0..COUNT)
            .map(|_| "True")
            .collect::<Vec<_>>()
            .join(",                               ")
    );
    let multiline = format!(
        "values = [{}]\n",
        (0..COUNT)
            .map(|_| "True")
            .collect::<Vec<_>>()
            .join(",\n                              ")
    );

    assert_eq!(single.len(), multiline.len());

    let mut operators = MutationOperatorSelection::default();
    for operator in operators
        .names()
        .iter()
        .map(|name| MutationOperatorSelection::parse_selector(name).unwrap()[0])
        .filter(|operator| *operator != MutationOperator::BooleanLiteral)
    {
        operators.exclude(operator);
    }

    let (shape, source) = [("single", single), ("multiline", multiline)]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let started = std::time::Instant::now();

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/long.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 1,
        },
        &source,
    );
    let elapsed = started.elapsed();

    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
    assert_eq!(output.candidates[0].operator.as_str(), "boolean_literal");
    println!(
        "shape={shape} source_bytes={} discovered={} retained={} elapsed_ms={}",
        source.len(),
        COUNT,
        output.candidates.len(),
        elapsed.as_secs_f64() * 1_000.0
    );
}

#[test]
#[ignore = "benchmark harness; run explicitly in release mode"]
fn benchmark_long_single_line_candidate_columns() {
    const COUNT: usize = 32_000;
    let single = format!(
        "values = [{}]\n",
        (0..COUNT)
            .map(|_| "True")
            .collect::<Vec<_>>()
            .join(",                               ")
    );
    let multiline = format!(
        "values = [{}]\n",
        (0..COUNT)
            .map(|_| "True")
            .collect::<Vec<_>>()
            .join(",\n                              ")
    );

    assert_eq!(single.len(), multiline.len());

    let mut operators = MutationOperatorSelection::default();
    for operator in operators
        .names()
        .iter()
        .map(|name| MutationOperatorSelection::parse_selector(name).unwrap()[0])
        .filter(|operator| *operator != MutationOperator::BooleanLiteral)
    {
        operators.exclude(operator);
    }
}

#[test]
#[ignore = "benchmark harness; run explicitly in release mode"]
fn benchmark_adversarial_ast_fact_indexes() {
    use std::fmt::Write as _;

    const FUNCTIONS: usize = 512;
    let mut source = String::with_capacity(FUNCTIONS * 180);
    for index in 0..FUNCTIONS {
        writeln!(
            source,
            "def scope_{index:04}(value: list[int] | tuple[int, ...] = [1, 2]):\n    assert not value\n    print(not value)\n    return (not value) and value[0] + 1"
        )
        .expect("writing to String cannot fail");
    }

    let started = std::time::Instant::now();

    let output = analyze_with_profile(MutationProfile::Focused, usize::MAX, &source);
    let elapsed = started.elapsed();
    let stats = output.fact_lookups;

    assert_eq!(stats.annotation.facts, FUNCTIONS);
    assert_eq!(stats.arid.facts, FUNCTIONS * 3);
    assert_eq!(stats.not_operand.facts, FUNCTIONS * 3);
    assert_eq!(stats.scope.facts, FUNCTIONS);
    assert_eq!(stats.not_operand.queries, FUNCTIONS * 3);
    assert_eq!(stats.not_operand.comparisons, 0);
    for index in [stats.annotation, stats.arid, stats.scope] {
        let comparisons_per_query = usize::BITS as usize - index.facts.leading_zeros() as usize;
        assert!(
            index.comparisons <= index.queries * comparisons_per_query,
            "stats={index:?}",
        );
    }
    let mut operator_counts = std::collections::BTreeMap::new();
    for candidate in &output.candidates {
        *operator_counts
            .entry(candidate.operator.as_str())
            .or_insert(0) += 1;
    }
    // Integer-neighbor mutations became default after this benchmark was added:
    // the terminal `+ 1` now contributes replacements `0` and `2` per function.
    assert_eq!(
        operator_counts.get("integer_literal_neighbor"),
        Some(&(FUNCTIONS * 2))
    );
    assert_eq!(
        output.candidates.len(),
        FUNCTIONS * 7,
        "{operator_counts:?}"
    );
    assert_eq!(
        output.candidates.first().map(|candidate| (
            candidate.original.as_str(),
            candidate.operator.as_str(),
            candidate.symbol.as_deref(),
        )),
        Some(("not value", "remove_not", Some("scope_0000"))),
    );
    assert_eq!(
        output.candidates.last().map(|candidate| (
            candidate.original.as_str(),
            candidate.operator.as_str(),
            candidate.symbol.as_deref(),
        )),
        Some(("1", "integer_literal_neighbor", Some("scope_0511"))),
    );
    println!(
        "source_bytes={} candidates={} elapsed_ms={} stats={stats:?}",
        source.len(),
        output.candidates.len(),
        elapsed.as_secs_f64() * 1_000.0,
    );
}

#[test]
fn all_operator_tokens_query_annotation_index() {
    let addition = analyze("result = left + right\n");
    let multiplication = analyze("result = left * right\n");
    let bitwise = analyze("result = left & right\n");

    assert_eq!(
        addition.fact_lookups.annotation.queries,
        multiplication.fact_lookups.annotation.queries,
    );
    assert_eq!(
        bitwise.fact_lookups.annotation.queries,
        addition.fact_lookups.annotation.queries,
    );
    assert!(addition.fact_lookups.annotation.queries > 0);
    assert_eq!(addition.fact_lookups.annotation.comparisons, 0);
    assert_eq!(addition.fact_lookups.annotation.facts, 0);
}

#[rstest::rstest]
#[case::value_0(0, (1, 0))]
#[case::value_6(6, (2, 0))]
#[case::value_8(8, (2, 1))]
#[case::value_11(11, (2, 4))]
#[case::value_24(24, (3, 9))]
fn line_index_reports_one_based_lines_and_unicode_scalar_columns(
    #[case] offset: usize,
    #[case] expected: (u32, u32),
) {
    let source = "alpha\nβeta\r\n終 = left == right\n";
    let line_index = LineIndex::new(source);

    assert_eq!(
        line_index.line_and_column(offset),
        expected,
        "position at byte offset {offset}",
    );
}

#[rstest::rstest]
#[case::value_0(0, (1, 0))]
#[case::value_3(3, (1, 0))]
#[case::value_5(5, (1, 2))]
#[case::value_6(6, (2, 0))]
#[case::value_9(9, (2, 1))]
#[case::value_10(10, (2, 2))]
fn line_index_ignores_only_a_leading_file_bom_in_columns(
    #[case] offset: usize,
    #[case] expected: (u32, u32),
) {
    let source = "\u{feff}ab\n\u{feff}c\n";
    let line_index = LineIndex::new(source);

    assert_eq!(
        line_index.line_and_column(offset),
        expected,
        "position at byte offset {offset}",
    );
}

#[test]
fn line_index_positions_token_and_type_annotation_candidates() {
    let source = "from typing import Optional\nπ = left == right\n値: Optional[int]\n";

    let output = analyze_types(source);
    let positions: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.line,
                candidate.column,
            )
        })
        .collect();

    assert_eq!(positions, vec![("==", 2, 9), ("Optional[int]", 3, 3)]);
}

#[test]
fn large_source_analysis_observes_cancellation_during_token_traversal() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let source = "value = left == right\n".repeat(20_000);
    // Account for the new preflight probes so cancellation still reaches the
    // token traversal named by this regression, rather than stopping in the guard.
    let parsed = ruff_python_parser::parse_module(&source).unwrap();
    let preflight_probes = AtomicUsize::new(0);
    super::depth::check(parsed.syntax(), &|| {
        preflight_probes.fetch_add(1, Ordering::Relaxed);
        false
    })
    .unwrap();
    let cancel_on = preflight_probes.load(Ordering::Relaxed) + 128;
    drop(parsed);
    let probes = AtomicUsize::new(0);

    let result = analyze_source_cancellable(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/large.py"),
            lines: &[],
            symbols: &[],
            operators: &MutationOperatorSelection::default(),
            profile: MutationProfile::Full,
            max_candidates: usize::MAX,
        },
        &source,
        || probes.fetch_add(1, Ordering::Relaxed) >= cancel_on,
    );

    assert!(matches!(result, Err(super::AnalysisError::Cancelled)));
}

fn analyze_types(source: &str) -> super::AnalyzerOutput {
    analyze_types_with_profile(MutationProfile::Full, source)
}

fn analyze_with_profile(
    profile: MutationProfile,
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    let operators = MutationOperatorSelection::default();
    analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile,
            max_candidates,
        },
        source,
    )
}

fn analyze_types_with_profile(profile: MutationProfile, source: &str) -> super::AnalyzerOutput {
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
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile,
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
            payload: super::payload::PayloadBudget::default(),
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

fn analyze_with_all_candidate_producers(
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
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
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/high.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates,
        },
        source,
    )
}

#[test]
fn bounded_collection_preserves_the_exact_full_output_prefix_and_retention_bounds() {
    use std::fmt::Write;

    let mut source = String::new();
    for index in 0..100 {
        writeln!(
            source,
            "def value_{index}(items: list[int]) -> list[int]:\n    return list(items[{index}] + {index})"
        )
        .expect("writing to a string succeeds");
    }
    let full = analyze_with_all_candidate_producers(10_000, &source);
    let bounded = analyze_with_all_candidate_producers(3, &source);

    assert!(full.candidates.len() > 3);
    assert_eq!(bounded.candidates, full.candidates[..3]);
    assert!(bounded.truncated);
    assert_eq!(
        bounded.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
    assert!(
        bounded
            .retention
            .producer_peaks
            .iter()
            .all(|peak| *peak > 0)
    );
    assert_eq!(bounded.retention.producer_peaks, [4; 3]);
    assert_eq!(bounded.retention.merged_peak, 12);
}

#[test]
fn bounded_collection_filters_focused_arid_candidates_before_prefix_capacity() {
    let selected_operators: &[&str] = &["boolean_literal", "binary_add_sub"];
    let source = format!("{}result = 1 + 2\n", "print(True)\n".repeat(100));
    let focused =
        analyze_selected_with_profile(selected_operators, MutationProfile::Focused, 1, &source);

    assert_eq!(focused.candidates.len(), 1);
    assert_eq!(focused.candidates[0].line, 101);
    assert_eq!(focused.candidates[0].operator.as_str(), "binary_add_sub");
    assert!(!focused.truncated);
    assert_eq!(focused.diagnostics, Vec::new());
    assert_eq!(focused.retention.producer_peaks[0], 1);
}

#[test]
fn bounded_collection_with_zero_limit_keeps_only_an_overflow_probe() {
    let output = analyze_with(Utf8Path::new("pkg/zero.py"), &[], &[], 0, "value = 1 + 2\n");

    assert_eq!(output.candidates, Vec::new());
    assert!(output.truncated);
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::CandidateLimitExceeded
    );
    assert!(
        output
            .retention
            .producer_peaks
            .iter()
            .all(|peak| *peak <= 1)
    );
    assert!(output.retention.merged_peak <= 3);
}

#[test]
fn focused_profile_suppresses_main_print_assert_and_defaults() {
    let selected_operators: &[&str] = &["compare_eq_ne", "binary_add_sub", "boolean_literal"];
    let source = "if __name__ == \"__main__\":\n    print(1 + 2)\n    assert 3 == 3\nelse:\n    fallback = 4 + 5\n\ndef f(flag=True, *, enabled=False):\n    return flag + enabled\n";
    let focused =
        analyze_selected_with_profile(selected_operators, MutationProfile::Focused, 10_000, source);
    let descriptors: Vec<_> = focused
        .candidates
        .iter()
        .map(|candidate| (candidate.line, candidate.operator.as_str()))
        .collect();

    assert_eq!(
        descriptors,
        vec![(5, "binary_add_sub"), (8, "binary_add_sub"),]
    );
}

#[rstest::rstest]
#[case::value_3(3)]
#[case::value_4(4)]
#[case::value_5(5)]
#[case::value_6(6)]
#[case::value_7(7)]
#[case::value_8(8)]
fn focused_profile_accepts_only_exact_main_guard_shapes(#[case] line: u32) {
    let source = "if \"__main__\" == __name__:\n    reversed = 1 + 2\nif __name__ != \"__main__\":\n    inequality = 3 + 4\nif __name__ == \"__main__\" == \"__main__\":\n    chained = 5 + 6\nif __name__ == \"entry\":\n    entry = 7 + 8\n";
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);

    assert!(
        focused
            .candidates
            .iter()
            .all(|candidate| candidate.line != 2)
    );

    assert!(
        focused
            .candidates
            .iter()
            .any(|candidate| candidate.line == line),
        "expected an eligible candidate on line {line}",
    );
}

#[test]
fn focused_profile_suppresses_only_bare_print_and_assert() {
    let selected_operators: &[&str] = &["binary_add_sub"];
    let source = "print(1 + 2)\nlogger.print(3 + 4)\nassert 5 + 6\nregular = 7 + 8\n";
    let focused =
        analyze_selected_with_profile(selected_operators, MutationProfile::Focused, 10_000, source);
    let descriptors: Vec<_> = focused
        .candidates
        .iter()
        .map(|candidate| (candidate.line, candidate.operator.as_str()))
        .collect();

    assert_eq!(
        descriptors,
        vec![(2, "binary_add_sub"), (4, "binary_add_sub")]
    );
}

#[test]
fn focused_profile_retains_type_annotation_candidates() {
    let source = "from typing import Optional\n\ndef choose(value: Optional[int], enabled=True) -> Optional[int]:\n    return value\n";
    let focused = analyze_types_with_profile(MutationProfile::Focused, source);

    assert!(
        focused
            .candidates
            .iter()
            .any(|candidate| candidate.operator.as_str().starts_with("type_"))
    );
    assert!(
        focused
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "boolean_literal")
    );
}

#[test]
fn focused_filter_runs_before_candidate_limit() {
    let selected_operators: &[&str] = &["boolean_literal", "binary_add_sub"];
    let focused = analyze_selected_with_profile(
        selected_operators,
        MutationProfile::Focused,
        1,
        "def choose(enabled=True):\n    return 1 + 2\n",
    );

    assert_eq!(focused.candidates.len(), 1);
    assert_eq!(focused.candidates[0].line, 2);
    assert_eq!(focused.candidates[0].operator.as_str(), "binary_add_sub");
}

#[test]
fn focused_profile_applies_after_line_and_symbol_selection() {
    let source = "def selected(enabled=True):\n    return 1 + 2\n\ndef ignored(enabled=True):\n    return 3 + 4\n";
    let lines = vec![LineRange { start: 2, end: 2 }];
    let symbols = vec!["pkg.sample:selected".to_owned()];
    let operators = MutationOperatorSelection::default();
    let focused = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &lines,
            symbols: &symbols,
            operators: &operators,
            profile: MutationProfile::Focused,
            max_candidates: 10_000,
        },
        source,
    );
    let descriptors: Vec<_> = focused
        .candidates
        .iter()
        .map(|candidate| (candidate.line, candidate.operator.as_str()))
        .collect();

    assert_eq!(
        descriptors,
        vec![
            (2, "integer_literal_neighbor"),
            (2, "integer_literal_neighbor"),
            (2, "binary_add_sub"),
            (2, "integer_literal_neighbor"),
            (2, "integer_literal_neighbor")
        ]
    );
}

#[test]
fn full_profile_matches_default_candidate_output() {
    assert_eq!(
        analyze("result = first + second\n").candidates,
        analyze_with_profile(MutationProfile::Full, 10_000, "result = first + second\n").candidates,
    );
}

#[test]
fn omits_candidates_for_unselected_operators() {
    let mut operators = MutationOperatorSelection::default();
    operators.exclude(MutationOperator::BinaryAddSub);

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        "result = left + right\n",
    );

    assert_eq!(output.candidates, Vec::new());
}

const MUTABLE_OPERATOR_TOKENS: &[&str] = &[
    "==", "!=", "<", "<=", ">", ">=", "in", "not in", "is", "is not", "and", "or", "+=", "-=",
    "*=", "/=", "//=", "%=", "*", "/", "//", "%", "&", "|", "<<", ">>", "break", "continue",
    "True", "False", "+", "-", "not",
];

const NESTED_QUOTE_PAIRS: &[(&str, &str, &str)] = &[
    (
        "single-quoted",
        "label = 'plain \"double\" text'\n",
        "label = 'mutable + \"double\" text'\n",
    ),
    (
        "double-quoted",
        "label = \"plain 'single' text\"\n",
        "label = \"mutable == 'single' text\"\n",
    ),
    (
        "triple-single-quoted",
        "label = '''plain \"double\" and 'single' text'''\n",
        "label = '''mutable True + \"double\" and 'single' text'''\n",
    ),
    (
        "triple-double-quoted",
        "label = \"\"\"plain 'single' and \"double\" text\"\"\"\n",
        "label = \"\"\"mutable False - 'single' and \"double\" text\"\"\"\n",
    ),
];

fn assert_only_trailing_expression_changes(literal_line: &str) {
    let source = format!("{literal_line}result = left + right\n");
    let output = analyze_with_only_operator(&source, MutationOperator::BinaryAddSub);
    assert_eq!(output.diagnostics, Vec::new());
    assert_eq!(output.candidates.len(), 1);
    let candidate = &output.candidates[0];
    assert_eq!(candidate.original, "+");
    assert_eq!(candidate.replacement, "-");
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[..start], format!("{literal_line}result = left "));
    assert_eq!(&source[end..], " right\n");
    let mut mutated = source.clone();
    mutated.replace_range(start..end, &candidate.replacement);
    assert_eq!(mutated, format!("{literal_line}result = left - right\n"));
}

#[rstest::rstest]
#[case::value_1(0)]
#[case::value_2(1)]
#[case::value_3(2)]
#[case::value_4(3)]
#[case::value_5(4)]
#[case::value_6(5)]
#[case::in_7(6)]
#[case::not_in_8(7)]
#[case::is_9(8)]
#[case::is_not_10(9)]
#[case::and_11(10)]
#[case::or_12(11)]
#[case::value_13(12)]
#[case::value_14(13)]
#[case::value_15(14)]
#[case::value_16(15)]
#[case::value_17(16)]
#[case::value_18(17)]
#[case::value_19(18)]
#[case::value_20(19)]
#[case::value_21(20)]
#[case::value_22(21)]
#[case::value_23(22)]
#[case::value_24(23)]
#[case::value_25(24)]
#[case::value_26(25)]
#[case::break_27(26)]
#[case::continue_28(27)]
#[case::enabled_29(28)]
#[case::disabled_30(29)]
#[case::value_31(30)]
#[case::value_32(31)]
#[case::not_33(32)]
fn ordinary_string_literals_ignore_every_mutable_operator_token(#[case] case_index: usize) {
    let selected_operators: &[&str] = TOKEN_OPERATOR_NAMES;

    assert_eq!(
        analyze_selected(
            selected_operators,
            "label = 'ordinary operator-free text'\n"
        )
        .candidates,
        Vec::new()
    );

    let token = MUTABLE_OPERATOR_TOKENS[case_index];

    let source = format!("label = {token:?}\n");
    assert!(
        analyze_selected(selected_operators, &source)
            .candidates
            .is_empty(),
        "ordinary string content {token:?} produced a candidate"
    );
    assert_only_trailing_expression_changes(&source);
}

#[rstest::rstest]
#[case::single_quoted_1(0)]
#[case::double_quoted_2(1)]
#[case::triple_single_quoted_3(2)]
#[case::triple_double_quoted_4(3)]
fn nested_quote_pairs_preserve_every_surrounding_string_byte(
    #[case] case_index: usize,
    #[values(0, 1)] scenario_index: usize,
) {
    let selected_operators: &[&str] = TOKEN_OPERATOR_NAMES;

    let (name, benign, adversarial) = NESTED_QUOTE_PAIRS[case_index];

    let source = [benign, adversarial]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    assert!(
        analyze_selected(selected_operators, source)
            .candidates
            .is_empty(),
        "nested quote fixture {name} produced a literal-content candidate"
    );
    assert_only_trailing_expression_changes(source);
}

#[rstest::rstest]
#[case::f_string(
    "f-string",
    "value = f\"literal == + True {plain}\"\n",
    "value = f\"literal == + True {left + right} {enabled is not None}\"\n"
)]
#[case::t_string(
    "t-string",
    "value = t\"literal == + True {plain}\"\n",
    "value = t\"literal == + True {left + right} {enabled is not None}\"\n"
)]
fn interpolated_string_pairs_skip_literals_and_retain_expressions(
    #[case] flavor: &str,
    #[case] literal: &str,
    #[case] expression: &str,
) {
    let selected_operators: &[&str] = TOKEN_OPERATOR_NAMES;

    assert!(
        analyze_selected(selected_operators, literal)
            .candidates
            .is_empty(),
        "{flavor} literal content produced a candidate"
    );
    let observed = analyze_selected(selected_operators, expression)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .map(|candidate| {
            (
                candidate.original,
                candidate.replacement,
                candidate.operator.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        [
            ("+".to_owned(), "-".to_owned(), "binary_add_sub"),
            ("is not".to_owned(), "is".to_owned(), "identity"),
        ],
        "{flavor} interpolation expressions were not analyzed exactly"
    );
}

#[test]
fn pep_695_type_positions_suppress_runtime_mutations() {
    let source = concat!(
        "def convert[T: Left | Right = list[str]](value: T):\n",
        "    return left | right\n",
        "class Box[U: Base | None = tuple[int]]:\n",
        "    runtime = first + second\n",
    );

    let output = analyze(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "bitwise_and_or" | "binary_add_sub"
            )
        })
        .map(|candidate| (candidate.original.as_str(), candidate.line))
        .collect::<Vec<_>>();

    assert_eq!(observed, [("|", 2), ("+", 4)]);
    assert_candidates_reparse(source, &output.candidates);
}

#[test]
fn pep_695_type_positions_emit_type_candidates() {
    let source = concat!(
        "from typing import Sequence\n",
        "type Values[T: list[str] = list[bytes], *Ts = list[float], **P = list[bool]] = list[int]\n",
        "class Outer:\n",
        "    type Nested[U: list[int]] = list[bytes]\n",
    );

    let output = analyze_types(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            ("list[str]", "Sequence[str]", 2, Some("Values")),
            ("list[bytes]", "Sequence[bytes]", 2, Some("Values")),
            ("list[float]", "Sequence[float]", 2, Some("Values")),
            ("list[bool]", "Sequence[bool]", 2, Some("Values")),
            ("list[int]", "Sequence[int]", 2, Some("Values")),
            ("list[int]", "Sequence[int]", 4, Some("Outer.Nested")),
            ("list[bytes]", "Sequence[bytes]", 4, Some("Outer.Nested")),
        ]
    );
    assert_candidates_reparse(source, &output.candidates);
}

#[test]
fn pep_695_type_parameters_shadow_imported_replacement_spellings() {
    let source = concat!(
        "from typing import Sequence\n",
        "def convert[Sequence](value: list[int]) -> list[str]:\n",
        "    local: list[float]\n",
        "class Box[Sequence]:\n",
        "    field: list[bytes]\n",
        "type Alias[Sequence] = list[int]\n",
        "safe: list[bool]\n",
    );

    let output = analyze_types(source);
    let observed = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.line,
                candidate.symbol.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(observed, [("list[bool]", "Sequence[bool]", 7, None)]);
}

#[test]
fn type_annotations_emit_supported_candidates_in_source_order() {
    let source = "from typing import AbstractSet, Iterator, Mapping, Optional\nimport typing as t\nfrom collections.abc import Iterable, Sequence\n\nmodule_value: Optional[int]\n\nclass Model:\n    names: list[str]\n\n    def convert(self, name: str | None, age: t.Optional[int]) -> set[str]:\n        mapping: dict[str, int] = {}\n        values: Iterable[str] = []\n        ordered: Sequence[str] = []\n        return set()\n";

    let output = analyze_types(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str().starts_with("type_"))
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

#[rstest::rstest]
#[case::optional_grouped_union(
    "Optional grouped union",
    "from typing import Optional\nx: Optional[(int\n | str)]\n",
    "(int\n | str)"
)]
#[case::none_leading_grouped_union(
    "None leading grouped union",
    "x: None | (int\n | str)\n",
    "(int\n | str)"
)]
#[case::none_trailing_grouped_union(
    "None trailing grouped union",
    "x: (int\n | str) | None\n",
    "(int\n | str)"
)]
#[case::none_trailing_ungrouped_multiline_union(
    "None trailing ungrouped multiline union",
    "x: (int\n | str\n | None)\n",
    "(int\n | str)"
)]
#[case::parameter_optional_grouped_union(
    "parameter Optional grouped union",
    "from typing import Optional\ndef f(value: Optional[(int\n | str)]):\n    pass\n",
    "(int\n | str)"
)]
#[case::return_optional_grouped_union(
    "return Optional grouped union",
    "from typing import Optional\ndef f() -> Optional[(int\n | str)]:\n    pass\n",
    "(int\n | str)"
)]
#[case::optional_union_comments(
    "Optional union comments",
    "from typing import Optional\nx: Optional[\n    # first\n    int\n    | str  # second\n]\n",
    "(\n    # first\n    int\n    | str  # second\n)"
)]
#[case::optional_grouped_union_surrounding_whitespace(
    "Optional grouped union surrounding whitespace",
    "from typing import Optional\nx: Optional[\n    (int | str)\n]\n",
    "(\n    (int | str)\n)"
)]
#[case::none_trailing_hash_string_literal(
    "None trailing hash string literal",
    "def resolve(value):\n    return int\nx: resolve(\"#\") | None\n",
    "resolve(\"#\")"
)]
#[case::optional_hash_string_literal(
    "Optional hash string literal",
    "from typing import Optional\ndef resolve(value):\n    return int\nx: Optional[resolve(\"#\")]\n",
    "resolve(\"#\")"
)]
fn nullable_removal_preserves_multiline_annotation_syntax(
    #[case] name: &str,
    #[case] source: &str,
    #[case] expected: &str,
) {
    let output = analyze_types(source);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_nullable_remove")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), 1, "{name}");
    assert_eq!(candidates[0].replacement, expected, "{name}");
    apply_candidate_and_reparse(source, candidates[0]);
}

#[rstest::rstest]
#[case::unaliased_import_assignment_and_restoration_1(0)]
#[case::direct_alias_assignment_and_restoration_2(1)]
#[case::delete_rebinding_3(2)]
#[case::function_definition_rebinding_and_restoration_4(3)]
#[case::class_definition_rebinding_and_restoration_5(4)]
fn typing_import_rebinding_linear(#[case] scenario_index: usize) {
    let (name, source, expected) = [
        (
            "unaliased import assignment and restoration",
            "from typing import Sequence\nbefore: list[str]\nSequence = local_sequence\nafter: list[str]\nfrom typing import Sequence as Sequence\nrestored: list[str]\n",
            vec![(139, 9, 6, None, "Sequence[str]")],
        ),
        (
            "direct alias assignment and restoration",
            "from typing import Sequence as Seq\nbefore: list[str]\nSeq = local_sequence\nafter: list[str]\nfrom typing import Sequence as Seq\nrestored: list[str]\n",
            vec![(136, 9, 6, None, "Seq[str]")],
        ),
        (
            "delete rebinding",
            "from typing import Sequence\nbefore: list[str]\ndel Sequence\nafter: list[str]\n",
            vec![],
        ),
        (
            "function definition rebinding and restoration",
            "from typing import Sequence\nbefore: list[str]\ndef Sequence():\n    pass\nafter: list[str]\nfrom typing import Sequence\nrestored: list[str]\n",
            vec![(126, 9, 7, None, "Sequence[str]")],
        ),
        (
            "class definition rebinding and restoration",
            "from typing import Sequence\nbefore: list[str]\nclass Sequence:\n    pass\nafter: list[str]\nfrom typing import Sequence\nrestored: list[str]\n",
            vec![(126, 9, 7, None, "Sequence[str]")],
        ),
    ].into_iter().nth(scenario_index).unwrap();

    let output = analyze_types(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.replacement.as_str(),
            )
        })
        .collect();

    assert_eq!(actual, expected, "{name}");
    assert_candidates_reparse(source, &output.candidates);
}

#[rstest::rstest]
#[case::module_alias_assignment_and_restoration_1(0)]
#[case::unsupported_competing_module_import_and_restoration_2(1)]
#[case::wildcard_import_also_invalidates_deferred_builtin_provenance_3(2)]
fn typing_module_alias_rebinding_linear(#[case] scenario_index: usize) {
    let (name, source, expected) = [
        (
            "module alias assignment and restoration",
            "import typing as t\nbefore: list[str]\nt = local_typing\nafter: list[str]\nimport typing as t\nrestored: list[str]\n",
            vec![(100, 9, 6, None, "t.Sequence[str]")],
        ),
        (
            "unsupported competing module import and restoration",
            "import typing as t\nbefore: list[str]\nimport local as t\nafter: list[str]\nimport typing as t\nrestored: list[str]\n",
            vec![(101, 9, 6, None, "t.Sequence[str]")],
        ),
        (
            "wildcard import also invalidates deferred builtin provenance",
            "import typing as t\nbefore: list[str]\nfrom local import *\nafter: list[str]\n",
            // Even the earlier annotation may see a shadowed list when evaluated.
            vec![],
        ),
    ].into_iter().nth(scenario_index).unwrap();

    let output = analyze_types(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.replacement.as_str(),
            )
        })
        .collect();

    assert_eq!(actual, expected, "{name}");
    assert_candidates_reparse(source, &output.candidates);
}

const TYPING_IMPORT_REBINDING_SCOPE_SOURCE: &str = concat!(
    "from typing import Sequence\n",
    "\n",
    "def Sequence(value: Sequence[str]) -> Sequence[str]:\n",
    "    pass\n",
    "same_name_after: list[str]\n",
    "from typing import Sequence\n",
    "\n",
    "def parameter_shadow(Sequence):\n",
    "    hidden: list[str]\n",
    "\n",
    "def local_shadow():\n",
    "    hidden_before: list[str]\n",
    "    Sequence = local_sequence\n",
    "    from typing import Sequence\n",
    "    restored: list[str]\n",
    "\n",
    "def nested_outer():\n",
    "    visible: list[str]\n",
    "    def nested():\n",
    "        hidden: list[str]\n",
    "        Sequence = local_sequence\n",
    "    class Nested:\n",
    "        visible: list[str]\n",
    "        Sequence = local_sequence\n",
    "        hidden: list[str]\n",
    "    visible_after: list[str]\n",
    "\n",
    "class Container:\n",
    "    Sequence = local_sequence\n",
    "    method_field: list[str]\n",
    "    def method(self, value: list[str]):\n",
    "        visible_body: list[str]\n",
    "\n",
    "def global_binding():\n",
    "    global Sequence\n",
    "    visible_before: list[str]\n",
    "    Sequence = local_sequence\n",
    "    hidden_after: list[str]\n",
    "\n",
    "def nonlocal_outer():\n",
    "    from typing import Sequence\n",
    "    def inner():\n",
    "        nonlocal Sequence\n",
    "        visible_before: list[str]\n",
    "        Sequence = local_sequence\n",
    "        hidden_after: list[str]\n",
    "    visible_outer: list[str]\n",
    "\n",
    "def with_target(manager):\n",
    "    hidden_before: list[str]\n",
    "    with manager as Sequence:\n",
    "        hidden_inside: list[str]\n",
    "\n",
    "def except_target():\n",
    "    hidden_before: list[str]\n",
    "    try:\n",
    "        work()\n",
    "    except Error as Sequence:\n",
    "        hidden_inside: list[str]\n",
    "\n",
    "def pattern_target(value):\n",
    "    hidden_before: list[str]\n",
    "    match value:\n",
    "        case {\"item\": Sequence}:\n",
    "            hidden_inside: list[str]\n",
    "\n",
    "def named_target(value):\n",
    "    hidden_before: list[str]\n",
    "    if (Sequence := value):\n",
    "        hidden_inside: list[str]\n",
    "\n",
    "def comprehension_target(values):\n",
    "    visible_before: list[str]\n",
    "    result = [item for Sequence in values for item in Sequence]\n",
    "    visible_after: list[str]\n",
    "\n",
    "untouched: list[str]\n",
);

#[test]
#[allow(clippy::too_many_lines)]
fn typing_import_rebinding_scope() {
    let source = TYPING_IMPORT_REBINDING_SCOPE_SOURCE;

    let output = analyze_types(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.original.as_str(),
                candidate.replacement.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                327,
                9,
                15,
                Some("local_shadow"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                371,
                9,
                18,
                Some("nested_outer"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                583,
                9,
                26,
                Some("nested_outer"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                731,
                9,
                32,
                Some("Container.method"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                804,
                9,
                36,
                Some("global_binding"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                994,
                9,
                44,
                Some("nonlocal_outer.inner"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                1089,
                9,
                47,
                Some("nonlocal_outer"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                1671,
                9,
                73,
                Some("comprehension_target"),
                "list[str]",
                "Sequence[str]"
            ),
            (
                1764,
                9,
                75,
                Some("comprehension_target"),
                "list[str]",
                "Sequence[str]"
            ),
        ]
    );
    assert_candidates_reparse(source, &output.candidates);

    let alias_source = concat!(
        "import typing as t\n",
        "def module_alias_shadow():\n",
        "    hidden_before: list[str]\n",
        "    t = local_typing\n",
        "    import typing as t\n",
        "    restored: list[str]\n",
        "untouched: list[str]\n",
    );
    let alias_output = analyze_types(alias_source);
    let alias_actual: Vec<_> = alias_output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.span.length,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(
        alias_actual,
        vec![
            (133, 9, 6, Some("module_alias_shadow"), "t.Sequence[str]"),
            (154, 9, 7, None, "t.Sequence[str]"),
        ]
    );
    for candidate in &alias_output.candidates {
        apply_candidate_and_reparse(alias_source, candidate);
    }
}

type TypingRebindingCase = (
    &'static str,
    &'static str,
    &'static [(u64, u32, Option<&'static str>, &'static str)],
);

const TYPING_REBINDING_CASES: &[TypingRebindingCase] = &[
    (
        "if joins optional and identical branches",
        concat!(
            "from typing import Sequence\n",
            "before: list[str]\n",
            "if condition:\n",
            "    Sequence = local_sequence\n",
            "after_optional: list[str]\n",
            "Sequence = local_sequence\n",
            "if condition:\n",
            "    from typing import Sequence\n",
            "else:\n",
            "    from typing import Sequence\n",
            "after_identical: list[str]\n",
        ),
        &[(243, 11, None, "Sequence[str]")],
    ),
    (
        "returning branch is not a continuation exit",
        concat!(
            "from typing import Sequence\n",
            "def reachable(flag):\n",
            "    from typing import Sequence\n",
            "    if flag:\n",
            "        Sequence = local_sequence\n",
            "        return\n",
            "    after_return: list[str]\n",
            "untouched: list[str]\n",
        ),
        &[
            (161, 7, Some("reachable"), "Sequence[str]"),
            (182, 8, None, "Sequence[str]"),
        ],
    ),
    (
        "loops include their zero iteration exits",
        concat!(
            "Sequence = local_sequence\n",
            "for item in items:\n",
            "    from typing import Sequence\n",
            "    inside_for: list[str]\n",
            "after_for: list[str]\n",
            "from typing import Sequence\n",
            "while condition:\n",
            "    Sequence = local_sequence\n",
            "after_while: list[str]\n",
            "from typing import Sequence\n",
            "untouched: list[str]\n",
        ),
        &[(261, 11, None, "Sequence[str]")],
    ),
    (
        "try joins normal handlers else and applies finally",
        concat!(
            "from typing import Sequence\n",
            "try:\n",
            "    Sequence = local_sequence\n",
            "except Error:\n",
            "    from typing import Sequence\n",
            "after_ambiguous: list[str]\n",
            "Sequence = local_sequence\n",
            "try:\n",
            "    from typing import Sequence\n",
            "except Error:\n",
            "    from typing import Sequence\n",
            "else:\n",
            "    from typing import Sequence\n",
            "after_identical: list[str]\n",
            "Sequence = local_sequence\n",
            "try:\n",
            "    Sequence = local_sequence\n",
            "except Error:\n",
            "    Sequence = local_sequence\n",
            "finally:\n",
            "    from typing import Sequence\n",
            "after_finally: list[str]\n",
        ),
        &[(471, 22, None, "Sequence[str]")],
    ),
    (
        "with and except targets bind before their suites",
        concat!(
            "from typing import Sequence\n",
            "with manager as Sequence:\n",
            "    hidden_with: list[str]\n",
            "after_with: list[str]\n",
            "from typing import Sequence\n",
            "try:\n",
            "    work()\n",
            "except Error as Sequence:\n",
            "    hidden_except: list[str]\n",
            "after_except: list[str]\n",
            "from typing import Sequence\n",
            "untouched: list[str]\n",
        ),
        &[(265, 12, None, "Sequence[str]")],
    ),
    (
        "match includes unmatched flow unless irrefutable",
        concat!(
            "from typing import Sequence\n",
            "match value:\n",
            "    case {\"item\": Sequence}:\n",
            "        hidden_case: list[str]\n",
            "after_optional: list[str]\n",
            "Sequence = local_sequence\n",
            "match value:\n",
            "    case _:\n",
            "        from typing import Sequence\n",
            "after_irrefutable: list[str]\n",
            "from typing import Sequence\n",
            "match value:\n",
            "    case Sequence if guard:\n",
            "        from typing import Sequence\n",
            "after_guarded: list[str]\n",
            "from typing import Sequence\n",
            "untouched: list[str]\n",
        ),
        &[(412, 17, None, "Sequence[str]")],
    ),
    (
        "named expression binding precedes branch suites",
        concat!(
            "from typing import Sequence\n",
            "if (Sequence := local_sequence):\n",
            "    hidden_named: list[str]\n",
            "after_named: list[str]\n",
            "from typing import Sequence\n",
            "untouched: list[str]\n",
        ),
        &[(151, 6, None, "Sequence[str]")],
    ),
];

#[rstest::rstest]
#[case::if_joins_optional_and_identical_branches_1(0)]
#[case::returning_branch_is_not_a_continuation_exit_2(1)]
#[case::loops_include_their_zero_iteration_exits_3(2)]
#[case::try_joins_normal_handlers_else_and_applies_finally_4(3)]
#[case::with_and_except_targets_bind_before_their_suites_5(4)]
#[case::match_includes_unmatched_flow_unless_irrefutable_6(5)]
#[case::named_expression_binding_precedes_branch_suites_7(6)]
fn typing_import_rebinding_control_flow(#[case] scenario_index: usize) {
    let (name, source, expected) = TYPING_REBINDING_CASES[scenario_index];

    let output = analyze_types(source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| {
            (
                candidate.span.start,
                candidate.line,
                candidate.symbol.as_deref(),
                candidate.replacement.as_str(),
            )
        })
        .collect();

    assert_eq!(actual, expected, "{name}");
    assert_candidates_reparse(source, &output.candidates);
}

#[test]
fn typing_import_rebinding_finally_preserves_exit_categories() {
    let source = concat!(
        "from typing import Sequence\n",
        "\n",
        "def return_path(flag):\n",
        "    from typing import Sequence\n",
        "    try:\n",
        "        if flag:\n",
        "            Sequence = local_sequence\n",
        "            return\n",
        "    finally:\n",
        "        in_return_finally: list[str]\n",
        "    after_return: list[str]\n",
        "\n",
        "def raise_path(flag):\n",
        "    from typing import Sequence\n",
        "    try:\n",
        "        if flag:\n",
        "            Sequence = local_sequence\n",
        "            raise Error\n",
        "    finally:\n",
        "        in_raise_finally: list[str]\n",
        "    after_raise: list[str]\n",
        "\n",
        "def break_path(flag, items):\n",
        "    from typing import Sequence\n",
        "    for item in items:\n",
        "        try:\n",
        "            if flag:\n",
        "                Sequence = local_sequence\n",
        "                break\n",
        "        finally:\n",
        "            in_break_finally: list[str]\n",
        "        after_break: list[str]\n",
        "    after_break_loop: list[str]\n",
        "\n",
        "def continue_path(flag, items):\n",
        "    from typing import Sequence\n",
        "    for item in items:\n",
        "        try:\n",
        "            if flag:\n",
        "                Sequence = local_sequence\n",
        "                continue\n",
        "        finally:\n",
        "            in_continue_finally: list[str]\n",
        "    after_continue_loop: list[str]\n",
        "\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (235, 9, 11, Some("return_path"), "Sequence[str]"),
            (454, 9, 21, Some("raise_path"), "Sequence[str]"),
            (725, 9, 32, Some("break_path"), "Sequence[str]"),
            (1063, 9, 46, None, "Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_match_propagates_failed_case_bindings() {
    let source = concat!(
        "from typing import Sequence\n",
        "match value:\n",
        "    case 0 if (Sequence := local_sequence):\n",
        "        pass\n",
        "    case _:\n",
        "        after_guard_failure: list[str]\n",
        "after_guard_match: list[str]\n",
        "from typing import Sequence\n",
        "match value:\n",
        "    case [Sequence, 0]:\n",
        "        pass\n",
        "    case _:\n",
        "        after_pattern_failure: list[str]\n",
        "after_pattern_match: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(379, 9, 16, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_loop_heads_include_back_edges() {
    let source = concat!(
        "from typing import Sequence\n",
        "\n",
        "def for_back_edge(items):\n",
        "    from typing import Sequence\n",
        "    for item in items:\n",
        "        before_rebind: list[str]\n",
        "        Sequence = local_sequence\n",
        "    after_for: list[str]\n",
        "\n",
        "def while_continue(condition, flag):\n",
        "    from typing import Sequence\n",
        "    while condition:\n",
        "        before_continue: list[str]\n",
        "        if flag:\n",
        "            Sequence = local_sequence\n",
        "            continue\n",
        "        break\n",
        "    after_while: list[str]\n",
        "\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(457, 9, 20, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_function_defaults() {
    let source = concat!(
        "from typing import Sequence\n",
        "def default_binding(\n",
        "    value: list[str] = (Sequence := local_sequence),\n",
        "    other: list[str] = None,\n",
        ") -> list[str]:\n",
        "    pass\n",
        "after_default: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(220, 9, 9, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_function_decorators() {
    let source = concat!(
        "from typing import Sequence\n",
        "@(Sequence := decorator)\n",
        "def decorated(value: list[str]) -> list[str]:\n",
        "    pass\n",
        "after_decorator: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(174, 9, 7, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_class_expressions() {
    let source = concat!(
        "from typing import Sequence\n",
        "@(Sequence := decorator)\n",
        "class Decorated:\n",
        "    in_decorated: list[str]\n",
        "after_decorated: list[str]\n",
        "from typing import Sequence\n",
        "class Based((Sequence := Base)):\n",
        "    in_based: list[str]\n",
        "after_based: list[str]\n",
        "from typing import Sequence\n",
        "class Keyword(metaclass=(Sequence := Meta)):\n",
        "    in_keyword: list[str]\n",
        "after_keyword: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(396, 9, 15, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_definition_header_lambda_defaults_only() {
    let source = concat!(
        "from typing import Sequence\n",
        "with_default = lambda value=(Sequence := local_sequence): value\n",
        "after_default: list[str]\n",
        "from typing import Sequence\n",
        "body_only = lambda: (Sequence := local_sequence)\n",
        "after_body: list[str]\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (206, 9, 6, None, "Sequence[str]"),
            (227, 9, 7, None, "Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_annotated_assignment_execution_order() {
    let source = concat!(
        "from typing import Sequence\n",
        "Sequence: list[str] = object\n",
        "after_module_valued: list[str]\n",
        "from typing import Sequence\n",
        "Sequence: object\n",
        "after_module_valueless: list[str]\n",
        "from typing import Sequence\n",
        "value: list[str] = (Sequence := object)\n",
        "after_module_rhs: list[str]\n",
        "from typing import Sequence\n",
        "class ClassValued:\n",
        "    Sequence: list[str] = object\n",
        "    after_valued: list[str]\n",
        "class ClassValueless:\n",
        "    Sequence: object\n",
        "    after_valueless: list[str]\n",
        "class ClassRhs:\n",
        "    value: list[str] = (Sequence := object)\n",
        "    after_rhs: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(source, &[(569, 9, 21, None, "Sequence[str]")]);
}

#[test]
fn typing_import_rebinding_class_external_writes_project_to_target_scope() {
    let source = concat!(
        "from typing import Sequence\n",
        "class DirectWrite:\n",
        "    global Sequence\n",
        "    Sequence = object\n",
        "    def method(self):\n",
        "        hidden: list[str]\n",
        "after_direct_write: list[str]\n",
        "class DirectReimport:\n",
        "    global Sequence\n",
        "    from typing import Sequence\n",
        "    def method(self):\n",
        "        restored: list[str]\n",
        "after_direct_reimport: list[str]\n",
        "Sequence = object\n",
        "import typing as t\n",
        "class AliasWrite:\n",
        "    global t\n",
        "    t = object\n",
        "    def method(self):\n",
        "        hidden: list[str]\n",
        "after_alias_write: list[str]\n",
        "class AliasReimport:\n",
        "    global t\n",
        "    import typing as t\n",
        "    def method(self):\n",
        "        restored: list[str]\n",
        "after_alias_reimport: list[str]\n",
        "t = object\n",
        "def direct_outer():\n",
        "    from typing import Sequence\n",
        "    class DirectWrite:\n",
        "        nonlocal Sequence\n",
        "        Sequence = object\n",
        "        def method(self):\n",
        "            hidden: list[str]\n",
        "    after_write: list[str]\n",
        "    class DirectReimport:\n",
        "        nonlocal Sequence\n",
        "        from typing import Sequence\n",
        "        def method(self):\n",
        "            restored: list[str]\n",
        "    after_reimport: list[str]\n",
        "def alias_outer():\n",
        "    import typing as t\n",
        "    class AliasWrite:\n",
        "        nonlocal t\n",
        "        t = object\n",
        "        def method(self):\n",
        "            hidden: list[str]\n",
        "    after_write: list[str]\n",
        "    class AliasReimport:\n",
        "        nonlocal t\n",
        "        import typing as t\n",
        "        def method(self):\n",
        "            restored: list[str]\n",
        "    after_reimport: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (281, 9, 12, Some("DirectReimport.method"), "Sequence[str]"),
            (581, 9, 26, Some("AliasReimport.method"), "t.Sequence[str]"),
            (
                980,
                9,
                41,
                Some("direct_outer.DirectReimport.method"),
                "Sequence[str]",
            ),
            (1010, 9, 42, Some("direct_outer"), "Sequence[str]"),
            (
                1324,
                9,
                55,
                Some("alias_outer.AliasReimport.method"),
                "t.Sequence[str]",
            ),
            (1354, 9, 56, Some("alias_outer"), "t.Sequence[str]"),
        ],
    );
}

#[test]
fn typing_import_rebinding_nested_class_globals_preserve_function_fallback() {
    let source = concat!(
        "Sequence = object\n",
        "t = object\n",
        "def direct_outer():\n",
        "    from typing import Sequence\n",
        "    class GlobalWrite:\n",
        "        global Sequence\n",
        "        Sequence = object\n",
        "        def method(self):\n",
        "            preserved: list[str]\n",
        "    after_class: list[str]\n",
        "def alias_outer():\n",
        "    import typing as t\n",
        "    class GlobalWrite:\n",
        "        global t\n",
        "        t = object\n",
        "        def method(self):\n",
        "            preserved: list[str]\n",
        "    after_class: list[str]\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    assert_type_list_sequence_sites(
        source,
        &[
            (
                203,
                9,
                9,
                Some("direct_outer.GlobalWrite.method"),
                "Sequence[str]",
            ),
            (230, 9, 10, Some("direct_outer"), "Sequence[str]"),
            (
                390,
                9,
                17,
                Some("alias_outer.GlobalWrite.method"),
                "t.Sequence[str]",
            ),
            (417, 9, 18, Some("alias_outer"), "t.Sequence[str]"),
        ],
    );
}

#[rstest::rstest]
#[case::class_inner_pass("class Inner: pass")]
#[case::def_first_self_pass("def first(self): pass")]
fn class_context_restores_method_fallback_after_nested_definitions(
    #[case] nested: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    let (import, binding, replacement) = [
        ("from typing import Sequence", "Sequence", "Sequence[str]"),
        ("import typing as t", "t", "t.Sequence[str]"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    // Class locals must not replace the method's enclosing import.
    let source = format!(
        "{import}\nclass Outer:\n    {binding} = object\n    {nested}\n    def method(self):\n        value: list[str]\n"
    );

    let output = analyze_types(&source);

    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
        .map(|candidate| (candidate.symbol.as_deref(), candidate.replacement.as_str()))
        .collect();

    // Returning from either nested scope restores Outer's fallback.
    assert_eq!(actual, [(Some("Outer.method"), replacement)], "{source}");
}

proptest! {
    #[test]
    fn class_context_preserves_enclosing_import_through_nested_classes(depth in 1usize..9) {
        // Every intermediate class shadows the module import.
        let mut source = String::from("from typing import Sequence\n");
        for level in 0..depth {
            let indent = "    ".repeat(level);
            writeln!(source, "{indent}class C{level}:").unwrap();
            writeln!(source, "{indent}    Sequence = object").unwrap();
        }
        let indent = "    ".repeat(depth);
        writeln!(source, "{indent}def method(self):\n{indent}    value: list[str]").unwrap();

        let output = analyze_types(&source);

        let actual: Vec<_> = output.candidates.iter()
            .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
            .map(|candidate| candidate.replacement.as_str())
            .collect();

        // Method lookup skips every class-local shadow.
        prop_assert_eq!(actual, ["Sequence[str]"]);
    }
}

#[test]
fn typing_import_rebinding_try_handler_includes_unknown_wildcard_effect() {
    let source = concat!(
        "from typing import Sequence\n",
        "try:\n",
        "    from unknown import *\n",
        "    raise Error\n",
        "except Error:\n",
        "    hidden_direct: list[str]\n",
        "Sequence = object\n",
        "import typing as t\n",
        "try:\n",
        "    from unknown import *\n",
        "    raise Error\n",
        "except Error:\n",
        "    hidden_alias: list[str]\n",
        "t = object\n",
        "from typing import Sequence\n",
        "untouched: list[str]\n",
    );
    // Restoring Sequence does not establish that list survived the wildcard import.
    assert_type_list_sequence_sites(source, &[]);
}

#[test]
fn binding_flow_internal_corpus_projects_categorized_exit_facts() {
    let items = BINDING_FLOW_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<BindingFlowCorpusCase>(line).unwrap())
        .collect::<Vec<_>>();
    let expected_ids = [
        "typing_loop_zero_iteration",
        "typing_loop_continue_backedge",
        "typing_loop_break_exit",
        "typing_match_guard_binding",
    ];
    let mut actual_ids = items
        .iter()
        .filter(|item| item.mode == "internal-fixture")
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    actual_ids.sort_unstable();
    let mut expected_sorted = expected_ids;
    expected_sorted.sort_unstable();

    assert_eq!(actual_ids, expected_sorted);

    for id in expected_ids {
        let item = items.iter().find(|item| item.id == id).unwrap_or_else(
            // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
            #[cfg_attr(coverage_nightly, coverage(off))]
            || panic!("missing internal-fixture case {id}"),
        );
        assert_eq!(item.mode, "internal-fixture", "case={id}");
        let expected = BindingFlowTestSnapshot {
            fallthrough: item.expected_fallthrough.clone(),
            breaks: item.expected_breaks.clone(),
            continues: item.expected_continues.clone(),
            terminates: item.expected_terminates.clone(),
        };
        assert_eq!(
            binding_flow_test_snapshot(&item.source),
            expected,
            "case={id}"
        );
    }
    for (item, expected_loop_head) in items
        .iter()
        .filter(|item| expected_ids.contains(&item.id.as_str()))
        .filter_map(|item| item.expected_loop_head.as_ref().map(|head| (item, head)))
    {
        assert_eq!(
            binding_flow_loop_head_snapshot(&item.source, true),
            *expected_loop_head,
            "loop head case={}",
            item.id
        );
    }
}

#[test]
fn binding_flow_continue_edge_is_observationally_required() {
    let source = concat!(
        "from typing import Sequence\n",
        "while condition:\n",
        "    Sequence = local_sequence\n",
        "    continue\n",
    );

    let with_continue = binding_flow_loop_head_snapshot(source, true);
    let without_continue = binding_flow_loop_head_snapshot(source, false);

    assert_eq!(with_continue, Vec::<String>::new());
    assert_eq!(
        without_continue,
        vec!["direct:Sequence=typing.Sequence".to_owned()]
    );
    assert_ne!(with_continue, without_continue);
}

#[test]
fn binding_flow_finally_restores_the_original_exit_category() {
    let source = concat!(
        "from typing import Sequence\n",
        "try:\n",
        "    Sequence = local_sequence\n",
        "    continue\n",
        "finally:\n",
        "    from typing import Sequence\n",
    );
    let known_sequence = vec!["direct:Sequence=typing.Sequence".to_owned()];

    assert_eq!(
        binding_flow_test_snapshot(source),
        BindingFlowTestSnapshot {
            fallthrough: Vec::new(),
            breaks: Vec::new(),
            continues: vec![known_sequence],
            terminates: Vec::new(),
        }
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
            .all(|candidate| !candidate.operator.as_str().starts_with("type_"))
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
            .all(|candidate| !candidate.operator.as_str().starts_with("type_"))
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
            candidate.operator.as_str().starts_with("type_")
                && candidate.operator.as_str() != "type_list_sequence"
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

#[rstest::rstest]
#[case::abc_sequence_1(0)]
#[case::t_sequence_2(1)]
#[case::collections_abc_sequence_3(2)]
fn qualified_annotation_spelling_uses_the_matching_provider_member_scenario_1(
    #[case] scenario_index: usize,
) {
    let parsed =
        parse_module("import collections.abc as abc\nimport typing as t\nimport collections.abc\n")
            .unwrap();
    let mut imports = super::KnownImports::default();
    for statement in &parsed.syntax().body {
        imports.transfer_statement(statement);
    }
    let targets = &["typing.AbstractSet", "collections.abc.Set"];

    let (source, expected) = [
        ("abc.Sequence", "abc.Set"),
        ("t.Sequence", "t.AbstractSet"),
        ("collections.abc.Sequence", "collections.abc.Set"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    assert_eq!(
        imports.spelling_for(source, targets).as_deref(),
        Some(expected)
    );
}

#[test]
fn qualified_annotation_spelling_uses_the_matching_provider_member() {
    let parsed =
        parse_module("import collections.abc as abc\nimport typing as t\nimport collections.abc\n")
            .unwrap();
    let mut imports = super::KnownImports::default();
    for statement in &parsed.syntax().body {
        imports.transfer_statement(statement);
    }
    let targets = &["typing.AbstractSet", "collections.abc.Set"];

    assert_eq!(
        imports.spelling_for("collections.other.Sequence", targets),
        None
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
            .all(|candidate| candidate.operator.as_str() != "type_list_sequence")
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
            payload: super::payload::PayloadBudget::default(),
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
            .filter(|candidate| candidate.operator.as_str().starts_with("type_"))
            .map(|candidate| candidate.original.as_str())
            .collect::<Vec<_>>(),
        vec!["list[str]", "list[str]"]
    );
    let symbol_output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
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
            .filter(|candidate| candidate.operator.as_str().starts_with("type_"))
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
#[test]
fn grammar_tokens_do_not_emit_expression_operator_mutations() {
    let source = concat!(
        "from package import *\n",
        "for item in items:\n    pass\n",
        "values = [item for item in items]\n",
        "result = call(*args, **kwargs)\n",
        "match value:\n    case left | right:\n        pass\n",
        "member = item in items\n",
        "product = left * right\n",
        "union = left | right\n",
        "nested_membership = [item for item in items] == values\n",
        "nested_unpacking = call(*args) * value\n",
    );

    let output = analyze(source);

    assert!(!output.candidates.iter().any(|candidate| {
        candidate.line <= 8
            && matches!(
                candidate.operator.as_str(),
                "membership" | "binary_mul_div" | "bitwise_and_or"
            )
    }));
    assert!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 9 && candidate.operator.as_str() == "membership")
    );
    assert!(
        output.candidates.iter().any(
            |candidate| candidate.line == 10 && candidate.operator.as_str() == "binary_mul_div"
        )
    );
    assert!(
        output.candidates.iter().any(
            |candidate| candidate.line == 11 && candidate.operator.as_str() == "bitwise_and_or"
        )
    );
    assert!(
        !output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 12 && candidate.operator.as_str() == "membership")
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.line == 13 && candidate.operator.as_str() == "binary_mul_div"
            })
            .count(),
        1
    );
}

proptest! {
    #[test]
    fn arbitrary_python_input_has_ordered_in_bounds_candidates(source in ".{0,4096}") {
        let output = analyze(&source);
        let mut previous_start = 0;
        for candidate in output.candidates {
            let start = usize::try_from(candidate.span.start).expect("span start fits usize");
            let length = usize::try_from(candidate.span.length).expect("span length fits usize");
            let end = start.checked_add(length).expect("candidate span does not overflow");

            prop_assert!(previous_start <= start);
            prop_assert!(end <= source.len());
            prop_assert_eq!(&source[start..end], &candidate.original);
            previous_start = start;
        }
    }
}

#[test]
fn emits_the_mvp_operator_replacements_in_source_order() {
    let selected_operators: &[&str] = &[
        "compare_eq_ne",
        "boolean_and_or",
        "membership",
        "identity",
        "augmented_add_sub",
        "binary_mul_div",
        "binary_floor_mod",
        "collection_list_tuple",
        "remove_not",
        "unary_sign",
        "boolean_literal",
    ];
    let source = "def f(a, b, xs, flag):\n    value = a == b and a not in xs and a is not b\n    value += a * b // 2 % 2\n    return not flag, +a, -b, True, False\n";

    let output = analyze_selected(selected_operators, source);
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
            (
                "not flag, +a, -b, True, False",
                "[not flag, +a, -b, True, False]",
                "collection_list_tuple"
            ),
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
    let selected_operators: &[&str] = &[
        "compare_eq_ne",
        "remove_not",
        "boolean_and_or",
        "binary_add_sub",
    ];
    let source =
        "top = left == right\ndef decide(flag, value):\n    return not flag or value + 1\n";

    let output = analyze_selected(selected_operators, source);
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
        .find(|candidate| candidate.operator.as_str() == "remove_not")
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
    let selected_operators: &[&str] = &[
        "compare_eq_ne",
        "compare_order",
        "membership",
        "identity",
        "boolean_and_or",
        "binary_add_sub",
        "binary_mul_div",
        "break_continue",
    ];
    let source = "def f(a, b, xs):\n    a != b\n    a < b\n    a <= b\n    a > b\n    a >= b\n    a in xs\n    a is b\n    a or b\n    a + b\n    a - b\n    a / b\n    while a:\n        break\n        continue\n";

    let output = analyze_selected(selected_operators, source);
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
fn reports_columns_without_counting_a_leading_file_bom() {
    let source = "\u{feff}x = 1 + 2\n";
    let candidate = analyze(source)
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .find(|candidate| candidate.original == "+")
        .expect("binary addition candidate");

    assert_eq!(
        candidate.span,
        ByteSpan {
            start: 9,
            length: 1
        }
    );
    assert_eq!((candidate.line, candidate.column), (1, 6));
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
fn filters_candidates_by_line_and_symbol() {
    let source = "def selected(a, b, c, d):\n    first = a == b\n    return c == d\n\ndef other(a, b):\n    return a == b\n";

    let output = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[
            LineRange { start: 2, end: 2 },
            LineRange { start: 6, end: 6 },
        ],
        &["pkg.sample:selected".to_owned()],
        10_000,
        source,
    );

    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| (candidate.line, candidate.symbol.as_deref()))
            .collect::<Vec<_>>(),
        vec![(2, Some("selected"))]
    );
}

#[test]
fn reports_invalid_syntax_without_candidates() {
    let output = analyze("def broken(:\n");

    assert_eq!(output.candidates, Vec::new());
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::InvalidSyntax
    );
    assert!(!output.truncated);
}

#[test]
fn annotation_scope_marker_projections_use_site_entry_and_comprehension_scope() {
    let annotation_source = concat!(
        "def outer():\n",
        "    from typing import Sequence\n",
        "    def inner():\n",
        "        nonlocal Sequence\n",
        "        before: list[str]\n",
        "        Sequence = object\n",
    );
    let site: AnnotationSiteTestSnapshot =
        annotation_site_test_snapshot(annotation_source, "list[str]").unwrap();

    assert_eq!(site.scope, "function");
    assert_eq!(site.symbol.as_deref(), Some("outer.inner"));
    assert_eq!(site.facts, vec!["direct:Sequence=typing.Sequence"]);

    let comprehension_source = concat!(
        "def collect(values):\n",
        "    result = [list(item) for list in list(values)]\n",
    );
    let first_iterable: NameResolutionTestSnapshot =
        name_resolution_test_snapshot(comprehension_source, "list(values)", "list").unwrap();
    let body: NameResolutionTestSnapshot =
        name_resolution_test_snapshot(comprehension_source, "list(item)", "list").unwrap();
    assert_eq!(first_iterable.resolution, "definitely-builtin");
    assert_eq!(body.resolution, "shadowed");
}

#[test]
fn loop_back_edge_bindings_make_builtin_resolution_uncertain() {
    let cases = [
        (
            "module for body",
            concat!(
                "for item in values:\n",
                "    current = list(item)\n",
                "    list = custom_list\n",
            ),
            "list(item)",
            "unknown",
        ),
        (
            "class for body",
            concat!(
                "class Namespace:\n",
                "    for item in values:\n",
                "        current = list(item)\n",
                "        list = custom_list\n",
            ),
            "list(item)",
            "unknown",
        ),
        (
            "while test",
            concat!("while list(items):\n", "    list = custom_list\n",),
            "list(items)",
            "unknown",
        ),
        (
            "nested same-scope loop",
            concat!(
                "for outer in values:\n",
                "    current = list(outer)\n",
                "    for inner in values:\n",
                "        list = custom_list\n",
            ),
            "list(outer)",
            "unknown",
        ),
        (
            "module loop through class body",
            concat!(
                "for item in values:\n",
                "    class Namespace:\n",
                "        current = list(item)\n",
                "    list = custom_list\n",
            ),
            "list(item)",
            "unknown",
        ),
        (
            "for target load",
            concat!(
                "for sink[list(item)] in values:\n",
                "    list = custom_list\n",
            ),
            "list(item)",
            "unknown",
        ),
    ];

    assert_name_resolution_cases(&cases);
}

#[test]
fn loop_back_edge_boundaries_preserve_definite_builtin_resolution() {
    let cases = [
        (
            "one-time for iterable",
            concat!("for item in list(values):\n", "    list = custom_list\n",),
            "list(values)",
            "definitely-builtin",
        ),
        (
            "nested function binding",
            concat!(
                "for item in values:\n",
                "    current = list(item)\n",
                "    def helper():\n",
                "        list = custom_list\n",
            ),
            "list(item)",
            "definitely-builtin",
        ),
        (
            "nested class binding",
            concat!(
                "for item in values:\n",
                "    current = list(item)\n",
                "    class Namespace:\n",
                "        list = custom_list\n",
            ),
            "list(item)",
            "definitely-builtin",
        ),
        (
            "class loop binding outside nested function",
            concat!(
                "class Namespace:\n",
                "    for item in values:\n",
                "        def helper():\n",
                "            return list(item)\n",
                "        list = custom_list\n",
            ),
            "list(item)",
            "definitely-builtin",
        ),
        (
            "for else binding",
            concat!(
                "for item in values:\n",
                "    current = list(item)\n",
                "else:\n",
                "    list = custom_list\n",
            ),
            "list(item)",
            "definitely-builtin",
        ),
        (
            "while else binding",
            concat!(
                "while condition:\n",
                "    current = list(items)\n",
                "else:\n",
                "    list = custom_list\n",
            ),
            "list(items)",
            "definitely-builtin",
        ),
    ];

    assert_name_resolution_cases(&cases);
}

fn assert_name_resolution_cases(cases: &[(&str, &str, &str, &str)]) {
    for (case, source, marker, expected) in cases {
        let snapshot = name_resolution_test_snapshot(source, marker, "list").unwrap_or_else(
            // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
            #[cfg_attr(coverage_nightly, coverage(off))]
            |error| panic!("{error}; case={case}"),
        );
        assert_eq!(snapshot.resolution, *expected, "case={case}");
    }
}

#[test]
fn annotation_scope_marker_projections_reject_infrastructure_setup_errors() {
    assert!(
        annotation_site_test_snapshot("def broken(:\n", "broken")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );
    assert!(
        annotation_site_test_snapshot("first: list[str]\nsecond: list[str]\n", "list[str]")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );
    assert!(
        name_resolution_test_snapshot("ValueError()\n", "ValueError", "Value")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );
}

#[test]
fn annotation_scope_private_correspondence_matches_lean() {
    let cases = ANNOTATION_SCOPE_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<AnnotationScopeCorpusCase>(line).unwrap())
        .filter(|item| item.mode == "internal-fixture")
        .collect::<Vec<_>>();

    assert_eq!(cases.len(), 20);
    for item in &cases {
        assert_eq!(item.schema, 1, "{}", item.id);
        assert!(!item.scenario.is_empty(), "{}", item.id);
        assert!(item.expected_operator.is_none(), "{}", item.id);
        assert!(item.expected_original.is_none(), "{}", item.id);
        assert!(item.expected_replacement.is_none(), "{}", item.id);
        assert!(
            matches!(item.observation_kind.as_str(), "annotation" | "resolution"),
            "{}",
            item.id
        );
    }
    for item in cases
        .iter()
        .filter(|item| item.observation_kind == "annotation")
    {
        let snapshot = annotation_site_test_snapshot(&item.source, &item.marker).unwrap_or_else(
            // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
            #[cfg_attr(coverage_nightly, coverage(off))]
            |error| panic!("{error}; case={}", item.id),
        );
        let marker_start = item.source.find(&item.marker).unwrap();
        let marker_end = marker_start + item.marker.len();
        assert!(snapshot.start <= marker_start, "{}", item.id);
        assert!(marker_end <= snapshot.end, "{}", item.id);
        assert_eq!(snapshot.facts, item.expected_facts, "{}", item.id);
        assert_eq!(snapshot.symbol, item.expected_symbol, "{}", item.id);
        assert_eq!(
            Some(snapshot.scope),
            item.expected_scope.as_deref(),
            "{}",
            item.id
        );
        assert_eq!(
            !snapshot.facts.is_empty(),
            item.expected_present,
            "{}",
            item.id
        );
    }
    for item in cases
        .iter()
        .filter(|item| item.observation_kind == "resolution")
    {
        let snapshot = name_resolution_test_snapshot(&item.source, &item.marker, "list")
            .unwrap_or_else(
                // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
                #[cfg_attr(coverage_nightly, coverage(off))]
                |error| panic!("{error}; case={}", item.id),
            );
        let marker_start = item.source.find(&item.marker).unwrap();
        let name_start = marker_start + item.marker.find("list").unwrap();
        assert_eq!(snapshot.start, name_start, "{}", item.id);
        assert_eq!(
            Some(snapshot.resolution),
            item.expected_resolution.as_deref(),
            "{}",
            item.id
        );
        assert_eq!(
            snapshot.resolution == "definitely-builtin",
            item.expected_present,
            "{}",
            item.id
        );
    }
}

#[test]
fn annotation_scope_private_projection_detects_wrong_site_and_scope_stage() {
    let site_source = concat!(
        "def outer():\n",
        "    from typing import Sequence\n",
        "    def inner():\n",
        "        nonlocal Sequence\n",
        "        before: list[str]\n",
        "        Sequence = object\n",
        "        after: tuple[str]\n",
    );
    let entry = annotation_site_test_snapshot(site_source, "list[str]").unwrap();
    let suite_later = annotation_site_test_snapshot(site_source, "tuple[str]").unwrap();

    assert_ne!(entry.facts, suite_later.facts);

    let comprehension_source = concat!(
        "def collect(values):\n",
        "    result = [list(item) for list in list(values)]\n",
    );
    let first =
        name_resolution_test_snapshot(comprehension_source, "list(values)", "list").unwrap();
    let body = name_resolution_test_snapshot(comprehension_source, "list(item)", "list").unwrap();
    assert_ne!(first.resolution, body.resolution);
}

#[test]
fn exception_match_binding_internal_corpus() {
    type AnnotationObserver = fn(&str, &str) -> Result<Vec<String>, String>;

    let cases = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(cases.len(), 25);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        15
    );
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "model-only")
            .count(),
        2
    );
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 8);
    let mut model_only_ids = cases
        .iter()
        .filter(|item| item.mode == "model-only")
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    model_only_ids.sort_unstable();
    assert_eq!(
        model_only_ids,
        [
            "handler_nonselected_join",
            "match_partial_failure_next_case"
        ]
    );
    let cases = cases
        .into_iter()
        .filter(|item| item.mode == "internal-fixture")
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 15);

    for item in &cases {
        assert_eq!(item.schema, 1, "{}", item.id);
        assert!(matches!(item.family.as_str(), "handler" | "match-case"));
        assert!(!item.name.is_empty(), "{}", item.id);
        assert!(item.operator.is_none(), "{}", item.id);
        assert!(item.original.is_none(), "{}", item.id);
        assert!(item.replacement.is_none(), "{}", item.id);
        assert!(item.symbol.is_none(), "{}", item.id);
        assert!(
            matches!(item.observation_kind.as_str(), "annotation" | "exits"),
            "{}",
            item.id
        );
    }
    let annotation_observers: [(&str, AnnotationObserver); 2] = [
        ("handler", binding_flow_marker_snapshot),
        ("site", |source, marker| {
            annotation_site_test_snapshot(source, marker).map(|snapshot| snapshot.facts)
        }),
    ];
    for (observer_kind, observe) in annotation_observers {
        for item in cases.iter().filter(|item| {
            item.observation_kind == "annotation"
                && (item.id == "handler_type_before_target") == (observer_kind == "handler")
        }) {
            assert!(item.expected_resolution.is_none(), "{}", item.id);
            assert!(item.expected_exit_category.is_none(), "{}", item.id);
            let actual_facts = observe(&item.source, &item.marker).unwrap_or_else(
                // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
                #[cfg_attr(coverage_nightly, coverage(off))]
                |error| panic!("{error}; case={}", item.id),
            );
            assert_eq!(actual_facts, item.expected_facts, "{}", item.id);
            assert_eq!(
                !actual_facts.is_empty(),
                item.expected_present,
                "{}",
                item.id
            );
        }
    }
    for item in cases.iter().filter(|item| item.observation_kind == "exits") {
        assert!(item.expected_resolution.is_none(), "{}", item.id);
        let category = item.expected_exit_category.as_deref().unwrap();
        assert!(["fallthrough", "break", "continue", "terminate"].contains(&category));
        let actual_facts = binding_flow_handler_exit_snapshot(&item.source, &item.marker, category)
            .unwrap_or_else(
                // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
                #[cfg_attr(coverage_nightly, coverage(off))]
                |error| panic!("{error}; case={}", item.id),
            );
        assert_eq!(actual_facts, item.expected_facts, "{}", item.id);
        assert_eq!(
            !actual_facts.is_empty(),
            item.expected_present,
            "{}",
            item.id
        );
    }
}

#[test]
fn exception_match_binding_private_projection_detects_broken_transitions() {
    let cases = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .collect::<Vec<_>>();
    let case = |id: &str| {
        cases.iter().find(|item| item.id == id).unwrap_or_else(
            // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
            #[cfg_attr(coverage_nightly, coverage(off))]
            || panic!("missing sensitivity case {id}"),
        )
    };

    let handler_type = case("handler_type_before_target");

    assert_eq!(
        binding_flow_marker_snapshot(&handler_type.source, &handler_type.marker).unwrap(),
        handler_type.expected_facts,
    );
    assert_eq!(
        binding_flow_marker_snapshot_with_mutation(
            &handler_type.source,
            &handler_type.marker,
            BindingFlowTestMutation::BindHandlerTargetBeforeType,
        )
        .unwrap(),
        Vec::<String>::new(),
    );

    let pattern_failure = case("match_partial_failure_observable");
    assert_eq!(
        binding_flow_marker_snapshot(&pattern_failure.source, &pattern_failure.marker).unwrap(),
        pattern_failure.expected_facts,
    );
    assert_eq!(
        binding_flow_marker_snapshot_with_mutation(
            &pattern_failure.source,
            &pattern_failure.marker,
            BindingFlowTestMutation::UsePrePatternFailureEnvironment,
        )
        .unwrap(),
        vec!["direct:Sequence=typing.Sequence"]
    );

    let false_guard = case("match_false_guard_next_case");
    assert_eq!(
        binding_flow_marker_snapshot(&false_guard.source, &false_guard.marker).unwrap(),
        false_guard.expected_facts,
    );
    assert_eq!(
        binding_flow_marker_snapshot_with_mutation(
            &false_guard.source,
            &false_guard.marker,
            BindingFlowTestMutation::UsePreGuardFailureEnvironment,
        )
        .unwrap(),
        vec!["direct:Sequence=typing.Sequence"]
    );

    let refutable = case("match_refutable_unmatched_join");
    let correct = binding_flow_marker_snapshot(&refutable.source, &refutable.marker).unwrap();
    let broken = binding_flow_marker_snapshot_with_mutation(
        &refutable.source,
        &refutable.marker,
        BindingFlowTestMutation::DropRefutableUnmatched,
    )
    .unwrap();
    assert_eq!(correct, refutable.expected_facts);
    assert_eq!(broken, vec!["direct:Sequence=typing.Sequence"]);
    assert_ne!(broken, correct);
}

#[rstest::rstest]
#[case::fallthrough("fallthrough", BindingFlowTestMutation::OmitHandlerFallthroughCleanup)]
#[case::input_break("break", BindingFlowTestMutation::OmitHandlerBreakCleanup)]
#[case::input_continue("continue", BindingFlowTestMutation::OmitHandlerContinueCleanup)]
#[case::terminate("terminate", BindingFlowTestMutation::OmitHandlerTerminateCleanup)]
fn exception_match_binding_handler_cleanup_rows_observe_post_cleanup_state(
    #[case] category: &str,
    #[case] mutation: BindingFlowTestMutation,
) {
    let cases = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .filter(|item| item.observation_kind == "exits")
        .collect::<Vec<_>>();

    assert_eq!(cases.len(), 5);
    assert!(cases.iter().all(|item| matches!(
        item.expected_exit_category.as_deref(),
        Some("fallthrough" | "break" | "continue" | "terminate")
    )));

    for item in cases
        .iter()
        .filter(|item| item.expected_exit_category.as_deref() == Some(category))
    {
        assert_eq!(
            binding_flow_handler_exit_snapshot(&item.source, &item.marker, category)
                .unwrap_or_else(
                    // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
                    #[cfg_attr(coverage_nightly, coverage(off))]
                    |error| panic!("{}: {error}", item.id)
                ),
            item.expected_facts,
            "{} must observe the categorized exit after handler cleanup",
            item.id
        );
        assert_eq!(
            binding_flow_handler_exit_snapshot_with_mutation(
                &item.source,
                &item.marker,
                category,
                mutation,
            )
            .unwrap_or_else(
                // Fixed test literals and checked-in corpus IDs cannot reach this diagnostic.
                #[cfg_attr(coverage_nightly, coverage(off))]
                |error| panic!("{}: {error}", item.id)
            ),
            vec!["direct:Sequence=typing.Sequence"],
            "{} must detect omitted {category} cleanup",
            item.id
        );
    }
}

#[test]
fn exception_match_binding_pattern_failure_mutation_preserves_reachability() {
    let item = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .find(|item| item.id == "match_partial_failure_observable")
        .expect("partial-pattern failure corpus row");

    assert_eq!(
        binding_flow_marker_snapshot(&item.source, &item.marker).unwrap(),
        item.expected_facts
    );
    assert_eq!(
        binding_flow_marker_snapshot_with_mutation(
            &item.source,
            &item.marker,
            BindingFlowTestMutation::UsePrePatternFailureEnvironment,
        )
        .unwrap(),
        vec!["direct:Sequence=typing.Sequence"]
    );
}

#[test]
fn exception_match_binding_guard_failure_mutation_preserves_reachability() {
    let item = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .find(|item| item.id == "match_false_guard_next_case")
        .expect("false-guard failure corpus row");

    assert_eq!(
        binding_flow_marker_snapshot(&item.source, &item.marker).unwrap(),
        item.expected_facts
    );
    assert_eq!(
        binding_flow_marker_snapshot_with_mutation(
            &item.source,
            &item.marker,
            BindingFlowTestMutation::UsePreGuardFailureEnvironment,
        )
        .unwrap(),
        vec!["direct:Sequence=typing.Sequence"]
    );
}

#[test]
fn exception_match_binding_refutable_unmatched_mutation_changes_observation() {
    let item = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .find(|item| item.id == "match_refutable_unmatched_join")
        .expect("refutable-unmatched corpus row");
    let correct = binding_flow_marker_snapshot(&item.source, &item.marker).unwrap();
    let broken = binding_flow_marker_snapshot_with_mutation(
        &item.source,
        &item.marker,
        BindingFlowTestMutation::DropRefutableUnmatched,
    )
    .unwrap();

    assert_eq!(correct, item.expected_facts);
    assert_ne!(
        broken, correct,
        "dropping the unmatched path must be observable"
    );
}

#[test]
fn exception_match_binding_marker_projection_rejects_unreachable_statements() {
    let source = concat!(
        "def run():\n",
        "    from typing import Sequence\n",
        "    reachable: list[str]\n",
        "    return 1  # reachable-return\n",
        "    unreachable: tuple[str]\n",
    );
    let known_sequence = vec!["direct:Sequence=typing.Sequence".to_owned()];

    assert_eq!(
        binding_flow_marker_snapshot(source, "list[str]").unwrap(),
        known_sequence
    );
    assert_eq!(
        binding_flow_marker_snapshot(source, "# reachable-return").unwrap(),
        known_sequence
    );
    assert!(
        binding_flow_marker_snapshot(source, "tuple[str]")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );

    let semicolon_dead = concat!(
        "def run():\n",
        "    from typing import Sequence\n",
        "    return 1; dead: tuple[str]\n",
    );
    assert!(
        binding_flow_marker_snapshot(semicolon_dead, "tuple[str]")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );

    let handler_source = concat!(
        "def run():\n",
        "    from typing import Sequence\n",
        "    try:\n",
        "        risky()\n",
        "    except Error as Sequence:\n",
        "        return 1\n",
        "        handler_unreachable: tuple[str]\n",
    );
    assert!(
        binding_flow_marker_snapshot(handler_source, "tuple[str]")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );

    let match_source = concat!(
        "def run(value):\n",
        "    from typing import Sequence\n",
        "    match value:\n",
        "        case _:\n",
        "            return 1\n",
        "            case_unreachable: tuple[str]\n",
    );
    assert!(
        binding_flow_marker_snapshot(match_source, "tuple[str]")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );
}

#[test]
fn exception_match_binding_marker_projection_bounds_match_case_headers() {
    let reachable_source = concat!(
        "def run(value):\n",
        "    from typing import Sequence\n",
        "    match value:\n",
        "        case [Sequence] if reachable_guard:  # reachable-case-header\n",
        "            return 1\n",
        "            dead_body: tuple[str]\n",
        "        case _:\n",
        "            pass\n",
    );
    let known_sequence = vec!["direct:Sequence=typing.Sequence".to_owned()];

    assert_eq!(
        binding_flow_marker_snapshot(reachable_source, "[Sequence]").unwrap(),
        known_sequence
    );
    assert_eq!(
        binding_flow_marker_snapshot(reachable_source, "reachable_guard").unwrap(),
        known_sequence
    );
    assert_eq!(
        binding_flow_marker_snapshot(reachable_source, "# reachable-case-header").unwrap(),
        known_sequence
    );
    assert!(
        binding_flow_marker_snapshot(reachable_source, "tuple[str]")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );

    let unreachable_case_source = concat!(
        "def run(value):\n",
        "    from typing import Sequence\n",
        "    match value:\n",
        "        case _:\n",
        "            pass\n",
        "        case 0 if unreachable_guard:\n",
        "            pass\n",
    );
    assert!(
        binding_flow_marker_snapshot(unreachable_case_source, "unreachable_guard")
            .unwrap_err()
            .starts_with("infrastructure-error:")
    );
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

#[rstest::rstest]
#[case::exception_1(0)]
#[case::removed_with_expression_exception_2(1)]
#[case::valueerror_typeerror_3(2)]
#[case::valueerror_retained_typeerror_4(3)]
#[case::internal_valueerror_external_typeerror_5(4)]
fn parenthesized_exception_removals_use_complete_expression_spans(#[case] scenario_index: usize) {
    let cases = [
        (
            "((Exception))",
            MutationOperator::ExceptionExceptionToBare,
            vec![("((Exception))", "")],
        ),
        (
            "(\n    # removed with expression\n    (Exception)\n)",
            MutationOperator::ExceptionExceptionToBare,
            vec![("(\n    # removed with expression\n    (Exception)\n)", "")],
        ),
        (
            "((ValueError), (TypeError))",
            MutationOperator::ExceptionTupleRemoveMember,
            vec![
                ("((ValueError), (TypeError))", "( (TypeError))"),
                ("((ValueError), (TypeError))", "((ValueError) )"),
            ],
        ),
        (
            "((ValueError), # retained\n    ((TypeError)),)",
            MutationOperator::ExceptionTupleRemoveMember,
            vec![
                (
                    "((ValueError), # retained\n    ((TypeError)),)",
                    "( # retained\n    ((TypeError)),)",
                ),
                (
                    "((ValueError), # retained\n    ((TypeError)),)",
                    "((ValueError), # retained\n    )",
                ),
            ],
        ),
        (
            "(( # internal\n    ValueError), # external\n    TypeError)",
            MutationOperator::ExceptionTupleRemoveMember,
            vec![
                (
                    "(( # internal\n    ValueError), # external\n    TypeError)",
                    "( # external\n    TypeError)",
                ),
                (
                    "(( # internal\n    ValueError), # external\n    TypeError)",
                    "(( # internal\n    ValueError) # external\n    )",
                ),
            ],
        ),
    ];

    let (expression, operator, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let source = format!("try:\n    work()\nexcept {expression}:\n    pass\n");
    let mut operators = MutationOperatorSelection::default();
    operators.include(operator);

    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("case.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 100,
        },
        &source,
    );
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == operator.as_str())
        .collect();
    // pins: issue #451 — AST name ranges omit syntactically significant parentheses.
    assert_eq!(
        candidates
            .iter()
            .map(|c| (c.original.as_str(), c.replacement.as_str()))
            .collect::<Vec<_>>(),
        expected,
        "{source}"
    );
    for candidate in candidates {
        apply_candidate_and_reparse(&source, candidate);
    }
}

#[rstest::rstest]
#[case::value("\r")]
#[case::value_2("\r\n")]
#[case::value_3("\n")]
fn python_physical_lines_select_candidates_after_all_newline_forms_table_1(
    #[case] newline: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    let final_newline = ["", newline].into_iter().nth(scenario_index).unwrap();

    let source = format!("def f():{newline}    return 1 + 2{final_newline}");

    let output = analyze_with(
        Utf8Path::new("case.py"),
        &[LineRange { start: 2, end: 2 }],
        &[],
        100,
        &source,
    );
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "binary_add_sub")
        .collect();
    // pins: issue #455 — line selection previously silently omitted lone-CR candidates.
    assert_eq!(candidates.len(), 1, "{source:?}");
    let candidate = candidates[0];
    assert_eq!((candidate.line, candidate.column), (2, 13));
    assert_eq!(
        usize::try_from(candidate.span.start).unwrap(),
        source.find('+').unwrap()
    );
    apply_candidate_and_reparse(&source, candidate);
}

#[test]
fn python_physical_lines_select_candidates_after_all_newline_forms() {
    let source = "\u{feff}# header\r\n\r# next\n値 = 1 + 2\r";

    let output = analyze_with(
        Utf8Path::new("case.py"),
        &[LineRange { start: 4, end: 4 }],
        &[],
        100,
        source,
    );
    let candidate = output
        .candidates
        .iter()
        .find(|c| c.original == "+")
        .expect("mixed-newline candidate");

    assert_eq!((candidate.line, candidate.column), (4, 6));
    assert_eq!(
        usize::try_from(candidate.span.start).unwrap(),
        source.find('+').unwrap()
    );
}

#[rstest::rstest]
#[case::list_0_for_in_list_values(
    "[(list := 0) for _ in []]\nlist(values)\n",
    "list(values)",
    "unknown"
)]
#[case::list_0_for_in_0_list_values(
    "((list := 0) for _ in [0])\nlist(values)\n",
    "list(values)",
    "unknown"
)]
#[case::def_f_list_values_list_0_for_in(
    "def f():\n    list(values)\n    [(list := 0) for _ in []]\n",
    "list(values)",
    "shadowed"
)]
#[case::list_values_list_0_for_in_0(
    "list(values)\n[(list := 0) for _ in [0]]\n",
    "list(values)",
    "definitely-builtin"
)]
#[case::list_list_values("(list := list(values))\n", "list(values)", "definitely-builtin")]
#[case::list_values_for_list_in_custom(
    "[list(values) for list in [custom]]\n",
    "list(values)",
    "shadowed"
)]
#[case::unused_list_values_for_list_in_custom(
    "[(unused := list(values)) for list in [custom]]\n",
    "list(values)",
    "shadowed"
)]
#[case::for_in_0_1_list_values_list_custom_for_in_0(
    "for _ in [0, 1]:\n    list(values)\n    [(list := custom) for _ in [0]]\n",
    "list(values)",
    "unknown"
)]
fn comprehension_named_binding_execution_facts_and_rhs_order(
    #[case] source: &str,
    #[case] marker: &str,
    #[case] expected: &str,
) {
    let snapshot = name_resolution_test_snapshot(source, marker, "list").unwrap();

    assert_eq!(snapshot.resolution, expected, "{source}");
}

#[test]
fn wide_mapping_keys_reject_conjugates_and_preserve_other_candidates() {
    use std::fmt::Write;
    let mut source = String::from("def classify(value):\n    match value:\n        case {");
    for index in 1..=1000 {
        write!(source, "{index}+2j: _, {index}-2j: _, ").unwrap();
    }
    source.push_str("False: _}: return True\n");

    let output = analyze(&source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.operator.as_str(),
                "boolean_literal" | "binary_add_sub"
            )
        })
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();

    assert_eq!(candidates, [("False", "True"), ("True", "False")]);
    // The mapping pass normalizes each key a bounded number of times and uses
    // HashSet membership, regardless of whether the collision is far away.
    assert!(!output.truncated);
}

#[rstest::rstest]
#[case::items_1_1(0)]
#[case::items_1_2(1)]
#[case::items_1_3(2)]
#[case::items_1_4(3)]
#[case::items_2_5(4)]
#[case::items_2_6(5)]
#[case::items_2_7(6)]
#[case::items_2_8(7)]
#[case::items_0_9(8)]
#[case::items_0_10(9)]
#[case::items_1_11(10)]
#[case::items_1_12(11)]
#[case::items_1_inner_comment_13(12)]
#[case::items_18446744073709551615_14(13)]
#[case::items_18446744073709551616_15(14)]
#[case::items_18446744073709551616_16(15)]
#[case::items_99999999999999999999999999999999999999999999_17(16)]
#[case::items_0x10_18(17)]
#[case::items_1_000_19(18)]
#[case::items_1_0_20(19)]
#[case::items_true_21(20)]
#[case::items_1_22(21)]
#[case::items_1_23(22)]
#[case::items_1_2_24(23)]
fn structure_negative_neighbors_cover_positions_spelling_and_range_limits(
    #[case] scenario_index: usize,
) {
    let cases: &[(&str, &[&str])] = &[
        ("items[-1]", &["0", "-2"]),
        ("items[-1:]", &["0", "-2"]),
        ("items[:-1]", &["0", "-2"]),
        ("items[::-1]", &["-2"]),
        ("items[-2]", &["-1", "-3"]),
        ("items[-2:]", &["-1", "-3"]),
        ("items[:-2]", &["-1", "-3"]),
        ("items[::-2]", &["-1", "-3"]),
        ("items[-0]", &["1", "-1"]),
        ("items[::-0]", &["1", "-1"]),
        ("items[(-1)]", &["0", "-2"]),
        ("items[-(1)]", &["0", "-2"]),
        ("items[( -\n  (1) # inner comment\n)]", &["0", "-2"]),
        ("items[-18446744073709551615]", &["-18446744073709551614"]),
        ("items[-18446744073709551616]", &[]),
        ("items[18446744073709551616]", &[]),
        ("items[-99999999999999999999999999999999999999999999]", &[]),
        ("items[-0x10]", &[]),
        ("items[-1_000]", &[]),
        ("items[-1.0]", &[]),
        ("items[-True]", &[]),
        ("items[--1]", &[]),
        ("items[+1]", &[]),
        ("items[-(1+2)]", &[]),
    ];

    let (expression, expected) = cases.get(scenario_index).unwrap();

    let source = format!("value = {expression}\n");

    let output = analyze(&source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| {
            matches!(
                c.operator.as_str(),
                "structure_index_neighbor" | "structure_slice_neighbor"
            )
        })
        .collect();

    assert_eq!(
        candidates
            .iter()
            .map(|c| c.replacement.as_str())
            .collect::<Vec<_>>(),
        *expected,
        "{source}"
    );
    for candidate in candidates {
        assert!(candidate.original.starts_with('-'), "{candidate:?}");
        apply_candidate_and_reparse(&source, candidate);
    }
}

#[test]
fn structure_negative_neighbors_remain_excluded_from_annotations_and_targets() {
    let source = concat!(
        "annotation: items[-1]\n",
        "annotation_slice: items[-1:-2:-1]\n",
        "items[-1] = value\n",
        "items[-1:-2:-1] = value\n",
        "del items[-1]\n",
        "del items[-1:-2:-1]\n",
    );

    let output = analyze(source);

    assert!(output.candidates.iter().all(|c| !matches!(
        c.operator.as_str(),
        "structure_index_neighbor" | "structure_slice_neighbor"
    )));
}

#[rstest::rstest]
#[case::nested_finally_1(0)]
#[case::else_exception_2(1)]
#[case::handler_exception_3(2)]
#[case::loop_exception_4(3)]
#[case::normal_successor_5(4)]
#[case::deferred_function_6(5)]
#[case::deferred_lambda_7(6)]
#[case::deferred_generator_8(7)]
#[case::lambda_default_9(8)]
#[case::generator_iterable_10(9)]
#[case::finally_restores_import_11(10)]
#[case::unreachable_hazard_12(11)]
#[case::walrus_before_failure_13(12)]
#[case::call_assignment_14(13)]
fn implicit_finally_preserves_nested_and_deferred_boundaries(#[case] scenario_index: usize) {
    let cases = [
        (
            "nested finally",
            "Sequence = set\ntry:\n    try:\n        hazard()\n        from typing import Sequence\n    finally:\n        pass\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
        (
            "else exception",
            "Sequence = set\ntry:\n    pass\nexcept KeyError:\n    pass\nelse:\n    hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
        (
            "handler exception",
            "Sequence = set\ntry:\n    raise KeyError\nexcept KeyError:\n    hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
        (
            "loop exception",
            "Sequence = set\ntry:\n    while condition:\n        hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
        (
            "normal successor",
            "Sequence = set\ntry:\n    hazard()\n    from typing import Sequence\nfinally:\n    pass\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "deferred function",
            "Sequence = set\ntry:\n    def later():\n        hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            1,
        ),
        (
            "deferred lambda",
            "Sequence = set\ntry:\n    later = lambda: hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            1,
        ),
        (
            "deferred generator",
            "Sequence = set\ntry:\n    later = (hazard() for x in ())\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            1,
        ),
        (
            "lambda default",
            "Sequence = set\ntry:\n    later = lambda x=hazard(): x\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
        (
            "generator iterable",
            "Sequence = set\ntry:\n    later = (x for x in hazard())\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
        (
            "finally restores import",
            "Sequence = set\ntry:\n    try:\n        hazard()\n    finally:\n        from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            1,
        ),
        (
            "unreachable hazard",
            "def f():\n    from typing import Sequence\n    try:\n        return\n        hazard()\n        Sequence = set\n    finally:\n        value: Sequence[int]\n",
            1,
        ),
        (
            "walrus before failure",
            "from typing import Sequence\ntry:\n    hazard((Sequence := set))\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
        (
            "call assignment",
            "Sequence = set\ntry:\n    x = hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
            0,
        ),
    ];

    let (label, source, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let actual = analyze_with_only_operator(source, MutationOperator::TypeListSequence);

    assert_eq!(actual.candidates.len(), expected, "{label}\n{source}");
}

#[rstest::rstest]
#[case::class_global_1(0)]
#[case::class_nonlocal_2(1)]
fn implicit_finally_covers_operator_and_statement_exception_entries_scenario_1(
    #[case] scenario_index: usize,
) {
    let bodies = [
        ("division", "1 / 0"),
        ("comparison", "left < right"),
        ("unary operator", "-operand"),
        ("assertion", "assert condition"),
        ("iteration", "for item in items:\n        pass"),
        ("context manager", "with manager:\n        pass"),
        ("class body", "class C:\n        hazard()"),
        ("truth test", "if condition:\n        pass"),
        (
            "handler header",
            "try:\n        pass\n    except hazard():\n        pass",
        ),
    ];
    for (label, body) in bodies {
        let source = format!(
            "Sequence = set\ntry:\n    {body}\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n"
        );

        let output = analyze_with_only_operator(&source, MutationOperator::TypeListSequence);

        assert!(output.candidates.is_empty(), "{label}");
    }

    let (label, source) = [
        (
            "class global",
            "from typing import Sequence\ntry:\n    class C:\n        global Sequence\n        Sequence = set\n        hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
        ),
        (
            "class nonlocal",
            "def f():\n    from typing import Sequence\n    try:\n        class C:\n            nonlocal Sequence\n            Sequence = set\n            hazard()\n        from typing import Sequence\n    finally:\n        value: Sequence[int]\n",
        ),
    ].into_iter().nth(scenario_index).unwrap();

    let output = analyze_with_only_operator(source, MutationOperator::TypeListSequence);

    assert!(output.candidates.is_empty(), "{label}");
}

#[rstest::rstest]
#[case::division_1(0)]
#[case::comparison_2(1)]
#[case::unary_operator_3(2)]
#[case::assertion_4(3)]
#[case::iteration_5(4)]
#[case::context_manager_6(5)]
#[case::class_body_7(6)]
#[case::truth_test_8(7)]
#[case::handler_header_9(8)]
fn implicit_finally_covers_operator_and_statement_exception_entries(#[case] scenario_index: usize) {
    let bodies = [
        ("division", "1 / 0"),
        ("comparison", "left < right"),
        ("unary operator", "-operand"),
        ("assertion", "assert condition"),
        ("iteration", "for item in items:\n        pass"),
        ("context manager", "with manager:\n        pass"),
        ("class body", "class C:\n        hazard()"),
        ("truth test", "if condition:\n        pass"),
        (
            "handler header",
            "try:\n        pass\n    except hazard():\n        pass",
        ),
    ];

    let (label, body) = bodies.into_iter().nth(scenario_index).unwrap();

    let source = format!(
        "Sequence = set\ntry:\n    {body}\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n"
    );

    let output = analyze_with_only_operator(&source, MutationOperator::TypeListSequence);

    assert!(output.candidates.is_empty(), "{label}");
}

#[rstest::rstest]
#[case::partial_unpack_binding_1(0)]
#[case::multiple_with_targets_2(1)]
#[case::partial_multiple_assignment_3(2)]
fn implicit_finally_covers_name_container_format_and_partial_binding_failures_scenario_1(
    #[case] scenario_index: usize,
) {
    let bodies = [
        ("name load", "missing"),
        ("set hash", "{[]}"),
        ("dict hash", "{[]: 1}"),
        ("format protocol", "f'{value}'"),
        ("starred expansion", "[*values]"),
        ("unpack", "first, second = values"),
    ];
    for (label, body) in bodies {
        let source = format!(
            "Sequence = set\ntry:\n    {body}\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n"
        );

        let output = analyze_with_only_operator(&source, MutationOperator::TypeListSequence);

        assert!(output.candidates.is_empty(), "{label}");
    }

    let (label, source) = [
        (
            "partial unpack binding",
            "from typing import Sequence\ntry:\n    Sequence, (first, second) = values\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
        ),
        (
            "multiple with targets",
            "from typing import Sequence\ntry:\n    with manager as Sequence, other:\n        from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
        ),
        (
            "partial multiple assignment",
            "from typing import Sequence\ntry:\n    Sequence = holder.value = supplied\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
        ),
    ].into_iter().nth(scenario_index).unwrap();

    let output = analyze_with_only_operator(source, MutationOperator::TypeListSequence);

    assert!(output.candidates.is_empty(), "{label}");
}

#[rstest::rstest]
#[case::name_load_1(0)]
#[case::set_hash_2(1)]
#[case::dict_hash_3(2)]
#[case::format_protocol_4(3)]
#[case::starred_expansion_5(4)]
#[case::unpack_6(5)]
fn implicit_finally_covers_name_container_format_and_partial_binding_failures(
    #[case] scenario_index: usize,
) {
    let bodies = [
        ("name load", "missing"),
        ("set hash", "{[]}"),
        ("dict hash", "{[]: 1}"),
        ("format protocol", "f'{value}'"),
        ("starred expansion", "[*values]"),
        ("unpack", "first, second = values"),
    ];

    let (label, body) = bodies.into_iter().nth(scenario_index).unwrap();

    let source = format!(
        "Sequence = set\ntry:\n    {body}\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n"
    );

    let output = analyze_with_only_operator(&source, MutationOperator::TypeListSequence);

    assert!(output.candidates.is_empty(), "{label}");
}

#[test]
fn implicit_finally_covers_pattern_capture_before_guard_failure() {
    let source = "from typing import Sequence\ntry:\n    match subject:\n        case Sequence if hazard():\n            pass\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n";

    assert_eq!(
        analyze_with_only_operator(source, MutationOperator::TypeListSequence).candidates,
        Vec::new()
    );
}

#[rstest::rstest]
#[case::call_before_import_1(0)]
#[case::import_before_call_2(1)]
#[case::normal_import_3(2)]
#[case::explicit_raise_4(3)]
#[case::bare_raise_5(4)]
#[case::second_manager_entry_6(5)]
#[case::second_manager_preserves_established_import_7(6)]
#[case::partial_target_binding_8(7)]
#[case::infallible_target_binding_9(8)]
#[case::async_manager_10(9)]
#[case::async_positive_11(10)]
fn with_suppression_joins_only_reachable_exception_states(#[case] scenario_index: usize) {
    let cases = [
        (
            "call before import",
            "Sequence = set\nwith manager:\n    hazard()\n    from typing import Sequence\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "import before call",
            "Sequence = set\nwith manager:\n    from typing import Sequence\n    hazard()\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "normal import",
            "Sequence = set\nwith manager:\n    from typing import Sequence\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "explicit raise",
            "Sequence = set\nwith manager:\n    raise KeyError\n    from typing import Sequence\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "bare raise",
            "from typing import Sequence\nwith manager:\n    raise\n    Sequence = set\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "second manager entry",
            "Sequence = set\nwith first, second:\n    from typing import Sequence\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "second manager preserves established import",
            "from typing import Sequence\nwith first, second:\n    pass\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "partial target binding",
            "from typing import Sequence\nwith manager as (Sequence, (x, y)):\n    from typing import Sequence\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "infallible target binding",
            "Sequence = set\nwith manager as target:\n    from typing import Sequence\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "async manager",
            "async def f():\n    Sequence = set\n    async with manager:\n        await hazard()\n        from typing import Sequence\n    value: Sequence[int]\n",
            0,
        ),
        (
            "async positive",
            "async def f():\n    Sequence = set\n    async with manager:\n        from typing import Sequence\n        await hazard()\n    value: Sequence[int]\n",
            1,
        ),
    ];

    let (label, source, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let actual = analyze_with_only_operator(source, MutationOperator::TypeListSequence);

    assert_eq!(actual.candidates.len(), expected, "{label}\n{source}");
}

#[rstest::rstest]
#[case::finally_restores_import_1(0)]
#[case::finally_preserves_exception_2(1)]
#[case::handler_raises_3(2)]
#[case::nested_with_4(3)]
#[case::bare_raise_through_finally_5(4)]
#[case::bare_raise_through_loop_6(5)]
#[case::handler_target_cleanup_7(6)]
#[case::inner_exit_raises_during_return_8(7)]
#[case::deferred_lambda_9(8)]
#[case::deferred_function_10(9)]
#[case::return_expression_raises_11(10)]
fn with_suppression_preserves_nested_flow_and_deferred_boundaries(#[case] scenario_index: usize) {
    let cases = [
        (
            "finally restores import",
            "Sequence = set\nwith manager:\n    try:\n        hazard()\n    finally:\n        from typing import Sequence\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "finally preserves exception",
            "Sequence = set\nwith manager:\n    try:\n        hazard()\n        from typing import Sequence\n    finally:\n        pass\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "handler raises",
            "Sequence = set\nwith manager:\n    try:\n        pass\n    except KeyError:\n        raise\n    from typing import Sequence\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "nested with",
            "Sequence = set\nwith outer:\n    with inner:\n        from typing import Sequence\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "bare raise through finally",
            "from typing import Sequence\nwith manager:\n    try:\n        raise\n    finally:\n        pass\n    Sequence = set\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "bare raise through loop",
            "from typing import Sequence\nwith manager:\n    while condition:\n        raise\n    Sequence = set\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "handler target cleanup",
            "from typing import Sequence\nwith manager:\n    try:\n        pass\n    except KeyError as Sequence:\n        raise\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "inner exit raises during return",
            "from typing import Sequence\nwith first, second:\n    Sequence = 0\n    return\nvalue: Sequence[int]\n",
            0,
        ),
        (
            "deferred lambda",
            "Sequence = set\nwith manager:\n    later = lambda: hazard()\n    from typing import Sequence\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "deferred function",
            "Sequence = set\nwith manager:\n    def later():\n        hazard()\n    from typing import Sequence\nvalue: Sequence[int]\n",
            1,
        ),
        (
            "return expression raises",
            "def f():\n    Sequence = set\n    with manager:\n        return hazard()\n        from typing import Sequence\n    value: Sequence[int]\n",
            0,
        ),
    ];

    let (label, source, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let actual = analyze_with_only_operator(source, MutationOperator::TypeListSequence);

    assert_eq!(actual.candidates.len(), expected, "{label}\n{source}");
}

#[rstest::rstest]
#[case::input_return_1(0)]
#[case::input_break_2(1)]
#[case::input_continue_3(2)]
fn with_suppression_preserves_successful_abrupt_categories(#[case] scenario_index: usize) {
    type ExitStates = fn(BindingFlowTestSnapshot) -> Vec<Vec<String>>;
    let cases: [(&str, ExitStates); 3] = [
        ("return", |snapshot| snapshot.terminates),
        ("break", |snapshot| snapshot.breaks),
        ("continue", |snapshot| snapshot.continues),
    ];

    let (abrupt, exit_states) = cases.into_iter().nth(scenario_index).unwrap();

    let source = format!("from typing import Sequence\nwith manager:\n    {abrupt}\n");

    let actual = binding_flow_test_snapshot(&source);

    assert!(actual.fallthrough.is_empty(), "{abrupt}: {actual:?}");
    let states = exit_states(actual);
    assert_eq!(
        states,
        vec![vec!["direct:Sequence=typing.Sequence".to_owned()]]
    );
    let overridden = format!(
        "from typing import Sequence\nwith manager:\n    try:\n        raise\n    finally:\n        {abrupt}\n"
    );

    let actual = binding_flow_test_snapshot(&overridden);

    assert!(
        actual.fallthrough.is_empty(),
        "finally {abrupt}: {actual:?}"
    );
}

#[rstest::rstest]
#[case::raise("    raise\n")]
#[case::try_raise_finally_pass("    try:\n        raise\n    finally:\n        pass\n")]
fn with_suppression_makes_bare_raise_successor_reachable(#[case] body: &str) {
    let source = format!("from typing import Sequence\nwith manager:\n{body}");

    let actual = binding_flow_test_snapshot(&source);

    assert_eq!(
        actual.fallthrough,
        vec![vec!["direct:Sequence=typing.Sequence".to_owned()]],
        "{source}"
    );
    assert_eq!(actual.terminates.len(), 1, "possible propagation: {source}");
}

#[rstest::rstest]
#[case::tuple_target_1(0)]
#[case::chained_target_2(1)]
#[case::rhs_walrus_3(2)]
#[case::destination_target_4(3)]
#[case::destination_rhs_5(4)]
#[case::starred_source_6(5)]
#[case::starred_destination_7(6)]
#[case::nested_target_8(7)]
#[case::starred_target_9(8)]
#[case::list_target_10(9)]
#[case::augassign_rhs_follows_target_11(10)]
#[case::augassign_destination_in_target_12(11)]
#[case::rhs_positive_13(12)]
#[case::chained_rhs_positive_14(13)]
#[case::ordinary_assignment_target_follows_rhs_15(14)]
#[case::augassign_target_precedes_rhs_16(15)]
#[case::callable_precedes_arguments_17(16)]
#[case::keyword_follows_positional_positive_18(17)]
#[case::try_partial_stores_19(18)]
#[case::try_failure_before_store_keeps_rhs_20(19)]
fn evaluation_order_preserves_only_pre_binding_builtin_lookups(#[case] scenario_index: usize) {
    let cases = [
        ("tuple target", "any, slots[any([])] = custom, 7", 0),
        ("chained target", "any = slots[any([])] = custom", 0),
        ("RHS walrus", "slots[any([])] = (any := custom)", 0),
        ("destination target", "all, slots[any([])] = custom, 7", 0),
        ("destination RHS", "slots[any([])] = (all := custom)", 0),
        (
            "starred source",
            "sink(flag=any([]), *[(any := custom)])",
            0,
        ),
        (
            "starred destination",
            "sink(flag=any([]), *[(all := custom)])",
            0,
        ),
        (
            "nested target",
            "(any, (slots[any([])], other)) = custom, (7, 8)",
            0,
        ),
        (
            "starred target",
            "any, *rest, slots[any([])] = custom, 1, 7",
            0,
        ),
        ("list target", "[any, slots[any([])]] = custom, 7", 0),
        (
            "augassign RHS follows target",
            "slots[(any := custom)] += any([])",
            0,
        ),
        (
            "augassign destination in target",
            "slots[(all := custom)] += any([])",
            0,
        ),
        ("RHS positive", "any, slots[0] = any([]), 7", 1),
        ("chained RHS positive", "any = slots[0] = any([])", 1),
        (
            "ordinary assignment target follows RHS",
            "slots[any([])] = (all := custom)",
            0,
        ),
        (
            "augassign target precedes RHS",
            "slots[any([])] += (all := custom)",
            1,
        ),
        ("callable precedes arguments", "any([(all := custom)])", 1),
        (
            "keyword follows positional positive",
            "sink(any([]), flag=(all := custom))",
            1,
        ),
        (
            "try partial stores",
            "try:\n    any, slots[any([])] = custom, 7\nexcept Exception:\n    pass\nobserved = any([])",
            0,
        ),
        (
            "try failure before store keeps RHS",
            "try:\n    any, slots[0] = any([]), 7\nexcept Exception:\n    pass",
            1,
        ),
    ];

    let (label, body, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let source = format!(
        "def custom(values): return 'custom'\ndef sink(*args, **kwargs): return kwargs\nslots = {{False: 0}}\n{body}\n"
    );

    let output = analyze(&source);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_any_all")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), expected, "{label}: {candidates:#?}");
}

#[rstest::rstest]
#[case::collection_list_tuple("collection_list_tuple", "list", "tuple")]
#[case::structure_sorted_reversed("structure_sorted_reversed", "sorted", "reversed")]
fn evaluation_order_applies_to_other_builtin_pairs_and_class_suites_scenario_1(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] destination: &str,
    #[values(0, 1)] scenario_index: usize,
    #[values(0, 1, 2)] nested_scenario_index: usize,
    #[values(0, 1)] inner_scenario_index: usize,
) {
    let positive = analyze(&format!("{destination}, other = {source}([]), 7\n"));

    assert_eq!(
        positive
            .candidates
            .iter()
            .filter(
                |candidate| candidate.operator.as_str() == operator && candidate.original == source
            )
            .count(),
        1
    );

    let binding = [source, destination]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let body = [
        format!("{binding}, slots[{source}([])] = custom, 7"),
        format!("slots[{source}([])] = ({binding} := custom)"),
        format!("sink(flag={source}([]), *[({binding} := custom)])"),
    ]
    .into_iter()
    .nth(nested_scenario_index)
    .unwrap();

    let source = [format!("{body}\n"), format!("class Example:\n    {body}\n")]
        .into_iter()
        .nth(inner_scenario_index)
        .unwrap();

    let output = analyze(&source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != operator
                || !matches!(candidate.original.as_str(), "list" | "sorted")),
        "{source}: {:#?}",
        output.candidates
    );
}

#[rstest::rstest]
#[case::collection_list_tuple("collection_list_tuple", "list", "tuple")]
#[case::structure_sorted_reversed("structure_sorted_reversed", "sorted", "reversed")]
fn evaluation_order_applies_to_other_builtin_pairs_and_class_suites(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] destination: &str,
) {
    let positive = analyze(&format!("{destination}, other = {source}([]), 7\n"));

    assert_eq!(
        positive
            .candidates
            .iter()
            .filter(
                |candidate| candidate.operator.as_str() == operator && candidate.original == source
            )
            .count(),
        1
    );
}

#[rstest::rstest]
#[case::del_slots_any_all("del slots[any([])], all")]
#[case::del_slots_any_all_2("del (slots[any([])], all)")]
#[case::def_any_value_any_pass("def any(value=any([])):\n    pass")]
#[case::class_any_value_any("class any:\n    value = any([])")]
fn evaluation_order_retains_lookups_before_later_deletions(#[case] body: &str) {
    let output = analyze(body);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "collection_any_all")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), 1, "{body}: {candidates:#?}");
}

#[rstest::rstest]
#[case::str("str", "str")]
#[case::int("int", "int")]
#[case::float("float", "float")]
#[case::bool("bool", "bool")]
#[case::bytes("bytes", "bytes")]
#[case::list("list", "list[int]")]
#[case::set("set", "set[int]")]
#[case::dict("dict", "dict[str, int]")]
fn nullable_builtin_provenance_rejects_shadowing_and_retains_builtins(
    #[case] name: &str,
    #[case] annotation: &str,
    #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12)] scenario_index: usize,
) {
    let (source, expected) = [
        (format!("value: {annotation}\n"), 1),
        (format!("{name} = custom\nvalue: {annotation}\n"), 0),
        (format!("value: {annotation}\n{name} = custom\n"), 0),
        (format!("class {name}: pass\nvalue: {annotation}\n"), 0),
        (
            format!("if condition:\n    {name} = custom\nvalue: {annotation}\n"),
            0,
        ),
        (
            format!("class Owner:\n    value: {annotation}\n    {name} = custom\n"),
            0,
        ),
        (
            format!(
                "def outer():\n    {name} = custom\n    def inner(value: {annotation}): pass\n"
            ),
            0,
        ),
        (
            format!("def function(value: {annotation}):\n    {name} = custom\n"),
            1,
        ),
        (
            format!("def function():\n    {name} = custom\n    value: {annotation}\n"),
            0,
        ),
        (
            format!("def function[{name}](value: {annotation}): pass\n"),
            0,
        ),
        (
            format!("class Owner[{name}]:\n    value: {annotation}\n"),
            0,
        ),
        (format!("from custom import *\nvalue: {annotation}\n"), 0),
        (format!("exec('pass')\nvalue: {annotation}\n"), 0),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let output = analyze_types(&source);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_nullable_add")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), expected, "{source}: {candidates:#?}");
}

#[rstest::rstest]
#[case::int_custom_value_list_int("int = custom\nvalue: list[int]\n", 0)]
#[case::str_custom_value_dict_str_int("str = custom\nvalue: dict[str, int]\n", 0)]
#[case::float_custom_value_list_dict_str_float(
    "float = custom\nvalue: list[dict[str, float]]\n",
    0
)]
#[case::list_custom_value_dict_str_list_int("list = custom\nvalue: dict[str, list[int]]\n", 0)]
#[case::from_typing_import_sequence_int_custom_value_sequence_int(
    "from typing import Sequence\nint = custom\nvalue: Sequence[int]\n",
    0
)]
#[case::from_typing_import_sequence_as_list_value_list_int(
    "from typing import Sequence as list\nvalue: list[int]\n",
    1
)]
#[case::from_typing_import_mapping_as_dict_value_dict_str_int(
    "from typing import Mapping as dict\nvalue: dict[str, int]\n",
    1
)]
#[case::class_owner_int_custom_def_method_value_int_pass(
    "class Owner:\n    int = custom\n    def method(value: int): pass\n",
    0
)]
#[case::class_owner_int_custom_def_method_value_int(
    "class Owner:\n    int = custom\n    def method():\n        value: int\n",
    1
)]
#[case::def_function_int_value_int("def function(int):\n    value: int\n", 0)]
#[case::def_function_int_int_int_pass("def function(int: int) -> int: pass\n", 2)]
#[case::def_function_global_int_value_int("def function():\n    global int\n    value: int\n", 1)]
#[case::int_custom_def_function_global_int_value_int(
    "int = custom\ndef function():\n    global int\n    value: int\n",
    0
)]
#[case::def_outer_int_custom_def_inner_nonlocal_int_value_int(
    "def outer():\n    int = custom\n    def inner():\n        nonlocal int\n        value: int\n",
    0
)]
#[case::class_outer_int_custom_class_inner_value_int(
    "class Outer:\n    int = custom\n    class Inner:\n        value: int\n",
    1
)]
#[case::class_owner_int_def_method_value_int_pass(
    "class Owner[int]:\n    def method(value: int): pass\n",
    0
)]
#[case::class_owner_int_custom_def_method_t_value_int_pass(
    "class Owner:\n    int = custom\n    def method[T](value: int): pass\n",
    0
)]
#[case::from_typing_import_sequence_as_list_int_custom_value_list_int(
    "from typing import Sequence as list\nint = custom\nvalue: list[int]\n",
    0
)]
fn nullable_builtin_provenance_checks_nested_arguments_and_preserves_import_aliases(
    #[case] source: &str,
    #[case] expected: usize,
) {
    let output = analyze_types(source);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_nullable_add")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), expected, "{source}: {candidates:#?}");
}

#[test]
fn nullable_builtin_provenance_does_not_change_removal_or_collection_guards() {
    let output = analyze_types(
        "from typing import Optional, Sequence\nint = custom\noptional: Optional[int]\nunion: int | None\ncollection: list[int]\n",
    );

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "type_nullable_remove")
            .count(),
        2
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator.as_str() == "type_list_sequence")
            .count(),
        1
    );
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "type_nullable_add")
    );
}

#[rstest::rstest]
#[case::dict_str_int("dict[*(str, int)]")]
#[case::dict_str_int_2("dict[*[str, int]]")]
#[case::list_int("list[[int]]")]
fn nullable_builtin_provenance_checks_starred_and_list_arguments(
    #[case] annotation: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    let (prefix, expected) = [("", 1), ("int = custom\n", 0)]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let source = format!("{prefix}value: {annotation}\n");

    let output = analyze_types(&source);
    let candidates = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "type_nullable_add")
        .collect::<Vec<_>>();

    assert_eq!(candidates.len(), expected, "{source}: {candidates:#?}");
}

#[rstest::rstest]
#[case::any("Any")]
#[case::a("A")]
#[case::t_any("t.Any")]
#[case::object("object")]
#[case::t("T")]
#[case::foo("'Foo'")]
#[case::ann_int_tag("Ann[int, 'tag']")]
#[case::call_int_str("Call[[int], str]")]
#[case::lit_1("Lit[1]")]
#[case::proto("Proto")]
fn annotation_descendants_reject_blocked_leaves_at_every_argument_position_scenario_1(
    #[case] leaf: &str,
    #[values(0, 1, 2, 3, 4, 5, 6)] scenario_index: usize,
) {
    let prefix = "from typing import Any, Any as A, Annotated as Ann, Callable as Call, Literal as Lit, Protocol as Proto, TypeVar\nimport typing as t\nT = TypeVar('T')\n";

    let annotation = [
        format!("dict[{leaf}, int]"),
        format!("dict[str, {leaf}]"),
        format!("list[dict[str, list[dict[int, {leaf}]]]]"),
        format!("list[tuple[int, str, {leaf}]]"),
        format!("dict[*(str, {leaf})]"),
        format!("dict[*[str, {leaf}]]"),
        format!("list[[{leaf}]]"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let source = format!("{prefix}value: {annotation}\n");

    let actual = analyze_with_only_operator(&source, MutationOperator::TypeNullableAdd);

    assert!(
        actual.candidates.is_empty(),
        "{source}: {:?}",
        actual.candidates
    );
}

#[rstest::rstest]
#[case::dict_str_int("dict[str, int]")]
#[case::dict_int_str("dict[int, str]")]
#[case::list_dict_str_list_dict_int_bytes("list[dict[str, list[dict[int, bytes]]]]")]
#[case::list_tuple_int_str_bytes("list[tuple[int, str, bytes]]")]
#[case::t_mapping_str_int("t.Mapping[str, int]")]
#[case::dict_str_int_6("dict[*(str, int)]")]
#[case::dict_str_int_7("dict[*[str, int]]")]
#[case::list_int("list[[int]]")]
fn annotation_descendants_reject_blocked_leaves_at_every_argument_position_scenario_2(
    #[case] annotation: &str,
) {
    let prefix = "from typing import Any, Any as A, Annotated as Ann, Callable as Call, Literal as Lit, Protocol as Proto, TypeVar\nimport typing as t\nT = TypeVar('T')\n";

    let source = format!("{prefix}value: {annotation}\n");

    let actual = analyze_with_only_operator(&source, MutationOperator::TypeNullableAdd);

    assert_eq!(actual.candidates.len(), 1, "{source}");
    assert_eq!(
        actual.candidates[0].replacement,
        format!("{annotation} | None")
    );
}

#[rstest::rstest]
#[case::dict_str_leaf("dict[str, LEAF]", MutationOperator::TypeNullableAdd)]
#[case::optional_dict_str_leaf("Optional[dict[str, LEAF]]", MutationOperator::TypeNullableRemove)]
#[case::list_dict_str_leaf("list[dict[str, LEAF]]", MutationOperator::TypeListSequence)]
#[case::set_tuple_str_leaf("set[tuple[str, LEAF]]", MutationOperator::TypeSetAbstractSet)]
#[case::dict_str_leaf_5("dict[str, LEAF]", MutationOperator::TypeMapping)]
#[case::iterable_dict_str_leaf("Iterable[dict[str, LEAF]]", MutationOperator::TypeIterableIterator)]
#[case::sequence_dict_str_leaf("Sequence[dict[str, LEAF]]", MutationOperator::TypeSequenceIterable)]
fn annotation_descendants_gate_every_type_operator(
    #[case] annotation: &str,
    #[case] operator: MutationOperator,
    #[values(0, 1)] scenario_index: usize,
) {
    let prefix =
        "from typing import Any, Optional, Sequence, AbstractSet, Mapping, Iterable, Iterator\n";

    let (leaf, expected) = [("Any", 0), ("int", 1)]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let annotation = annotation.replace("LEAF", leaf);
    let source = format!("{prefix}value: {annotation}\n");

    let actual = analyze_with_only_operator(&source, operator);

    assert_eq!(actual.candidates.len(), expected, "{operator:?}: {source}");
}

#[rstest::rstest]
#[case::any("any", "all", "collection_any_all")]
#[case::all("all", "any", "collection_any_all")]
#[case::list("list", "tuple", "collection_list_tuple")]
#[case::tuple("tuple", "list", "collection_list_tuple")]
#[case::min("min", "max", "collection_min_max")]
#[case::max("max", "min", "collection_min_max")]
fn declaration_only_preserves_collection_sources_and_destinations(
    #[case] source_name: &str,
    #[case] destination: &str,
    #[case] operator: &str,
    #[values(0, 1)] scenario_index: usize,
    #[values(0, 1, 2, 3)] nested_scenario_index: usize,
) {
    let declared = [source_name, destination]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let source = [
        format!("{declared}: int\n{declared}: object\nobserved = {source_name}(values)\n"),
        format!("class C:\n {declared}: int\n observed = {source_name}(values)\n"),
        format!("{declared}: int\ndef f():\n return {source_name}(values)\n"),
        format!("def f():\n class C:\n  {declared}: int\n  observed = {source_name}(values)\n"),
    ]
    .into_iter()
    .nth(nested_scenario_index)
    .unwrap();

    let output = analyze(&source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == operator)
        .collect();

    assert_eq!(candidates.len(), 1, "{source}");
    assert_eq!(candidates[0].original, source_name, "{source}");
    assert_eq!(candidates[0].replacement, destination, "{source}");
    apply_candidate_and_reparse(&source, candidates[0]);
}

#[rstest::rstest]
#[case::def_f_any_int_return_any("def f():\n any: int\n return any([])\n")]
#[case::def_f_result_any_any_int_return_result(
    "def f():\n result = any([])\n any: int\n return result\n"
)]
#[case::def_f_all_int_return_any("def f():\n all: int\n return any([])\n")]
#[case::any_custom_any_int_result_any("any = custom\nany: int\nresult = any([])\n")]
#[case::all_custom_all_int_result_any("all = custom\nall: int\nresult = any([])\n")]
#[case::any_object_custom_result_any("any: object = custom\nresult = any([])\n")]
#[case::any_custom_class_c_any_int_result_any(
    "any = custom\nclass C:\n any: int\n result = any([])\n"
)]
#[case::class_c_any_custom_any_int_result_any(
    "class C:\n any = custom\n any: int\n result = any([])\n"
)]
#[case::any_int_exec_code_result_any("any: int\nexec(code)\nresult = any([])\n")]
#[case::any_custom_member_int_result_any("(any := custom).member: int\nresult = any([])\n")]
#[case::container_all_custom_int_result_any("container[(all := custom)]: int\nresult = any([])\n")]
#[case::container_exec_code_int_result_any("container[exec(code)]: int\nresult = any([])\n")]
#[case::def_f_class_c_pass_any_int_return_any(
    "def f():\n class C:\n  pass\n any: int\n return any([])\n"
)]
fn declaration_only_retains_builtin_shadows_and_target_effects(#[case] source: &str) {
    assert!(parse_module(source).is_ok(), "{source}");

    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "collection_any_all"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::import_operator_as_op_op_object_op_object_result_op_add_2_1(
    "import operator as op\nop: object\nop: object\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_result_op_add_2_1_op_object(
    "import operator as op\nresult = op.add(2, 1)\nop: object\n"
)]
#[case::from_operator_import_add_as_fn_fn_object_result_fn_2_1(
    "from operator import add as fn\nfn: object\nresult = fn(2, 1)\n"
)]
#[case::import_operator_as_op_class_c_op_object_result_op_add_2_1(
    "import operator as op\nclass C:\n op: object\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_def_f_class_c_op_object_return_op_add_2_1(
    "import operator as op\ndef f():\n class C:\n  op: object\n return op.add(2, 1)\n"
)]
#[case::import_operator_as_op_def_f_pass_op_object_result_op_add_2_1(
    "import operator as op\ndef f():\n pass\nop: object\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_if_condition_op_object_result_op_add_2_1(
    "import operator as op\nif condition:\n op: object\nresult = op.add(2, 1)\n"
)]
fn declaration_only_preserves_operator_aliases_in_module_and_class_scopes(#[case] source: &str) {
    assert!(parse_module(source).is_ok(), "{source}");

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "operator_function")
        .collect();

    assert_eq!(candidates.len(), 1, "{source}");
    assert!(candidates[0].replacement.ends_with("sub"), "{source}");
    apply_candidate_and_reparse(source, candidates[0]);
}

#[rstest::rstest]
#[case::import_operator_as_op_class_c_op_object_result_op_add_2_1(
    "import operator as op\nclass C:\n op: object\n result = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_def_f_op_object_return_op_add_2_1(
    "import operator as op\ndef f():\n op: object\n return op.add(2, 1)\n"
)]
#[case::import_operator_as_op_def_f_result_op_add_2_1_op_object_return_re(
    "import operator as op\ndef f():\n result = op.add(2, 1)\n op: object\n return result\n"
)]
#[case::import_operator_as_op_def_f_class_c_pass_op_object_return_op_add(
    "import operator as op\ndef f():\n class C:\n  pass\n op: object\n return op.add(2, 1)\n"
)]
#[case::import_operator_as_op_op_object_custom_result_op_add_2_1(
    "import operator as op\nop: object = custom\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_op_custom_op_object_result_op_add_2_1(
    "import operator as op\nop = custom\nop: object\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_op_object_exec_code_result_op_add_2_1(
    "import operator as op\nop: object\nexec(code)\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_op_custom_member_int_result_op_add_2_1(
    "import operator as op\n(op := custom).member: int\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_container_op_custom_int_result_op_add_2_1(
    "import operator as op\ncontainer[(op := custom)]: int\nresult = op.add(2, 1)\n"
)]
#[case::import_operator_as_op_container_exec_code_int_result_op_add_2_1(
    "import operator as op\ncontainer[exec(code)]: int\nresult = op.add(2, 1)\n"
)]
#[case::if_condition_import_operator_as_op_op_object_result_op_add_2_1(
    "if condition:\n import operator as op\nop: object\nresult = op.add(2, 1)\n"
)]
#[case::def_f_import_operator_as_op_op_object_return_op_add_2_1(
    "def f():\n import operator as op\n op: object\n return op.add(2, 1)\n"
)]
fn declaration_only_retains_operator_alias_guards_and_target_effects(#[case] source: &str) {
    assert!(parse_module(source).is_ok(), "{source}");

    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator.as_str() != "operator_function"),
        "{source}"
    );
}

#[rstest::rstest]
#[case::metaclass_meta("(metaclass=Meta)")]
#[case::base("(Base)")]
#[case::bases("(*bases)")]
#[case::keywords("(**keywords)")]
#[case::object("(object)")]
#[case::metaclass_type("(metaclass=type)")]
fn prepared_namespace_blocks_direct_builtin_pairs_and_exceptions_scenario_1(
    #[case] header: &str,
    #[values(0, 1, 2, 3, 4, 5)] scenario_index: usize,
) {
    let (expression, operator) = [
        ("any([])", "collection_any_all"),
        ("all([])", "collection_any_all"),
        ("list(values)", "collection_list_tuple"),
        ("set(values)", "collection_set_frozenset"),
        ("min(values)", "collection_min_max"),
        ("sorted(values)", "structure_sorted_reversed"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let source = format!("class Subject{header}:\n    observed = {expression}\n");

    assert!(
        analyze(&source)
            .candidates
            .iter()
            .all(|c| c.operator.as_str() != operator),
        "{source}"
    );
}

#[rstest::rstest]
#[case::metaclass_meta("(metaclass=Meta)")]
#[case::base("(Base)")]
#[case::bases("(*bases)")]
#[case::keywords("(**keywords)")]
#[case::object("(object)")]
#[case::metaclass_type("(metaclass=type)")]
fn prepared_namespace_blocks_direct_builtin_pairs_and_exceptions(#[case] header: &str) {
    let source = format!(
        "class Subject{header}:\n    try: pass\n    except ValueError: pass\n    raise TypeError()\n"
    );
    assert!(analyze(&source).candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::class_subject_observed_any("class Subject:\n    observed = any([])\n", 1)]
#[case::class_subject_observed_any_2("class Subject():\n    observed = any([])\n", 1)]
#[case::class_subject_metaclass_meta_global_any_all_observed_any(
    "class Subject(metaclass=Meta):\n    global any, all\n    observed = any([])\n",
    1
)]
#[case::class_subject_metaclass_meta_global_any_observed_any(
    "class Subject(metaclass=Meta):\n    global any\n    observed = any([])\n",
    0
)]
#[case::class_subject_metaclass_meta_global_all_observed_any(
    "class Subject(metaclass=Meta):\n    global all\n    observed = any([])\n",
    0
)]
#[case::class_subject_metaclass_meta_def_method_self_return_any(
    "class Subject(metaclass=Meta):\n    def method(self): return any([])\n",
    1
)]
#[case::class_subject_metaclass_meta_def_method_self_def_closure_return_a(
    "class Subject(metaclass=Meta):\n    def method(self):\n        def closure(): return any([])\n        return closure()\n",
    1
)]
#[case::class_subject_metaclass_meta_observed_any_for_in_1(
    "class Subject(metaclass=Meta):\n    observed = [any([]) for _ in [1]]\n",
    1
)]
#[case::class_subject_metaclass_meta_observed_x_for_x_in_any(
    "class Subject(metaclass=Meta):\n    observed = [x for x in [any([])]]\n",
    0
)]
#[case::class_subject_metaclass_meta_def_method_value_any_pass(
    "class Subject(metaclass=Meta):\n    def method(value=any([])): pass\n",
    0
)]
#[case::class_subject_metaclass_meta_decorator_any_def_method_pass(
    "class Subject(metaclass=Meta):\n    @decorator(any([]))\n    def method(): pass\n",
    0
)]
#[case::class_subject_metaclass_meta_class_nested_observed_any(
    "class Subject(metaclass=Meta):\n    class Nested:\n        observed = any([])\n",
    1
)]
#[case::class_subject_metaclass_meta_class_nested_base_def_method_self_re(
    "class Subject(metaclass=Meta):\n    class Nested(Base):\n        def method(self): return any([])\n",
    1
)]
#[case::class_subject_factory_any_pass("class Subject(factory(any([]))): pass\n", 1)]
#[case::class_subject_t_factory_any_pass("class Subject[T](factory(any([]))): pass\n", 1)]
#[case::class_subject_any_factory_any_pass("class Subject[any](factory(any([]))): pass\n", 0)]
#[case::def_outer_any_custom_class_subject_metaclass_meta_nonlocal_any_ob(
    "def outer():\n    any = custom\n    class Subject(metaclass=Meta):\n        nonlocal any\n        observed = any([])\n",
    0
)]
fn prepared_namespace_preserves_lookup_bypasses_and_plain_classes(
    #[case] source: &str,
    #[case] expected: usize,
) {
    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "collection_any_all")
        .collect();

    assert_eq!(candidates.len(), expected, "{source}");
    for candidate in candidates {
        assert_eq!(candidate.original, "any");
        assert_eq!(candidate.replacement, "all");
        let start = usize::try_from(candidate.span.start).unwrap();
        assert_eq!(&source[start..start + 3], "any");
    }
}

#[rstest::rstest]
#[case::class_subject_value_int("class Subject:\n    value: int\n", 1)]
#[case::class_subject_metaclass_meta_value_int(
    "class Subject(metaclass=Meta):\n    value: int\n",
    0
)]
#[case::class_subject_base_def_method_self_value_int_pass(
    "class Subject(Base):\n    def method(self, value: int): pass\n",
    0
)]
#[case::class_subject_base_def_method_t_self_value_int_pass(
    "class Subject(Base):\n    def method[T](self, value: int): pass\n",
    0
)]
#[case::class_subject_base_def_method_self_value_int(
    "class Subject(Base):\n    def method(self):\n        value: int\n",
    1
)]
#[case::class_subject_base_global_int_value_int(
    "class Subject(Base):\n    global int\n    value: int\n",
    1
)]
fn prepared_namespace_blocks_class_visible_builtin_annotations(
    #[case] source: &str,
    #[case] expected: usize,
) {
    let output = analyze_types(source);

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == "type_nullable_add")
            .count(),
        expected,
        "{source}"
    );
}

#[rstest::rstest]
#[case::value_sequence_int("value: Sequence[int]\n", 1)]
#[case::class_subject_value_sequence_int("class Subject:\n    value: Sequence[int]\n", 1)]
#[case::class_subject_value_sequence_int_3("class Subject():\n    value: Sequence[int]\n", 1)]
#[case::class_subject_metaclass_meta_value_sequence_int(
    "class Subject(metaclass=Meta):\n    value: Sequence[int]\n",
    0
)]
#[case::class_subject_base_def_method_value_sequence_int_pass(
    "class Subject(Base):\n    def method(value: Sequence[int]): pass\n",
    0
)]
#[case::class_subject_base_def_method_t_value_sequence_int_pass(
    "class Subject(Base):\n    def method[T](value: Sequence[int]): pass\n",
    0
)]
#[case::class_subject_base_def_factory_def_inner_value_sequence_int_pass(
    "class Subject(Base):\n    def factory():\n        def inner(value: Sequence[int]): pass\n",
    1
)]
#[case::class_subject_base_global_sequence_iterable_value_sequence_int(
    "class Subject(Base):\n    global Sequence, Iterable\n    value: Sequence[int]\n",
    1
)]
#[case::class_subject_base_global_sequence_value_sequence_int(
    "class Subject(Base):\n    global Sequence\n    value: Sequence[int]\n",
    0
)]
#[case::class_subject_base_global_iterable_value_sequence_int(
    "class Subject(Base):\n    global Iterable\n    value: Sequence[int]\n",
    0
)]
#[case::class_subject_base_from_typing_import_sequence_iterable_value_seq(
    "class Subject(Base):\n    from typing import Sequence, Iterable\n    value: Sequence[int]\n",
    0
)]
#[case::def_outer_from_typing_import_sequence_iterable_class_subject_base(
    "def outer():\n    from typing import Sequence, Iterable\n    class Subject(Base):\n        nonlocal Sequence, Iterable\n        value: Sequence[int]\n",
    0
)]
fn prepared_annotation_import_stability_respects_scope_and_directives(
    #[case] body: &str,
    #[case] expected: usize,
) {
    let imports = "from typing import Sequence, Iterable\n";

    let source = format!("{imports}{body}");

    let output = analyze_types(&source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "type_sequence_iterable")
        .collect();

    assert_eq!(candidates.len(), expected, "{source}");
    for candidate in candidates {
        assert_eq!(candidate.original, "Sequence[int]");
        assert_eq!(candidate.replacement, "Iterable[int]");
    }
}

#[rstest::rstest]
#[case::value("", 0)]
#[case::global_t("    global t\n", 1)]
fn prepared_annotation_import_qualified_alias_is_checked(
    #[case] directive: &str,
    #[case] expected: usize,
) {
    let source = format!(
        "import typing as t\nclass Subject(metaclass=Meta):\n{directive}    value: t.Sequence[int]\n"
    );

    let output = analyze_types(&source);

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == "type_sequence_iterable")
            .count(),
        expected,
        "{source}"
    );
}

#[rstest::rstest]
#[case::c("C")]
#[case::c_2("_C")]
#[case::value("___")]
fn private_annotation_import_endpoints_and_spelling_boundaries(
    #[case] class: &str,
    #[values(0, 1, 2)] scenario_index: usize,
    #[values(0, 1)] nested_scenario_index: usize,
) {
    let alias = ["Alias", "__Alias", "__Alias__"]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let (outer, inner, original) = [
        ("Sequence", "Iterable", "Sequence[int]".to_owned()),
        ("Iterable", "Sequence", format!("{alias}[int]")),
    ]
    .into_iter()
    .nth(nested_scenario_index)
    .unwrap();

    let source = format!(
        "from typing import {outer}\nclass {class}:\n    from typing import {inner} as {alias}\n    value: {original}\n"
    );
    let expected = usize::from(class == "___" || alias != "__Alias");

    let output = analyze_types(&source);

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == "type_sequence_iterable")
            .count(),
        expected,
        "{source}"
    );
}

#[rstest::rstest]
#[case::from_typing_import_iterable_class_c_from_typing_import_sequence_a(
    "from typing import Iterable\nclass C:\n    from typing import Sequence as __Alias\n    _C__Alias = tuple\n    value: __Alias[int]\n"
)]
#[case::from_typing_import_sequence_class_c_from_typing_import_iterable_a(
    "from typing import Sequence\nclass C:\n    from typing import Iterable as __Alias\n    _C__Alias = tuple\n    value: Sequence[int]\n"
)]
#[case::from_typing_import_iterable_class_c_from_typing_import_sequence_a_3(
    "from typing import Iterable\nclass C:\n    from typing import Sequence as _C__Alias\n    __Alias = tuple\n    value: _C__Alias[int]\n"
)]
#[case::from_typing_import_sequence_class_c_from_typing_import_iterable_a_4(
    "from typing import Sequence\nclass C:\n    from typing import Iterable as _C__Alias\n    __Alias = tuple\n    value: Sequence[int]\n"
)]
#[case::from_typing_import_sequence_as_c_alias_iterable_class_c_global_al(
    "from typing import Sequence as _C__Alias, Iterable\nclass C:\n    global __Alias\n    __Alias = tuple\nvalue: _C__Alias[int]\n"
)]
#[case::from_typing_import_sequence_iterable_as_c_alias_class_c_global_c(
    "from typing import Sequence, Iterable as _C__Alias\nclass C:\n    global _C__Alias\n    __Alias = tuple\nvalue: Sequence[int]\n"
)]
#[case::from_typing_import_iterable_class_c_def_factory_from_typing_impor(
    "from typing import Iterable\nclass C:\n    def factory():\n        from typing import Sequence as _C__Alias\n        def write():\n            nonlocal __Alias\n            __Alias = tuple\n        write()\n        def inner(value: _C__Alias[int]): pass\n"
)]
#[case::from_typing_import_sequence_iterable_class_c_import_typing_as_t_c(
    "from typing import Sequence, Iterable\nclass C:\n    import typing as __t\n    _C__t = object()\n    value: __t.Sequence[int]\n"
)]
fn private_annotation_import_tracks_reverse_writes_and_directives(#[case] source: &str) {
    let output = analyze_types(source);

    assert!(
        !output
            .candidates
            .iter()
            .any(|candidate| candidate.operator.as_str() == "type_sequence_iterable"),
        "unsafe candidates: {source}"
    );
}

#[rstest::rstest]
#[case::from_typing_import_sequence_as_alias_iterable_value_alias_int(
    "from typing import Sequence as __Alias, Iterable\nvalue: __Alias[int]\n",
    1
)]
#[case::from_typing_import_sequence_as_c_alias_iterable_class_c_alias_tup(
    "from typing import Sequence as _C__Alias, Iterable\nclass C:\n    __Alias = tuple\nvalue: _C__Alias[int]\n",
    1
)]
#[case::from_typing_import_iterable_class_outer_class_from_typing_import(
    "from typing import Iterable\nclass Outer:\n    class ___:\n        from typing import Sequence as __Alias\n        value: __Alias[int]\n",
    1
)]
#[case::from_typing_import_iterable_class_c_def_factory_from_typing_impor(
    "from typing import Iterable\nclass C:\n    def factory():\n        from typing import Sequence as __Alias\n        def inner(value: __Alias[int]): pass\n",
    0
)]
#[case::from_typing_import_iterable_class_c_def_factory_from_typing_impor_5(
    "from typing import Iterable\nclass C:\n    def factory():\n        from typing import Sequence as _C__Alias\n        def inner(value: _C__Alias[int]): pass\n",
    0
)]
fn private_annotation_import_keeps_context_and_unrelated_scopes_separate(
    #[case] source: &str,
    #[case] expected: usize,
) {
    let output = analyze_types(source);

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == "type_sequence_iterable")
            .count(),
        expected,
        "{source}"
    );
}

#[test]
fn private_annotation_import_parameter_bindings_shadow_canonical_roots() {
    let source = "from typing import Sequence as _C__Alias, Iterable\nclass C:\n    def factory(__Alias):\n        def inner(value: _C__Alias[int]): pass\n        return inner\n";

    let output = analyze_types(source);

    assert!(output.candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::sequence("Sequence", "__Alias[int]")]
#[case::mapping("Mapping", "__Alias[str, int]")]
#[case::abstractset("AbstractSet", "__Alias[int]")]
#[case::iterable("Iterable", "__Alias[int]")]
#[case::optional("Optional", "__Alias[int]")]
fn private_annotation_import_shared_type_operators_reject_private_sources(
    #[case] imported: &str,
    #[case] annotation: &str,
) {
    let source = format!(
        "from typing import Iterable, Iterator\nclass C:\n    from typing import {imported} as __Alias\n    value: {annotation}\n"
    );

    let output = analyze_types(&source);

    assert!(
        output.candidates.is_empty(),
        "{source}: {:?}",
        output.candidates
    );
}

#[rstest::rstest]
#[case::from_typing_import_sequence_as_alias_iterable_class_c_t_alias_int(
    "from typing import Sequence as __Alias, Iterable\nclass C[T: __Alias[int]]: pass\n",
    1
)]
#[case::from_typing_import_sequence_as_other_alias_iterable_class_c_value(
    "from typing import Sequence as _Other__Alias, Iterable\nclass C:\n    value: _Other__Alias[int]\n",
    1
)]
#[case::from_typing_import_iterable_class_c_def_factory_from_typing_impor(
    "from typing import Iterable\nclass C:\n    def factory():\n        from typing import Sequence as _C__Alias\n        __Alias = tuple\n        value: _C__Alias[int]\n",
    0
)]
fn private_annotation_import_gate_preserves_header_context(
    #[case] source: &str,
    #[case] expected: usize,
) {
    let output = analyze_types(source);

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == "type_sequence_iterable")
            .count(),
        expected,
        "{source}"
    );
}

fn hierarchy_candidates(source: &str) -> Vec<(String, String)> {
    let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();
    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 100,
        },
        source,
    );
    assert_candidates_reparse(source, &output.candidates);
    output
        .candidates
        .into_iter()
        .map(super::payload::Charged::into_untracked)
        .map(|c| (c.original, c.replacement))
        .collect()
}

#[rstest::rstest]
#[case::def_f_raise_missingerror_x_from_cause("def f():\n    raise MissingError('x') from cause\n")]
#[case::def_f_try_action_except_missingerror_pass(
    "def f():\n    try: action()\n    except MissingError: pass\n"
)]
#[case::def_f_try_action_except_missingerror_pass_3(
    "def f():\n    try: action()\n    except* MissingError: pass\n"
)]
fn exception_hierarchy_mutates_local_handlers_and_raises(#[case] usage: &str) {
    let classes = "class AppError(Exception): pass\nclass MissingError(AppError): pass\nclass ConflictError(AppError): pass\n";

    assert_eq!(
        hierarchy_candidates(&format!("{classes}{usage}")),
        vec![
            ("MissingError".into(), "AppError".into()),
            ("MissingError".into(), "ConflictError".into()),
        ]
    );
}

#[test]
fn exception_hierarchy_custom_constructors_are_handler_only() {
    let classes = "class AppError(Exception): pass\nclass MissingError(AppError):\n    def __init__(self, *, code): pass\n";

    assert_eq!(
        hierarchy_candidates(&format!(
            "{classes}def f():\n    raise MissingError(code=1)\n"
        )),
        Vec::<(String, String)>::new()
    );
    assert_eq!(
        hierarchy_candidates(&format!(
            "{classes}def f():\n    try: action()\n    except MissingError: pass\n"
        )),
        vec![("MissingError".into(), "AppError".into())]
    );
}

#[rstest::rstest]
#[case::def_f_missingerror_raise_missingerror("def f(MissingError):\n    raise MissingError()\n")]
#[case::def_f_raise_missingerror_apperror_other(
    "def f():\n    raise MissingError()\n    AppError = other\n"
)]
#[case::missingerror_other_def_f_raise_missingerror(
    "MissingError = other\ndef f():\n    raise MissingError()\n"
)]
#[case::def_f_exec_apperror_other_raise_missingerror(
    "def f():\n    exec('AppError = other')\n    raise MissingError()\n"
)]
fn exception_hierarchy_rejects_shadowed_and_unsupported_classes_scenario_1(#[case] usage: &str) {
    let classes = "class AppError(Exception): pass\nclass MissingError(AppError): pass\n";

    assert!(
        hierarchy_candidates(&format!("{classes}{usage}")).is_empty(),
        "{usage}"
    );
}

#[rstest::rstest]
#[case::baseexception("BaseException")]
#[case::systemexit("SystemExit")]
#[case::keyboardinterrupt("KeyboardInterrupt")]
#[case::generatorexit("GeneratorExit")]
#[case::exceptiongroup("ExceptionGroup")]
#[case::baseexceptiongroup("BaseExceptionGroup")]
#[case::unknown("Unknown")]
fn exception_hierarchy_rejects_shadowed_and_unsupported_classes_scenario_2(#[case] base: &str) {
    assert!(hierarchy_candidates(&format!("class AppError({base}): pass\nclass MissingError(AppError): pass\ntry: action()\nexcept MissingError: pass\n")).is_empty(), "{base}");
}

#[rstest::rstest]
#[case::def_f_try_action_except_child_as_root_pass(
    "def f():\n    try: action()\n    except Child as Root: pass\n"
)]
#[case::def_f_match_x_case_x_root_pass_raise_child(
    "def f():\n    match x:\n        case {'x': Root}: pass\n    raise Child()\n"
)]
#[case::def_f_global_root_root_other_raise_child(
    "def f():\n    global Root\n    Root = other\n    raise Child()\n"
)]
#[case::root_bases_other_def_f_raise_child(
    "Root.__bases__ = (other,)\ndef f():\n    raise Child()\n"
)]
fn exception_hierarchy_preserves_alias_identity_and_scope_scenario_1(#[case] usage: &str) {
    let classes = "class Root(Exception): pass\nclass Child(Root): pass\n";

    assert!(
        hierarchy_candidates(&format!("{classes}{usage}")).is_empty(),
        "{usage}"
    );
}

#[test]
fn exception_hierarchy_preserves_alias_identity_and_scope() {
    let classes = "class Root(Exception): pass\nclass Child(Root): pass\n";

    assert_eq!(
        hierarchy_candidates(&format!(
            "{classes}class Service:\n    Root = other\n    def f(self):\n        raise Child()\n"
        )),
        vec![("Child".into(), "Root".into())]
    );
}

#[rstest::rstest]
#[case::from_errors_import_child_as_c_root_as_r_def_f_raise_c("from .errors import Child as C, Root as R\ndef f():\n    raise C()\n", vec![("C", "R")])]
#[case::import_pkg_errors_as_e_def_f_raise_e_child("import pkg.errors as e\ndef f():\n    raise e.Child()\n", vec![("e.Child", "e.Root"), ("e.Child", "e.Sibling")])]
#[case::from_errors_import_child_def_f_raise_child("from .errors import Child\ndef f():\n    raise Child()\n", vec![])]
fn exception_hierarchy_cross_module_and_relative_imports(
    #[case] source: &str,
    #[case]
    #[by_ref]
    expected: &[(&str, &str)],
) {
    use super::exception_hierarchy::ExceptionIndex;
    let errors =
        "class Root(Exception): pass\nclass Child(Root): pass\nclass Sibling(Root): pass\n";

    let path = Utf8Path::new("pkg/service.py");
    let index = ExceptionIndex::from_sources(
        &[
            ("pkg/errors.py".into(), errors.into()),
            (path.to_owned(), source.into()),
        ],
        &["".into()],
        &|| false,
    )
    .unwrap();
    let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();

    let output = super::analyze_source_with_exceptions(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path,
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 100,
        },
        source,
        &|| false,
        Some(&index),
    )
    .unwrap();

    assert_eq!(
        output
            .candidates
            .iter()
            .map(|c| (c.original.as_str(), c.replacement.as_str()))
            .collect::<Vec<_>>(),
        expected
    );
    for c in &output.candidates {
        apply_candidate_and_reparse(source, c);
    }
}

#[rstest::rstest]
#[case::class_child_root_pass_class_root_exception_pass_def_f_raise_child(
    "class Child(Root): pass\nclass Root(Exception): pass\ndef f():\n    raise Child()\n"
)]
#[case::class_root_exception_pass_decorate_class_child_root_pass_def_f_ra(
    "class Root(Exception): pass\n@decorate\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
)]
#[case::class_root_exception_pass_if_condition_class_child_root_pass_def(
    "class Root(Exception): pass\nif condition:\n    class Child(Root): pass\ndef f():\n    raise Child()\n"
)]
#[case::class_root_exception_pass_class_child_root_mixin_pass_def_f_raise(
    "class Root(Exception): pass\nclass Child(Root, Mixin): pass\ndef f():\n    raise Child()\n"
)]
#[case::class_root_exception_def_new_cls_pass_class_child_root_pass_def_f(
    "class Root(Exception):\n    def __new__(cls): pass\nclass Child(Root): pass\ndef f():\n    raise Child\n"
)]
#[case::class_root_oserror_pass_class_child_root_pass_def_f_raise_child(
    "class Root(OSError): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
)]
#[case::class_root_exception_pass_class_child_root_pass_def_f_root_def_in(
    "class Root(Exception): pass\nclass Child(Root): pass\ndef f(Root):\n    def inner():\n        raise Child()\n"
)]
fn exception_hierarchy_is_conservative_at_definition_and_constructor_boundaries_table_1(
    #[case] source: &str,
) {
    assert!(hierarchy_candidates(source).is_empty(), "{source}");
}

#[test]
fn exception_hierarchy_is_conservative_at_definition_and_constructor_boundaries() {
    let source = "class Root(Exception): pass\nclass Child(Root): pass\ndef f(Root):\n    raise Child()\nclass Later(Child): pass\n";

    assert!(
        hierarchy_candidates(source).is_empty(),
        "later definitions must not become visible inside an earlier function"
    );
    let source = "class Root(RuntimeError): pass\nclass Child(Root): pass\ntry: action()\nexcept Child: pass\n";
    assert_eq!(
        hierarchy_candidates(source),
        vec![("Child".into(), "Root".into())]
    );
}

#[rstest::rstest]
#[case::zero(0)]
#[case::limit_1(1)]
#[case::limit_2(2)]
#[case::limit_3(3)]
#[case::limit_4(4)]
#[case::limit_5(5)]
#[case::at_capacity(6)]
fn exception_hierarchy_respects_candidate_prefix_selection_and_cancellation_prefix_boundary(
    #[case] limit: usize,
) {
    let source = "class Root(Exception): pass\nclass A(Root): pass\nclass B(Root): pass\nclass C(Root): pass\ndef f():\n    raise A()\ndef g():\n    raise B()\n";
    let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();
    let mut request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let full = analyze_source(&request, source);

    assert_eq!(full.candidates.len(), 6);

    request.max_candidates = limit;

    let output = analyze_source(&request, source);

    assert_eq!(output.candidates, full.candidates[..limit]);
    assert_eq!(output.truncated, limit < 6);
}

proptest::proptest! {
    #[test]
    fn exception_hierarchy_respects_candidate_prefix_selection_and_cancellation_generated_prefix(limit in 0usize..128) {
    let source = "class Root(Exception): pass\nclass A(Root): pass\nclass B(Root): pass\nclass C(Root): pass\ndef f():\n    raise A()\ndef g():\n    raise B()\n";
    let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();
    let mut request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let full = analyze_source(&request, source);

    assert_eq!(full.candidates.len(), 6);

    request.max_candidates = limit;

    let output = analyze_source(&request, source);

    proptest::prop_assert_eq!(&output.candidates[..], &full.candidates[..limit.min(full.candidates.len())]);
    proptest::prop_assert_eq!(output.truncated, limit < 6);

    }
}

#[test]
fn exception_hierarchy_respects_candidate_prefix_selection_and_cancellation() {
    let source = "class Root(Exception): pass\nclass A(Root): pass\nclass B(Root): pass\nclass C(Root): pass\ndef f():\n    raise A()\ndef g():\n    raise B()\n";
    let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();
    let mut request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let full = analyze_source(&request, source);

    assert_eq!(full.candidates.len(), 6);

    request.max_candidates = 100;
    let symbols = ["g".to_owned()];
    request.symbols = &symbols;
    assert_eq!(analyze_source(&request, source).candidates.len(), 3);
    assert!(matches!(
        analyze_source_cancellable(&request, source, || true),
        Err(super::AnalysisError::Cancelled)
    ));
}

#[rstest::rstest]
#[case::import_cycle(0, vec![])]
#[case::unloaded_submodule(1, vec![])]
#[case::loaded_submodule(2, vec!["pkg.errors.Root"])]
fn exception_hierarchy_requires_acyclic_imports_and_loaded_submodules(
    #[case] scenario_index: usize,
    #[case]
    #[by_ref]
    expected: &[&str],
) {
    use super::exception_hierarchy::ExceptionIndex;
    let cases = [
        vec![
            (
                "a.py",
                "from b import Child\nclass Root(Exception): pass\ndef f():\n    raise Child()\n",
            ),
            ("b.py", "from a import Root\nclass Child(Root): pass\n"),
        ],
        vec![
            (
                "a.py",
                "import pkg\ndef f():\n    raise pkg.errors.Child()\n",
            ),
            (
                "pkg/errors.py",
                "class Root(Exception): pass\nclass Child(Root): pass\n",
            ),
            ("pkg/__init__.py", ""),
        ],
        vec![
            (
                "a.py",
                "import pkg.errors\ndef f():\n    raise pkg.errors.Child()\n",
            ),
            (
                "pkg/errors.py",
                "class Root(Exception): pass\nclass Child(Root): pass\n",
            ),
            ("pkg/__init__.py", ""),
        ],
    ];

    let sources = cases.into_iter().nth(scenario_index).unwrap();

    let input = sources
        .iter()
        .map(|(p, s)| ((*p).into(), (*s).to_owned()))
        .collect::<Vec<_>>();
    let index = ExceptionIndex::from_sources(&input, &["".into()], &|| false).unwrap();
    let parsed = parse_module(sources[0].1).unwrap();

    let mut output = Vec::new();
    index
        .collect(
            Utf8Path::new("a.py"),
            parsed.syntax(),
            100,
            &|| false,
            |_, name| {
                output.push(name);
                Ok(())
            },
        )
        .unwrap();

    assert_eq!(output, expected, "{sources:?}");
}

#[rstest::rstest]
#[case::class_root_exception_pass_class_child_root_pass_def_bind_x_child(
    "class Root(Exception): pass\nclass Child(Root): pass\ndef bind(x=(Child := ValueError)): pass\ndef f():\n    raise Root()\n"
)]
#[case::from_builtins_import_exception_as_e_class_root_e_base_pass_class(
    "from builtins import Exception as E\nclass Root(E.__base__): pass\nclass Child(Root): pass\ndef f():\n    raise Child()\n"
)]
#[case::class_root_exception_pass_class_child_root_pass_class_other_child(
    "class Root(Exception): pass\nclass Child(Root): pass\nclass Other((Child := ValueError)): pass\ndef f():\n    raise Root()\n"
)]
fn exception_hierarchy_rejects_definition_header_rebinding_and_builtin_attributes(
    #[case] source: &str,
) {
    assert!(hierarchy_candidates(source).is_empty(), "{source}");
}

#[rstest::rstest]
#[case::class_class_exception_pass_class_child_class_pass_class_other_exc(
    "class __class__(Exception): pass\nclass Child(__class__): pass\nclass Other(Exception):\n    def f(self):\n        raise __class__()\n"
)]
#[case::class_root_exception_pass_class_child_root_pass_class_other_def_f(
    "class Root(Exception): pass\nclass __Child(Root): pass\nclass Other:\n    def f(self):\n        raise __Child()\n"
)]
fn exception_hierarchy_respects_implicit_class_cells_and_private_names(#[case] source: &str) {
    assert!(hierarchy_candidates(source).is_empty(), "{source}");
}

#[rstest::rstest]
#[case::class_service_def_f_self_child_raise_service_child(
    "class Service:\n    def f(self, __Child):\n        raise _Service__Child()\n"
)]
#[case::class_service_def_f_self_child_valueerror_raise_service_child(
    "class Service:\n    def f(self):\n        __Child = ValueError\n        raise _Service__Child()\n"
)]
#[case::class_service_def_f_self_child_def_nested_raise_service_child(
    "class Service:\n    def f(self, __Child):\n        def nested():\n            raise _Service__Child()\n"
)]
#[case::class_service_def_change_self_global_child_child_valueerror_def_f(
    "class Service:\n    def change(self):\n        global __Child\n        __Child = ValueError\ndef f():\n    raise _Service__Child()\n"
)]
fn exception_hierarchy_audit_mangled_bindings_shadow_module_aliases(#[case] usage: &str) {
    let classes = "class Root(Exception): pass\nclass _Service__Child(Root): pass\n";

    assert!(
        hierarchy_candidates(&format!("{classes}{usage}")).is_empty(),
        "{usage}"
    );
}

#[test]
fn exception_hierarchy_audit_mangled_spelling_without_local_binding_remains_eligible() {
    assert_eq!(
        hierarchy_candidates(
            "class Root(Exception): pass\nclass _Service__Child(Root): pass\nclass Service:\n    def f(self):\n        raise _Service__Child()\n"
        ),
        vec![("_Service__Child".into(), "Root".into())]
    );
}

// Exact-record and oracle scenarios state their operator selection at each call site.
// Keeping this fixture independent of Default prevents unrelated new defaults
// from changing the behavior under test.
fn analyze_selected(selected: &[&str], source: &str) -> super::AnalyzerOutput {
    analyze_selected_with(
        selected,
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        10_000,
        source,
    )
}

fn analyze_selected_with(
    selected: &[&str],
    path: &Utf8Path,
    lines: &[LineRange],
    symbols: &[String],
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    let operators = serde_json::from_value(serde_json::json!(selected)).unwrap();
    analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
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

fn analyze_selected_with_profile(
    selected: &[&str],
    profile: MutationProfile,
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    let operators = serde_json::from_value(serde_json::json!(selected)).unwrap();
    analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile,
            max_candidates,
        },
        source,
    )
}

#[test]
fn eager_enum_imports_prove_member_replacements() {
    let source = "from enum import Enum\nclass Color(Enum):\n    RED = 1\n    BLUE = 2\ndef selected(): return Color.RED\n";

    let output = analyze_with_only_operator(source, MutationOperator::EnumMemberReplace);

    assert_eq!(output.candidates.len(), 1);
}

#[rstest::rstest]
#[case::lazy_import_json_flag_true("lazy import json\nflag = True\n")]
#[case::lazy_from_pathlib_import_path_flag_true("lazy from pathlib import Path\nflag = True\n")]
#[case::result_xs_for_xs_in_rows_if_true("result = [*xs for xs in rows if True]\n")]
#[case::result_xs_for_xs_in_rows_if_true_4("result = {*xs for xs in rows if True}\n")]
#[case::result_d_for_d_in_rows_if_true("result = {**d for d in rows if True}\n")]
#[case::result_xs_for_xs_in_rows_if_true_6("result = (*xs for xs in rows if True)\n")]
#[case::async_def_f_rows_return_xs_async_for_xs_in_rows_if_true(
    "async def f(rows):\n    return [*xs async for xs in rows if True]\n"
)]
fn python315_new_syntax_preserves_boolean_candidates_and_spans(#[case] source: &str) {
    let output = analyze_with_only_operator(source, MutationOperator::BooleanLiteral);

    assert!(
        output.diagnostics.is_empty(),
        "{source}: {:?}",
        output.diagnostics
    );
    assert_eq!(output.candidates.len(), 1, "{source}");
    let candidate = &output.candidates[0];
    assert_eq!(candidate.original, "True", "{source}");
    assert_eq!(
        usize::try_from(candidate.span.start).unwrap(),
        source.find("True").unwrap()
    );
}

#[rstest::rstest]
#[case::import_operator_as_op("import operator as op", "op.add(1, 2)")]
#[case::from_operator_import_add("from operator import add", "add(1, 2)")]
fn python315_lazy_callable_aliases_are_not_proven_scenario_1(
    #[case] import: &str,
    #[case] call: &str,
    #[values(0, 1, 2)] scenario_index: usize,
) {
    let eager = format!("{import}\nresult = {call}\n");

    assert_eq!(
        analyze_with_only_operator(&eager, MutationOperator::OperatorFunction)
            .candidates
            .len(),
        1
    );

    let source = [
        format!("lazy {eager}"),
        format!("__lazy_modules__ = ['operator']\n{eager}"),
        format!("__lazy_modules__ = provider()\n{eager}"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let output = analyze_with_only_operator(&source, MutationOperator::OperatorFunction);

    assert!(output.diagnostics.is_empty(), "{source}");
    assert!(output.candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::import_operator_as_op("import operator as op", "op.add(1, 2)")]
#[case::from_operator_import_add("from operator import add", "add(1, 2)")]
fn python315_lazy_callable_aliases_are_not_proven(#[case] import: &str, #[case] call: &str) {
    let eager = format!("{import}\nresult = {call}\n");

    assert_eq!(
        analyze_with_only_operator(&eager, MutationOperator::OperatorFunction)
            .candidates
            .len(),
        1
    );
}

#[rstest::rstest]
#[case::from_typing_import_sequence("from typing import Sequence")]
#[case::import_typing("import typing")]
fn python315_lazy_annotation_aliases_are_not_proven_scenario_1(
    #[case] import: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    let name = if import.starts_with("from") {
        "Sequence"
    } else {
        "typing.Sequence"
    };
    let eager = format!("{import}\nvalue: {name}[int]\n");

    assert_eq!(
        analyze_with_only_operator(&eager, MutationOperator::TypeListSequence)
            .candidates
            .len(),
        1
    );

    let source = [
        format!("lazy {eager}"),
        format!("__lazy_modules__ = ['typing']\n{eager}"),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    assert!(
        analyze_with_only_operator(&source, MutationOperator::TypeListSequence)
            .candidates
            .is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::from_typing_import_sequence("from typing import Sequence")]
#[case::import_typing("import typing")]
fn python315_lazy_annotation_aliases_are_not_proven(#[case] import: &str) {
    let name = if import.starts_with("from") {
        "Sequence"
    } else {
        "typing.Sequence"
    };
    let eager = format!("{import}\nvalue: {name}[int]\n");

    assert_eq!(
        analyze_with_only_operator(&eager, MutationOperator::TypeListSequence)
            .candidates
            .len(),
        1
    );
}

#[rstest::rstest]
#[case::lazy_import("lazy ")]
#[case::lazy_module("__lazy_modules__ = ['enum']\n")]
fn python315_lazy_enum_aliases_are_not_proven(#[case] prefix: &str) {
    let eager = "from enum import Enum\nclass Color(Enum):\n    RED = 1\n    BLUE = 2\ndef selected(): return Color.RED\n";
    let source = format!("{prefix}{eager}");

    assert!(
        analyze_with_only_operator(&source, MutationOperator::EnumMemberReplace)
            .candidates
            .is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::lazy("lazy ")]
#[case::lazy_modules_errors("__lazy_modules__ = ['errors']\n")]
fn python315_lazy_project_exception_imports_are_not_proven(#[case] prefix: &str) {
    use super::exception_hierarchy::ExceptionIndex;

    let source = format!("{prefix}from errors import Root, Child\ndef f(): raise Child()\n");
    let input = vec![
        (
            "errors.py".into(),
            "class Root(Exception): pass\nclass Child(Root): pass\n".into(),
        ),
        ("service.py".into(), source.clone()),
    ];
    let index = ExceptionIndex::from_sources(&input, &["".into()], &|| false).unwrap();
    let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();
    let request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("service.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };

    let output =
        super::analyze_source_with_exceptions(&request, &source, &|| false, Some(&index)).unwrap();

    assert!(output.candidates.is_empty(), "{source}");
}

#[rstest::rstest]
#[case::import_errors_as_e("import errors as e")]
#[case::lazy_import_errors_as_e("lazy import errors as e")]
#[case::lazy_modules_errors_import_errors_as_e("__lazy_modules__ = ['errors']\nimport errors as e")]
fn python315_lazy_external_writes_invalidate_other_modules(
    #[case] import: &str,
    #[values(0, 1)] scenario_index: usize,
) {
    use super::exception_hierarchy::ExceptionIndex;

    let write = ["", "e.Child = ValueError\n"]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let source = "from errors import Root, Child\ndef f(): raise Child()\n";
    let input = vec![
        (
            "errors.py".into(),
            "class Root(Exception): pass\nclass Child(Root): pass\n".into(),
        ),
        ("patcher.py".into(), format!("{import}\n{write}")),
        ("service.py".into(), source.into()),
    ];
    let index = ExceptionIndex::from_sources(&input, &["".into()], &|| false).unwrap();
    let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();
    let request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("service.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };

    let output =
        super::analyze_source_with_exceptions(&request, source, &|| false, Some(&index)).unwrap();

    assert_eq!(
        output.candidates.is_empty(),
        !write.is_empty(),
        "{import}\n{write}"
    );
}

#[rstest::rstest]
#[case::value_0_add_1_2_for_add_in_values("{**{0: add(1, 2)} for add in values}", 0)]
#[case::d_for_d_in_make_rows_add_1_2("{**d for d in make_rows(add(1, 2))}", 1)]
#[case::add_1_2_for_add_in_values("[*[add(1, 2)] for add in values]", 0)]
#[case::add_1_2_for_add_in_values_4("(*[add(1, 2)] for add in values)", 0)]
#[case::add_1_2_for_add_in_values_5("{*[add(1, 2)] for add in values}", 0)]
fn python315_unpacking_comprehensions_preserve_import_shadowing(
    #[case] expression: &str,
    #[case] expected: usize,
) {
    let source = format!("from operator import add\nresult = {expression}\n");

    let output = analyze_with_only_operator(&source, MutationOperator::OperatorFunction);

    assert!(output.diagnostics.is_empty(), "{source}");
    assert_eq!(output.candidates.len(), expected, "{source}");
}

#[rstest::rstest]
#[case::import_operator_as_op_lazy_import_operator_as_late_late_add_repla(
    "import operator as op\nlazy import operator as late\nlate.add = replacement\nresult = op.add(1,2)\n"
)]
#[case::import_operator_as_op_lazy_import_operator_as_late_escape_late_re(
    "import operator as op\nlazy import operator as late\nescape(late)\nresult = op.add(1,2)\n"
)]
fn python315_lazy_stdlib_aliases_preserve_write_and_escape_hazards_table_1(#[case] source: &str) {
    assert!(
        analyze_with_only_operator(source, MutationOperator::OperatorFunction)
            .candidates
            .is_empty(),
        "{source}"
    );
}

#[test]
fn python315_lazy_stdlib_aliases_preserve_write_and_escape_hazards() {
    let source = "from decimal import Decimal\nlazy from decimal import Decimal as late\nescape(late)\nresult = Decimal.from_float(value)\n";

    assert_eq!(
        analyze_with_only_operator(source, MutationOperator::StdlibConversionCallRemove)
            .candidates
            .len(),
        0
    );
}

#[rstest::rstest]
#[case::lazy_import_enum_as_late_escape_late("lazy import enum as late\nescape(late)\n")]
#[case::lazy_from_enum_import_enum_as_late_late_new_replacement(
    "lazy from enum import Enum as late\nlate.__new__ = replacement\n"
)]
fn python315_lazy_enum_aliases_preserve_write_and_escape_hazards(#[case] action: &str) {
    let source = format!(
        "from enum import Enum\n{action}class Color(Enum):\n    RED = 1\n    BLUE = 2\ndef selected(): return Color.RED\n"
    );

    assert!(
        analyze_with_only_operator(&source, MutationOperator::EnumMemberReplace)
            .candidates
            .is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::for_x_in_values_consume_x(
    "for x in values:\n    consume(x)\n",
    "for x in values:\n    consume(x)\n    break\n"
)]
#[case::while_ready_work("while ready(): work()\n", "while ready(): work(); break\n")]
fn loop_break_insert_appends_to_for_and_while_bodies(#[case] source: &str, #[case] expected: &str) {
    let operator = MutationOperator::from_name("loop_break_insert")
        .expect("loop_break_insert is a registered opt-out operator");

    let output = analyze_with_only_operator(source, operator);

    assert_eq!(output.candidates.len(), 1, "{source}");
    let candidate = &output.candidates[0];
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    let mutated = format!(
        "{}{}{}",
        &source[..start],
        candidate.replacement,
        &source[end..]
    );
    assert_eq!(mutated, expected);
    assert!(parse_module(&mutated).is_ok(), "{mutated}");
}

#[rstest::rstest]
#[case::for_x_in_xs_consume_keep_else_done(
    "for x in xs:\r\n\tconsume(\"値\")  # keep\r\nelse:\r\n\tdone()\r\n",
    "for x in xs:\r\n\tconsume(\"値\")  # keep\r\n\tbreak\r\nelse:\r\n\tdone()\r\n"
)]
#[case::for_x_in_xs_a_b_tail(
    "for x in xs: a(); b(); # tail\n",
    "for x in xs: a(); b(); break; # tail\n"
)]
#[case::while_ready_if_flag_work_tail(
    "while ready():\n    if flag:\n        work() # tail",
    "while ready():\n    if flag:\n        work() # tail\n    break"
)]
#[case::for_x_in_xs("for x in xs:\n    値", "for x in xs:\n    値\n    break")]
#[case::for_x_in_xs_work_x_tail(
    "for x in xs: work(\n    x\n) # tail\n",
    "for x in xs: work(\n    x\n); break # tail\n"
)]
#[case::async_def_f_xs_async_for_x_in_xs_await_consume_x(
    "async def f(xs):\n    async for x in xs:\n        await consume(x)\n",
    "async def f(xs):\n    async for x in xs:\n        await consume(x)\n        break\n"
)]
fn loop_break_insert_preserves_layout_and_suite_boundaries(
    #[case] source: &str,
    #[case] expected: &str,
) {
    let output = analyze_selected(&["loop_break_insert"], source);

    assert!(output.diagnostics.is_empty(), "{source}");
    assert_eq!(output.candidates.len(), 1, "{source}");
    let candidate = &output.candidates[0];
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[start..end], candidate.original);
    assert!(end > start);
    let mutated = format!(
        "{}{}{}",
        &source[..start],
        candidate.replacement,
        &source[end..]
    );
    assert_eq!(mutated, expected);
    assert!(parse_module(&mutated).is_ok(), "{mutated}");
}

#[rstest::rstest]
#[case::input_break("break")]
#[case::input_continue("continue")]
#[case::input_return("return")]
#[case::raise_runtimeerror("raise RuntimeError()")]
fn loop_break_insert_skips_unreachable_tails_but_visits_nested_loops_table_1(#[case] exit: &str) {
    let source = format!("def f(xs):\n    for x in xs:\n        {exit}\n        work()\n");

    assert_eq!(
        analyze_selected(&["loop_break_insert"], &source)
            .candidates
            .len(),
        0
    );
}

#[test]
fn loop_break_insert_skips_unreachable_tails_but_visits_nested_loops() {
    let source = "for x in xs:\n    for y in ys:\n        consume(x, y)\nelse:\n    for z in zs:\n        finish(z)\n";

    let output = analyze_selected(&["loop_break_insert"], source);

    assert_eq!(output.candidates.len(), 3);
    let replacements: Vec<_> = output
        .candidates
        .iter()
        .map(|c| c.replacement.as_str())
        .collect();
    assert!(replacements.contains(&"\n    break\n"));
    assert!(replacements.contains(&"\n        break\n"));
    for candidate in &output.candidates {
        let start = usize::try_from(candidate.span.start).unwrap();
        let end = start + usize::try_from(candidate.span.length).unwrap();
        let mutated = format!(
            "{}{}{}",
            &source[..start],
            candidate.replacement,
            &source[end..]
        );
        assert!(parse_module(&mutated).is_ok(), "{mutated}");
    }
}

#[test]
fn loop_break_insert_respects_selection_prefix_cancellation_and_payload() {
    let source = "def first(xs):\n    for x in xs:\n        consume(x)\ndef second(xs):\n    while ready():\n        work()\n";
    let all = analyze_selected(&["loop_break_insert"], source);

    assert_eq!(all.candidates.len(), 2);
    let one = analyze_selected_with(
        &["loop_break_insert"],
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        1,
        source,
    );
    assert_eq!(one.candidates.len(), 1);
    assert!(one.truncated);
    assert_eq!(one.candidates[0].span, all.candidates[0].span);
    let chosen = analyze_selected_with(
        &["loop_break_insert"],
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 6, end: 6 }],
        &["second".to_owned()],
        10,
        source,
    );
    assert_eq!(chosen.candidates.len(), 1);
    assert_eq!(chosen.candidates[0].symbol.as_deref(), Some("second"));
    let operators = serde_json::from_value(serde_json::json!(["loop_break_insert"])).unwrap();
    let request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::new(1),
        path: Utf8Path::new("pkg/sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
    assert!(analyze_source_cancellable(&request, source, || false).is_err());
}

#[test]
fn loop_break_insert_does_not_enter_a_continued_logical_line() {
    let source = "for x in xs:\n    consume(x) \\\n\n";

    let output = analyze_selected(&["loop_break_insert"], source);

    assert_eq!(output.candidates.len(), 1);
    let c = &output.candidates[0];
    let start = usize::try_from(c.span.start).unwrap();
    let end = start + usize::try_from(c.span.length).unwrap();
    let mutant = format!("{}{}{}", &source[..start], c.replacement, &source[end..]);
    assert!(parse_module(&mutant).is_ok(), "{mutant}");
}

#[test]
fn loop_break_insert_shared_anchor_has_stable_prefix_and_outer_scope() {
    let source = "def outer(xs, ys):\n    for x in xs:\n        for y in ys:\n            work()\n";
    let all = analyze_selected(&["loop_break_insert"], source);

    assert_eq!(all.candidates.len(), 2);
    assert_eq!(all.candidates[0].span, all.candidates[1].span);
    let prefix = analyze_selected_with(
        &["loop_break_insert"],
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        1,
        source,
    );
    assert_eq!(prefix.candidates.len(), 1);
    assert_eq!(
        prefix.candidates[0].replacement,
        all.candidates[0].replacement
    );
    assert!(prefix.truncated);
    let source = "def outer(xs):\n    for x in xs:\n        def inner():\n            return x\n";

    let output = analyze_selected(&["loop_break_insert"], source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("outer"));
}

#[test]
fn iterator_stop_insert_preserves_docstring_and_original_body() {
    let operator = MutationOperator::from_name("iterator_stop_insert")
        .expect("iterator_stop_insert is registered by default");
    let source = "class Items:\n    def __next__(self):\n        \"\"\"Read one item.\"\"\"\n        return self.read()\n";

    let output = analyze_with_only_operator(source, operator);

    assert_eq!(output.candidates.len(), 1);
    let c = &output.candidates[0];
    let start = usize::try_from(c.span.start).unwrap();
    let end = start + usize::try_from(c.span.length).unwrap();
    let mutant = format!("{}{}{}", &source[..start], c.replacement, &source[end..]);
    assert_eq!(
        mutant,
        "class Items:\n    def __next__(self):\n        \"\"\"Read one item.\"\"\"\n        raise StopIteration()\n        return self.read()\n"
    );
    assert!(parse_module(&mutant).is_ok());
}

#[rstest::rstest]
#[case::class_it_def_next_self_return_1(
    "class It:\n    def __next__(self): return 1\n",
    "class It:\n    def __next__(self): raise StopIteration(); return 1\n"
)]
#[case::class_it_async_def_anext_self_return_await_self_read(
    "class It:\r\n\tasync def __anext__(self):\r\n\t\treturn await self.read()\r\n",
    "class It:\r\n\tasync def __anext__(self):\r\n\t\traise StopAsyncIteration()\r\n\t\treturn await self.read()\r\n"
)]
#[case::class_it_def_next_self_doc_return_1_tail(
    "class It:\n    def __next__(self): \"doc\"; return 1 # tail\n",
    "class It:\n    def __next__(self): \"doc\"; raise StopIteration(); return 1 # tail\n"
)]
#[case::class_it_def_next_self_return_1_4(
    "class It:\n    def __next__(self): \\\n        return 1\n",
    "class It:\n    def __next__(self): \\\n        raise StopIteration(); return 1\n"
)]
#[case::class_it_def_next_self_def_gen_yield_1_return_next_gen(
    "class It:\n    def __next__(self):\n        def gen():\n            yield 1\n        return next(gen())\n",
    "class It:\n    def __next__(self):\n        raise StopIteration()\n        def gen():\n            yield 1\n        return next(gen())\n"
)]
#[case::class_it_def_next_self_decorate_def_inner_return_1_return_inner(
    "class It:\n    def __next__(self):\n        @decorate\n        def inner(): return 1\n        return inner()\n",
    "class It:\n    def __next__(self):\n        raise StopIteration()\n        @decorate\n        def inner(): return 1\n        return inner()\n"
)]
fn iterator_stop_insert_handles_async_inline_and_nested_scopes(
    #[case] source: &str,
    #[case] expected: &str,
) {
    let output = analyze_selected(
        &["iterator_stop_insert", "async_iterator_stop_insert"],
        source,
    );

    assert_eq!(
        output.candidates.len(),
        1,
        "{source}: {:?}",
        output.diagnostics
    );
    let c = &output.candidates[0];
    let start = usize::try_from(c.span.start).unwrap();
    let end = start + usize::try_from(c.span.length).unwrap();
    let mutant = format!("{}{}{}", &source[..start], c.replacement, &source[end..]);
    assert_eq!(mutant, expected);
    assert!(parse_module(&mutant).is_ok(), "{mutant}");
    assert!(c.symbol.as_deref().is_some_and(|s| s.starts_with("It.__")));
}

#[rstest::rstest]
#[case::def_next_self_return_1("def __next__(self): return 1\n")]
#[case::class_it_async_def_next_self_return_1(
    "class It:\n    async def __next__(self): return 1\n"
)]
#[case::class_it_def_anext_self_return_1("class It:\n    def __anext__(self): return 1\n")]
#[case::class_it_def_next_self_yield_1("class It:\n    def __next__(self): yield 1\n")]
#[case::class_it_async_def_anext_self_yield_1(
    "class It:\n    async def __anext__(self): yield 1\n"
)]
#[case::class_it_def_next_self_def_f_x_yield_1_pass_return_f(
    "class It:\n    def __next__(self):\n        def f(x=(yield 1)): pass\n        return f()\n"
)]
#[case::class_it_decorate_def_next_self_return_1(
    "class It:\n    @decorate\n    def __next__(self): return 1\n"
)]
#[case::class_it_def_outer_self_def_next_self_return_1_return_next(
    "class It:\n    def outer(self):\n        def __next__(self): return 1\n        return __next__\n"
)]
#[case::stopiteration_custom_class_it_def_next_self_return_1(
    "StopIteration = Custom\nclass It:\n    def __next__(self): return 1\n"
)]
#[case::class_it_def_next_self_return_1_stopiteration_custom(
    "class It:\n    def __next__(self): return 1\nStopIteration = Custom\n"
)]
#[case::class_it_stopiteration_def_next_self_return_1(
    "class It[StopIteration]:\n    def __next__(self): return 1\n"
)]
#[case::class_it_def_next_stopiteration_self_return_1(
    "class It:\n    def __next__[StopIteration](self): return 1\n"
)]
#[case::stopasynciteration_custom_class_it_async_def_anext_self_return_1(
    "StopAsyncIteration = Custom\nclass It:\n    async def __anext__(self): return 1\n"
)]
#[case::class_it_def_next_self_stopiteration_return_1(
    "class It:\n    def __next__(self, StopIteration): return 1\n"
)]
#[case::class_it_def_next_self_stopiteration_custom_return_1(
    "class It:\n    def __next__(self):\n        StopIteration = Custom\n        return 1\n"
)]
#[case::class_it_def_next_self_return_1_stopiteration_custom_16(
    "class It:\n    def __next__(self):\n        return 1\n        StopIteration = Custom\n"
)]
#[case::class_it_def_next_self_raise_stopiteration(
    "class It:\n    def __next__(self): raise StopIteration()\n"
)]
#[case::class_it_async_def_anext_self_raise_stopasynciteration(
    "class It:\n    async def __anext__(self): raise StopAsyncIteration\n"
)]
#[case::from_unknown_import_class_it_def_next_self_return_1(
    "from unknown import *\nclass It:\n    def __next__(self): return 1\n"
)]
#[case::exec_code_class_it_def_next_self_return_1(
    "exec(code)\nclass It:\n    def __next__(self): return 1\n"
)]
fn iterator_stop_insert_excludes_generators_shadowing_and_non_methods(#[case] source: &str) {
    let output = analyze_selected(
        &["iterator_stop_insert", "async_iterator_stop_insert"],
        source,
    );

    assert_eq!(output.candidates.len(), 0, "{source}");
}

#[test]
fn iterator_stop_insert_respects_builtin_class_scope_and_prefix_budgets() {
    let source = "class It:\n    StopIteration = Custom\n    def __next__(self): return 1\n    async def __anext__(self): return 2\n";
    let selected = ["iterator_stop_insert", "async_iterator_stop_insert"];
    let all = analyze_selected(&selected, source);

    assert_eq!(all.candidates.len(), 2);
    let prefix = analyze_selected_with(
        &selected,
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        1,
        source,
    );
    assert_eq!(prefix.candidates.len(), 1);
    assert!(prefix.truncated);
    assert_eq!(
        prefix.candidates[0].replacement,
        all.candidates[0].replacement
    );
    let filtered = analyze_selected_with(
        &selected,
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 4, end: 4 }],
        &["It.__anext__".to_owned()],
        10,
        source,
    );
    assert_eq!(filtered.candidates.len(), 1);
    assert_eq!(
        filtered.candidates[0].operator.as_str(),
        "async_iterator_stop_insert"
    );
    let operators = serde_json::from_value(serde_json::json!(selected)).unwrap();
    let request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::new(1),
        path: Utf8Path::new("pkg/sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
    assert!(analyze_source_cancellable(&request, source, || false).is_err());
}

#[rstest::rstest]
#[case::class_it_def_next_self_global_stopiteration_return_1(
    "class It:\n    def __next__(self):\n        global StopIteration\n        return 1\n"
)]
#[case::class_it_async_def_anext_self_global_stopasynciteration_return_1(
    "class It:\n    async def __anext__(self):\n        global StopAsyncIteration\n        return 1\n"
)]
#[case::class_it_def_next_self_if_flag_global_stopiteration_return_1(
    "class It:\n    def __next__(self):\n        if flag:\n            global StopIteration\n        return 1\n"
)]
fn iterator_entry_insertion_excludes_global_declaration_conflicts(#[case] source: &str) {
    assert_eq!(
        analyze_selected(
            &["iterator_stop_insert", "async_iterator_stop_insert"],
            source
        )
        .candidates
        .len(),
        0,
        "{source}"
    );
}

#[rstest::rstest]
#[case::class_it_def_next_self_doc_return_1(
    "class It:\n    def __next__(self):\n        \"doc\"; return 1\n",
    "class It:\n    def __next__(self):\n        \"doc\"; raise StopIteration(); return 1\n"
)]
#[case::class_it_def_next_self_return_1(
    "class It:\r    def __next__(self):\r        return 1\r",
    "class It:\r    def __next__(self):\r        raise StopIteration()\r        return 1\r"
)]
fn iterator_entry_insertion_supports_semicolon_docstrings_and_cr_lines(
    #[case] source: &str,
    #[case] expected: &str,
) {
    let output = analyze_selected(&["iterator_stop_insert"], source);

    assert_eq!(output.candidates.len(), 1, "{source}");
    let c = &output.candidates[0];
    let start = usize::try_from(c.span.start).unwrap();
    let end = start + usize::try_from(c.span.length).unwrap();
    let mutant = format!("{}{}{}", &source[..start], c.replacement, &source[end..]);
    assert_eq!(mutant, expected);
    assert!(parse_module(&mutant).is_ok());
}

#[test]
fn loop_entry_insertion_preserves_cr_only_tail_indentation() {
    let source = "for x in xs:\r    if flag:\r        work()\r";

    let output = analyze_selected(&["loop_break_insert"], source);

    assert_eq!(output.candidates.len(), 1);
    let c = &output.candidates[0];
    let start = usize::try_from(c.span.start).unwrap();
    let end = start + usize::try_from(c.span.length).unwrap();
    let mutant = format!("{}{}{}", &source[..start], c.replacement, &source[end..]);
    assert_eq!(
        mutant,
        "for x in xs:\r    if flag:\r        work()\r    break\r"
    );
}

#[rstest::rstest]
#[case::list_comprehension_to_generator("list_comprehension_to_generator")]
#[case::generator_return_insert("generator_return_insert")]
#[case::generator_skip_first("generator_skip_first")]
#[case::generator_empty("generator_empty")]
fn generator_mutation_operators_are_individually_opt_out(#[case] name: &str) {
    let mut operators = MutationOperatorSelection::default();

    let operator = MutationOperator::from_name(name).expect("generator mutation is registered");

    assert!(operators.contains(operator));
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[path = "generator_mutation_tests.rs"]
mod generator_mutation_tests;

#[path = "effective_python_tests.rs"]
mod effective_python_tests;
