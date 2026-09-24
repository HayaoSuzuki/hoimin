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
use crate::analyzer::AnalyzerDiagnosticCode;
use camino::Utf8Path;
use hoimin_core::{
    ByteSpan, LineRange, MutationOperator, MutationOperatorSelection, MutationProfile,
};
use proptest::prelude::*;
use ruff_python_parser::parse_module;
use std::fmt::Write as _;

const BINDING_FLOW_CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/binding-flow-joins.jsonl");
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
        .unwrap_or_else(|| panic!("missing Lean bounded-discovery case {id}"))
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
    assert_eq!(output.candidates[0].operator, "binary_add_sub");
    assert_eq!(output.candidates[0].original, "+");
    assert!(!output.truncated);
}

#[test]
fn selected_collection_literal_still_builds_its_candidate() {
    reset_candidate_work_stats();
    let output = analyze("data = [item]\n");

    assert_eq!(candidate_work_stats().0, 1);
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "collection_list_tuple"
            && candidate.original == "[item]"
            && candidate.replacement == "(item,)"
    }));
}

#[test]
fn line_index_answers_out_of_order_ascii_and_unicode_offsets() {
    let source = "\u{feff}aβ😀\r\nxy\u{301}z\r最後\n";
    let index = LineIndex::new(source);

    for (offset, expected) in [
        (24, (3, 2)),
        (3, (1, 0)),
        (16, (2, 3)),
        (6, (1, 2)),
        (18, (3, 0)),
        (4, (1, 1)),
    ] {
        assert_eq!(index.line_and_column(source, offset), expected);
    }
}

#[test]
fn straight_line_import_transfer_preserves_selected_candidates_and_skips_unselected() {
    for aliases in [8, 32, 128] {
        for annotations in [8, 32, 128] {
            let mut source = String::from("from typing import Sequence\n");
            for index in 0..aliases {
                writeln!(source, "import typing as t{index}").unwrap();
            }
            source.push_str(&"x: int\n".repeat(annotations));
            source.push_str("value: list[int]\n");
            for selected in [false, true] {
                super::IMPORT_CLONE_CALLS.set(0);
                super::IMPORT_CLONE_ENTRIES.set(0);
                reset_annotation_retention_stats();
                let operator = if selected {
                    MutationOperator::TypeListSequence
                } else {
                    MutationOperator::BooleanLiteral
                };
                let output = analyze_with_only_operator(&source, operator);
                assert!(output.diagnostics.is_empty());
                assert!(!output.truncated);
                assert_eq!(output.candidates.len(), usize::from(selected));
                if selected {
                    let candidate = &output.candidates[0];
                    assert_eq!(candidate.original, "list[int]");
                    assert_eq!(candidate.replacement, "Sequence[int]");
                    assert_eq!(super::IMPORT_CLONE_CALLS.get(), 1);
                    assert_eq!(super::IMPORT_CLONE_ENTRIES.get(), aliases + 1);
                    assert_eq!(annotation_retention_stats(), (annotations + 1, 0));
                } else {
                    assert_eq!(super::IMPORT_CLONE_CALLS.get(), 0);
                    assert_eq!(super::IMPORT_CLONE_ENTRIES.get(), 0);
                    assert_eq!(annotation_retention_stats(), (0, 0));
                }
            }
        }
    }
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
    assert!(unselected.candidates.is_empty());
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

#[test]
fn operator_function_pairs_cover_qualified_imported_and_higher_order_references() {
    for &(original, replacement, alias) in OPERATOR_FUNCTION_PAIRS {
        for (original, replacement) in std::iter::once((original, replacement)).chain(alias) {
            for module in ["operator", "op"] {
                let source = format!(
                    "import operator as {module}\nresult = {module}.{original}(left(), right())\n"
                );
                let output = analyze(&source);
                assert_eq!(output.candidates.len(), 1, "{source}");
                let candidate = &output.candidates[0];
                assert_eq!(candidate.operator, "operator_function");
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
                assert_eq!(candidate.operator, "operator_function");
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
}

#[test]
fn operator_function_lambdas_replace_only_callable_references() {
    for (name, replacement) in [
        (
            "contains",
            "(lambda container, item, /: item not in container)",
        ),
        (
            "__contains__",
            "(lambda container, item, /: item not in container)",
        ),
        ("setitem", "(lambda container, key, value, /: None)"),
        ("__setitem__", "(lambda container, key, value, /: None)"),
        ("delitem", "(lambda container, key, /: None)"),
        ("__delitem__", "(lambda container, key, /: None)"),
        ("call", "(lambda target, /, *args, **kwargs: None)"),
        ("__call__", "(lambda target, /, *args, **kwargs: None)"),
    ] {
        for (import, reference) in [
            ("import operator as op".to_owned(), format!("op.{name}")),
            (
                format!("from operator import {name} as action"),
                "action".to_owned(),
            ),
        ] {
            let source = format!("{import}\nresult = {reference}(*arguments(), **keywords())\n");
            let output = analyze(&source);
            assert_eq!(output.candidates.len(), 1, "{source}");
            let candidate = &output.candidates[0];
            assert_eq!(candidate.operator, "operator_function");
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
    }
}

#[test]
fn operator_function_uncertain_bindings_and_namespaces_are_excluded() {
    for alteration in [
        "op = other",
        "del op",
        "op += other",
        "op: object",
        "op, x = pair",
        "def f(op): pass",
        "def f(*op): pass",
        "def f(**op): pass",
        "def f(*, op): pass",
        "f = lambda op: op",
        "def op(): pass",
        "class op: pass",
        "type op = object",
        "def f[op](): pass",
        "class C[*op]: pass",
        "type T[**op] = object",
        "import other as op",
        "from other import value as op",
        "import operator as op",
        "if condition:\n    import operator as op",
        "def f():\n    import operator as op",
        "from other import *",
        "for op in values: pass",
        "with context() as op: pass",
        "try: pass\nexcept Exception as op: pass",
        "match value:\n    case op: pass",
        "match value:\n    case [*op]: pass",
        "match value:\n    case {'x': x, **op}: pass",
        "values = [x for op in items]",
        "values = {x for op in items}",
        "values = {x: x for op in items}",
        "values = (x for op in items)",
        "value = (op := other)",
        "def f():\n    global op",
        "def f():\n    nonlocal op",
        "op.add = other",
        "del op.add",
        "op.add += other",
        "op.add: object",
        "setattr(op, 'add', other)",
        "delattr(op, 'add')",
        "exec(code)",
        "globals()['op'] = other",
        "locals().update(values)",
        "vars(op)['add'] = other",
        "op.__dict__['add'] = other",
        "namespace = op.__dict__",
        "run = exec",
        "import operator as other\nother.sub = replacement",
        "def f():\n    import operator as other\n    other.add = replacement",
        "other = op\nother.add = replacement",
        "change(op)",
        "import builtins\nbuiltins.__import__ = replacement",
        "__builtins__['__import__'] = replacement",
    ] {
        let source = format!("import operator as op\nresult = op.add(a, b)\n{alteration}\n");
        assert!(parse_module(&source).is_ok(), "{source}");
        assert!(
            analyze(&source)
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function"),
            "{source}"
        );
    }
    for source in [
        "if condition:\n    import operator as op\nresult = op.add(a, b)\n",
        "def f():\n    import operator as op\n    return op.add(a, b)\n",
        "from .operator import add\nresult = add(a, b)\n",
        "from other import add\nresult = add(a, b)\n",
        "import other as op\nresult = op.add(a, b)\n",
        "from operator import add as plus\ndef f(plus): return plus(a, b)\n",
        "from operator import add as plus\n__import__ = custom\nresult = plus(a, b)\n",
        "from operator import add as plus\ndef f(__import__): return plus(a, b)\n",
    ] {
        assert!(
            analyze(source)
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function"),
            "{source}"
        );
    }
}

#[test]
fn operator_function_helpers_and_undocumented_dunders_are_excluded() {
    for name in [
        "attrgetter",
        "itemgetter",
        "methodcaller",
        "length_hint",
        "__truth__",
        "__is__",
        "__is_not__",
        "__is_none__",
        "__is_not_none__",
        "__countOf__",
        "__indexOf__",
        "__and___",
        "__not___",
    ] {
        let source = format!("import operator as op\nresult = op.{name}(value)\n");
        assert!(analyze(&source).candidates.is_empty(), "{source}");
    }
}

#[test]
fn operator_function_source_order_selection_profiles_and_annotations() {
    for source in [
        "result = op.add(a, b)\nimport operator as op\n",
        "result = plus(a, b)\nfrom operator import add as plus\n",
        "def f(): return plus(a, b)\nfrom operator import add as plus\n",
        "import operator as op\nx: op.add\ndef f(x: op.mul) -> op.sub: pass\ntype Alias = op.pow\n",
    ] {
        assert!(
            analyze(source)
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function"),
            "{source}"
        );
    }
    let source = "import operator as op\nfrom operator import add as plus\ndef calculate(a, b):\n    return op.add(a, b), plus(a, b)\ndef combine(values):\n    return map(op.add, values, values)\n";
    let full = analyze(source);
    assert_eq!(
        full.candidates
            .iter()
            .filter(|c| c.operator == "operator_function")
            .count(),
        3
    );
    for limit in 0..=5 {
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
            .filter(|c| c.operator == "operator_function")
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
            .all(|c| c.operator != "operator_function")
    );
}

#[test]
fn operator_function_independent_bindings_and_import_builtin_guards() {
    for source in [
        "import operator\nresult = operator.add(a, b)\n",
        "from operator import add\nresult = add(a, b)\n",
        "import operator as op\nfrom operator import add as plus\nplus = other\nresult = op.add(a, b)\n",
        "import operator as op\n__import__ = custom\nresult = op.add(a, b)\n",
        "from operator import setitem as update\n__import__ = custom\nresult = update(a, b, c)\n",
        "import operator as op\nclass C:\n    def f(self):\n        self.value = value\n        return op.add(a, b)\n",
    ] {
        assert_eq!(
            analyze(source)
                .candidates
                .iter()
                .filter(|c| c.operator == "operator_function")
                .count(),
            1,
            "{source}"
        );
    }
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

#[test]
fn operator_function_class_mangling_does_not_trust_unrelated_callables() {
    for source in [
        "from operator import add as __op\n_C__op = lambda a, b: a * b\nclass C:\n    def run(self): return __op(2, 3)\n",
        "import operator as __op\nclass Other:\n    @staticmethod\n    def add(a, b): return a * b\n_C__op = Other\nclass C:\n    def run(self): return __op.add(2, 3)\n",
        "from operator import setitem as __op\n_C__op = lambda a, b, c: c\nclass C:\n    def run(self): return __op(container, key, value)\n",
    ] {
        assert!(parse_module(source).is_ok(), "{source}");
        let output = analyze(source);
        assert!(
            output
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function"),
            "{source}"
        );
    }
}

#[test]
fn operator_function_implicit_class_bindings_do_not_resolve_to_module_imports() {
    for source in [
        "from operator import add as __class__\nclass C:\n    def __init__(self, a, b): self.value = a * b\n    def run(self): return __class__(2, 3).value\n",
        "import operator as __class__\nclass C:\n    @staticmethod\n    def add(a, b): return a * b\n    def run(self): return __class__.add(2, 3)\n",
    ] {
        assert!(parse_module(source).is_ok(), "{source}");
        let output = analyze(source);
        assert!(
            output
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function"),
            "{source}"
        );
    }
    // CPython supplies class names without corresponding AST Store nodes. The
    // conservative guard also excludes the safe __safe__ spelling in this table.
    for alias in [
        "__module__",
        "__qualname__",
        "__firstlineno__",
        "__type_params__",
        "__annotations__",
        "__doc__",
        "__static_attributes__",
        "__classcell__",
        "__classdict__",
        "__classdictcell__",
        "__annotate_func__",
        "__safe__",
    ] {
        for (import, reference) in [
            (
                format!("from operator import add as {alias}"),
                alias.to_owned(),
            ),
            (
                format!("import operator as {alias}"),
                format!("{alias}.add"),
            ),
        ] {
            let source = format!("{import}\nclass C:\n    result = {reference}(2, 3)\n");
            assert!(parse_module(&source).is_ok(), "{source}");
            assert!(
                analyze(&source)
                    .candidates
                    .iter()
                    .all(|c| c.operator != "operator_function"),
                "{source}"
            );
        }
    }
}

#[test]
fn operator_function_class_guard_preserves_independent_import_references() {
    for alias in ["__op", "__class__", "__safe__"] {
        for (import, reference) in [
            (
                format!("from operator import add as {alias}"),
                alias.to_owned(),
            ),
            (
                format!("import operator as {alias}"),
                format!("{alias}.add"),
            ),
        ] {
            let source = format!(
                "{import}\nbefore = {reference}(2, 3)\nclass C: pass\ndef run(): return {reference}(2, 3)\n"
            );
            let output = analyze(&source);
            assert_eq!(output.candidates.len(), 2, "{source}");
            for candidate in &output.candidates {
                assert_eq!(candidate.operator, "operator_function");
                apply_candidate_and_reparse(&source, candidate);
            }
        }
    }
    let source = "import operator as op\nfrom operator import add as plus\nclass C:\n    initial = op.__add__(2, 3)\n    def run(self):\n        def nested(): return plus(2, 3)\n        return op.add(2, 3)\n";
    let output = analyze(source);
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
        assert_eq!(candidate.operator, "operator_function");
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn operator_function_class_namespace_lookups_are_excluded() {
    for (import, alias, reference) in [
        ("import operator as op", "op", "op.add"),
        ("from operator import add as plus", "plus", "plus"),
    ] {
        let source = format!(
            "{import}\nclass Proxy:\n    @staticmethod\n    def add(left, right): return left * right\nclass Meta(type):\n    @classmethod\n    def __prepare__(mcls, name, bases): return {{'{alias}': Proxy}}\nclass Base(metaclass=Meta): pass\nclass Subject(Base):\n    result = {reference}(2, 3)\n    @decorate({reference})\n    def configured(self, action={reference}): return action\n    callback = lambda: {reference}(2, 3)\n    def method(self):\n        class Nested(Base):\n            result = {reference}(2, 3)\n        return {reference}(2, 3)\n"
        );
        assert!(parse_module(&source).is_ok(), "{source}");
        let output = analyze(&source);
        let candidates = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "operator_function")
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
}

#[test]
fn operator_function_class_comprehensions_use_implicit_function_scope() {
    let source = "import operator as op\nclass Subject:\n    values = [op.add(2, 3) for _ in (0,)]\n    direct = op.add(2, 3)\n    callback = lambda: op.add(2, 3)\n";
    let candidates = analyze(source)
        .candidates
        .into_iter()
        .filter(|candidate| candidate.operator == "operator_function")
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
        .filter(|candidate| candidate.operator == "operator_function")
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

#[test]
fn operator_function_class_comprehensions_preserve_identity_guards_and_first_iterable_scope() {
    let source = "import operator as op\nclass Subject:\n    values = [op.sub(3, 2) for _ in op.add((0,), (1,))]\n    nested = [[op.mul(left, right) for right in op.sub((2,), (1,))] for left in op.add((0,), (1,))]\n";
    let candidates = analyze(source)
        .candidates
        .into_iter()
        .filter(|candidate| candidate.operator == "operator_function")
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

    for excluded in [
        "from operator import add as plus\nclass Subject:\n    values = [plus(2, 3) for plus in (lambda left, right: left * right,)]\n",
        "from operator import add as __plus\nclass Subject:\n    values = [__plus(2, 3) for _ in (0,)]\n",
        "import operator as __op\nclass Subject:\n    values = [__op.add(2, 3) for _ in (0,)]\n",
    ] {
        assert!(
            analyze(excluded)
                .candidates
                .iter()
                .all(|candidate| candidate.operator != "operator_function"),
            "{excluded}"
        );
    }
}

#[test]
fn operator_function_qualified_dynamic_namespace_access_is_uncertain() {
    for alteration in [
        "import builtins as b\nb.exec(code)",
        "op.__setattr__('add', replacement)",
        "op.__delattr__('add')",
        "op.__getattribute__('__dict__')['add'] = replacement",
        "from operator import __dict__ as namespace\nnamespace['add'] = replacement",
        "from operator import __setattr__ as write\nwrite('add', replacement)",
        "from builtins import exec as execute\nexecute(code)",
        "from builtins import globals as namespace\nnamespace()['op'] = other",
        "from sys import modules as loaded\nloaded['operator'].add = replacement",
        "import builtins as b\nrun = b.exec\nrun(code)",
        "import builtins as b\nb.globals()['op'] = other",
        "__builtins__.exec(code)",
        "import sys\nsys.modules['operator'].add = replacement",
        "import sys as system\nsystem.modules['builtins'].__import__ = replacement",
    ] {
        let source = format!("import operator as op\nresult = op.add(a, b)\n{alteration}\n");
        assert!(
            analyze(&source)
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function"),
            "{source}"
        );
    }
}

#[test]
fn operator_function_lambda_replacements_exclude_pattern_values() {
    for name in ["contains", "setitem", "delitem", "call"] {
        let source = format!("import operator as op\nmatch value:\n    case op.{name}: pass\n");
        let output = analyze(&source);
        for candidate in &output.candidates {
            apply_candidate_and_reparse(&source, candidate);
        }
        assert!(
            output
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function")
        );
    }
    for source in [
        "from operator import add as plus\nmatch value:\n    case plus(): pass\n",
        "import operator as op\nmatch value:\n    case op.call(): pass\n",
    ] {
        assert!(
            analyze(source)
                .candidates
                .iter()
                .all(|c| c.operator != "operator_function"),
            "{source}"
        );
    }
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
        candidate.operator
    );
    mutated
}

fn assert_type_list_sequence_sites(source: &str, expected: &[(u64, u64, u32, Option<&str>, &str)]) {
    let output = analyze_types(source);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "type_list_sequence")
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
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

fn prefix_candidate(start: u64, replacement: &str, operator: &str) -> AnalyzerCandidate {
    AnalyzerCandidate {
        path: Utf8Path::new("pkg/sample.py").to_path_buf(),
        span: ByteSpan { start, length: 1 },
        original: "original".to_owned(),
        replacement: replacement.to_owned(),
        operator: operator.to_owned(),
        line: 1,
        column: 0,
        symbol: None,
    }
}

fn candidate_starts(candidates: &[AnalyzerCandidate]) -> Vec<u64> {
    candidates
        .iter()
        .map(|candidate| candidate.span.start)
        .collect()
}

#[test]
fn candidate_prefix_retains_the_earliest_k_plus_one_unique_candidates() {
    let mut prefix = CandidatePrefix::new(2);
    assert_eq!(prefix.capacity, 3);

    for candidate in [
        prefix_candidate(9, "nine", "operator"),
        prefix_candidate(1, "one", "operator"),
        prefix_candidate(5, "five", "operator"),
        prefix_candidate(3, "three", "operator"),
        prefix_candidate(1, "one", "operator"),
    ] {
        prefix.push(candidate);
    }

    assert_eq!(prefix.identities.len(), 3);
    let result = prefix.finish();
    assert_eq!(candidate_starts(&result.candidates), vec![1, 3, 5]);
    assert!(result.overflowed);
    assert_eq!(result.candidates.len(), 3);
}

#[test]
fn candidate_prefix_keeps_emission_order_for_equal_sort_keys() {
    let mut prefix = CandidatePrefix::new(2);
    for candidate in [
        prefix_candidate(1, "first", "operator"),
        prefix_candidate(1, "second", "operator"),
        prefix_candidate(1, "third", "operator"),
    ] {
        prefix.push(candidate);
    }

    let result = prefix.finish();
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
    let mut prefix = CandidatePrefix::new(0);
    assert_eq!(prefix.capacity, 1);
    prefix.push(prefix_candidate(3, "three", "operator"));
    prefix.push(prefix_candidate(3, "three", "operator"));
    prefix.push(prefix_candidate(1, "one", "operator"));

    let result = prefix.finish();
    assert_eq!(candidate_starts(&result.candidates), vec![1]);
    assert!(result.overflowed);
    assert_eq!(result.candidates.len(), 1);
}

#[test]
fn candidate_prefix_saturates_capacity_at_usize_maximum() {
    let prefix = CandidatePrefix::new(usize::MAX);

    assert_eq!(prefix.capacity, usize::MAX);
}

#[test]
fn candidate_prefix_matches_the_lean_out_of_order_duplicate_case() {
    let item = bounded_discovery_case("out_of_order_duplicate");
    assert_eq!(
        (item.schema, item.mode.as_str(), item.scenario.as_str()),
        (1, "internal-fixture", "candidate_prefix")
    );
    let mut prefix = CandidatePrefix::new(usize::try_from(item.limit).unwrap());
    for input in &item.token {
        assert!(input.eligible);
        assert!(matches!(input.producer.as_str(), "token" | "ast"));
        let _ = input.emission_index;
        prefix.push(prefix_candidate(
            input.order_key,
            &input.identity.to_string(),
            "operator",
        ));
    }
    let result = prefix.finish();
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
    assert_eq!(actual.operator, expected.operator);
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
    let eligibility = bounded_discovery_case("eligibility_before_capacity");
    let focused = analyze_with_profile(
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
    let output = analyze_with_profile(
        MutationProfile::Full,
        usize::try_from(zero.limit).unwrap(),
        "value = left == right\n",
    );
    assert!(output.candidates.is_empty());
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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

#[test]
fn exception_handler_type_builtin_calls_are_excluded_for_ordinary_and_starred_handlers() {
    for (source, expected) in [
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
    ] {
        let output = analyze(source);
        let collection_candidates: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "collection_list_tuple")
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
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
            .all(|candidate| candidate.operator != "exception_type_pair")
    );

    let starred = "try:\n    work()\nexcept* ValueError:\n    pass\n";
    assert!(parse_module(starred).is_ok(), "except* fixture parses");
    assert_eq!(
        analyze(starred)
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "exception_type_pair")
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
    {
        apply_candidate_and_reparse(nested_source, candidate);
    }
}

#[test]
fn except_star_exception_type_pairs_keep_shared_selection_and_profile_behavior() {
    let source =
        "def selected():\n    try:\n        work()\n    except* ValueError:\n        pass\n";
    let summarize = |output: &super::AnalyzerOutput| {
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "exception_type_pair")
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
    let full = analyze_with_profile(MutationProfile::Full, 10_000, source);
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
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

    let selected = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 4, end: 4 }],
        &["pkg.sample:selected".to_owned()],
        10_000,
        source,
    );
    assert_eq!(summarize(&selected), summarize(&full));

    let bounded = analyze_with_profile(
        MutationProfile::Full,
        1,
        "try:\n    work()\nexcept* KeyError:\n    pass\n",
    );
    assert_eq!(summarize(&bounded).len(), 1);
    assert!(bounded.truncated);

    let only_exception_pair: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!(["exception_type_pair"])).unwrap();
    let only_pair_output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &only_exception_pair,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        "try:\n    work(left == right)\nexcept* ValueError:\n    pass\n",
    );
    assert_eq!(summarize(&only_pair_output).len(), 1);
    assert_eq!(only_pair_output.candidates.len(), 1);

    let mut excluded_operators = MutationOperatorSelection::default();
    excluded_operators.exclude(MutationOperator::ExceptionTypePair);
    let excluded = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &excluded_operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert!(summarize(&excluded).is_empty());

    let arid_source = concat!(
        "if __name__ == \"__main__\":\n",
        "    try:\n",
        "        work()\n",
        "    except* ValueError:\n",
        "        pass\n",
    );
    assert_eq!(
        summarize(&analyze_with_profile(
            MutationProfile::Full,
            10_000,
            arid_source,
        ))
        .len(),
        1
    );
    assert!(
        summarize(&analyze_with_profile(
            MutationProfile::Focused,
            10_000,
            arid_source,
        ))
        .is_empty()
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
        .filter(|candidate| candidate.operator.starts_with("exception_"))
        .collect::<Vec<_>>();
    assert_eq!(exception_candidates.len(), 1);
    assert_eq!(exception_candidates[0].operator, "exception_type_pair");
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
    {
        let mutated = apply_candidate_and_reparse(source, candidate);
        assert!(mutated.contains("(message, code=code)"));
        assert!(mutated.contains("(key) from cause"));
    }
}

#[test]
fn raise_exception_type_pair_supports_every_curated_source_in_both_shapes() {
    for (source_name, expected_replacements) in [
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
    ] {
        for source in [
            format!("raise {source_name}\n"),
            format!("raise {source_name}('message')\n"),
        ] {
            let output = analyze(&source);
            let candidates = output
                .candidates
                .iter()
                .filter(|candidate| candidate.operator == "exception_type_pair")
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
            .all(|candidate| candidate.operator != "exception_type_pair")
    );
}

#[test]
fn raise_exception_type_pairs_observe_extended_resolution_boundaries() {
    let exception_lines = |source: &str| {
        analyze(source)
            .candidates
            .into_iter()
            .filter(|candidate| candidate.operator == "exception_type_pair")
            .map(|candidate| candidate.line)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        exception_lines("raise ValueError\nValueError = CustomValueError\nraise ValueError\n"),
        [1]
    );
    assert!(exception_lines("from helpers import ValueError\nraise ValueError\n").is_empty());
    assert!(exception_lines("from helpers import TypeError\nraise ValueError\n").is_empty());
    assert!(exception_lines("def source(ValueError):\n    raise ValueError\n").is_empty());
    assert!(exception_lines("def destination(TypeError):\n    raise ValueError\n").is_empty());
    assert_eq!(
        exception_lines(concat!(
            "def comprehension(errors):\n",
            "    captured = [TypeError for TypeError in errors]\n",
            "    raise ValueError\n",
        )),
        [3]
    );
    assert!(exception_lines("from helpers import *\nraise ValueError\n").is_empty());
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(exception_pairs, [("ValueError", "TypeError")]);

    let ordinary_pairs = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator == "collection_any_all")
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(ordinary_pairs, [("any", "all"), ("all", "any")]);
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
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
        .filter(|candidate| candidate.operator == "exception_type_pair")
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
            .filter(|candidate| candidate.operator == "exception_type_pair")
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
            path: Utf8Path::new("pkg/sample.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );
    assert!(
        excluded
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "exception_type_pair")
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
        .filter(|candidate| candidate.operator.starts_with("exception_"))
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
        candidate.operator == "exception_bare_to_exception"
            && candidate.original == "except"
            && candidate.replacement == "except Exception"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_exception_to_bare"
            && candidate.original == "Exception"
            && candidate.replacement.is_empty()
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_base_boundary"
            && candidate.original == "Exception"
            && candidate.replacement == "BaseException"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_base_boundary"
            && candidate.original == "BaseException"
            && candidate.replacement == "Exception"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_tuple_add_pair"
            && candidate.original == "(ValueError,)"
            && candidate.replacement == "(ValueError, TypeError)"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_tuple_remove_member"
            && candidate.original == "(ValueError, TypeError,)"
    }));
    assert!(output.candidates.iter().any(|candidate| {
        candidate.operator == "exception_tuple_remove_member"
            && candidate.original.contains("# keep this comment")
    }));
    for candidate in &output.candidates {
        if candidate.operator.starts_with("exception_") {
            let mutated = apply_candidate_and_reparse(source, candidate);
            if candidate.original.contains("# keep this comment") {
                assert!(mutated.contains("# keep this comment"));
            }
        }
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
    assert!(
        analyze_source(
            &AnalyzeRequest {
                path: Utf8Path::new("pkg/sample.py"),
                lines: &[],
                symbols: &[],
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 10_000,
            },
            unsupported,
        )
        .candidates
        .iter()
        .all(|candidate| !candidate.operator.starts_with("exception_"))
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
            .all(|candidate| !candidate.operator.starts_with("exception_")),
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
            .filter(|candidate| candidate.operator == "exception_exception_to_bare")
            .count(),
        1
    );

    for termination in ["SystemExit", "KeyboardInterrupt", "GeneratorExit"] {
        let source = format!("try:\n    work()\nexcept ({termination}, ValueError):\n    pass\n");
        let output = analyze_source(
            &AnalyzeRequest {
                path: Utf8Path::new("pkg/sample.py"),
                lines: &[],
                symbols: &[],
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 10_000,
            },
            &source,
        );
        for candidate in output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "exception_tuple_remove_member")
        {
            let mutated = apply_candidate_and_reparse(&source, candidate);
            assert!(
                !mutated.contains(&format!("except ({termination}")),
                "tuple removal generated an individual termination handler: {mutated}"
            );
        }
    }
}

#[test]
fn annotations_do_not_emit_default_bitwise_mutations() {
    let output = analyze("value: Left | None\nresult = left & right\n");

    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "bitwise_and_or")
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

#[test]
fn type_alias_and_match_capture_bindings_shadow_collection_builtins() {
    for source in [
        "type list = int\nresult = list(items)\n",
        "match value:\n    case list:\n        pass\nresult = list(items)\n",
    ] {
        let output = analyze(source);
        assert!(
            output
                .candidates
                .iter()
                .all(|candidate| candidate.operator != "collection_list_tuple"),
            "unexpected list candidate for {source:?}: {:#?}",
            output.candidates
        );
    }
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
        candidate.operator.starts_with("collection_")
            || candidate.operator.starts_with("structure_")
    }) {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn annotations_and_star_imports_suppress_collection_candidates() {
    let annotation_output = analyze(
        "from typing import Callable\nhandler: Callable[[Left, Right], Result]\nvalue: tuple[Left, Right]\n",
    );
    assert!(
        annotation_output
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
    );

    let star_import_output =
        analyze("from helpers import *\nresult = list(items)\nvalue = tuple(items)\n");
    assert!(
        star_import_output
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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
            .all(|candidate| candidate.operator != "collection_list_tuple"),
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
            .all(|candidate| candidate.operator != "structure_sorted_reversed"),
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
            .all(|candidate| candidate.operator != "structure_sorted_reversed"),
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
            .all(|candidate| candidate.operator != "structure_sorted_reversed"),
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
            .filter(|candidate| candidate.operator == "structure_sorted_reversed")
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
            .filter(|candidate| candidate.operator == "structure_sorted_reversed")
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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
    assert!(
        wildcard
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "collection_list_tuple")
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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
            .all(|candidate| candidate.operator != "collection_list_tuple")
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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
        .filter(|candidate| candidate.operator == "collection_list_tuple")
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
            .all(|candidate| candidate.operator != "collection_list_tuple")
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
        .filter(|candidate| candidate.operator.starts_with("collection_"))
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
        .filter(|candidate| candidate.operator == "collection_string_split_rsplit")
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

#[test]
fn tuple_to_list_preserves_parenthesized_boundary_elements_in_unparenthesized_tuples() {
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

    for (source, original, replacement) in cases {
        let output = analyze(source);
        let candidates: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "collection_list_tuple")
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
            .all(|candidate| !candidate.operator.starts_with("collection_")),
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
        .filter(|candidate| candidate.operator.starts_with("structure_"))
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
        .filter(|candidate| candidate.operator.starts_with("structure_"))
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
        .filter(|candidate| candidate.operator.starts_with("structure_"))
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
fn structure_excludes_unsupported_shapes_and_receivers() {
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
    let output = analyze(source);

    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| !candidate.operator.starts_with("structure_")),
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
            .filter(|candidate| candidate.operator == operator)
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
        .filter(|candidate| candidate.operator == "bitwise_and_or")
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
        .filter(|candidate| candidate.operator == "bitwise_shift")
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
        .filter(|candidate| candidate.operator == "structure_index_neighbor")
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
        .filter(|candidate| candidate.operator == "structure_slice_neighbor")
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
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn candidate_replacements_reparse_as_python() {
    let source = "result = left == right\n";
    let output = analyze(source);
    let candidate = output
        .candidates
        .iter()
        .find(|candidate| candidate.operator == "compare_eq_ne")
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

#[test]
fn native_python_operator_syntax_uses_exact_ast_roles_and_reparses() {
    for (source, operator, original, replacement) in [
        (
            "def calculate(a, b):\n    return a ** b\n",
            "binary_power",
            "**",
            "*",
        ),
        (
            "def calculate(a, b):\n    return a @ b\n",
            "binary_matmul",
            "@",
            "*",
        ),
        (
            "def calculate(a, b):\n    return a ^ b\n",
            "bitwise_xor",
            "^",
            "&",
        ),
        (
            "def calculate(a):\n    return ~a\n",
            "bitwise_invert",
            "~",
            "+",
        ),
        (
            "def calculate(a, b):\n    a **= b\n    return a\n",
            "augmented_power",
            "**=",
            "*=",
        ),
        (
            "def calculate(a, b):\n    a @= b\n    return a\n",
            "augmented_matmul",
            "@=",
            "*=",
        ),
        (
            "def calculate(a, b):\n    a &= b\n    return a\n",
            "augmented_bitwise_and_or",
            "&=",
            "|=",
        ),
        (
            "def calculate(a, b):\n    a |= b\n    return a\n",
            "augmented_bitwise_and_or",
            "|=",
            "&=",
        ),
        (
            "def calculate(a, b):\n    a ^= b\n    return a\n",
            "augmented_bitwise_xor",
            "^=",
            "&=",
        ),
        (
            "def calculate(a, b):\n    a <<= b\n    return a\n",
            "augmented_bitwise_shift",
            "<<=",
            ">>=",
        ),
        (
            "def calculate(a, b):\n    a >>= b\n    return a\n",
            "augmented_bitwise_shift",
            ">>=",
            "<<=",
        ),
    ] {
        let operators: MutationOperatorSelection =
            serde_json::from_value(serde_json::json!([operator])).unwrap();
        let output = analyze_source(
            &AnalyzeRequest {
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
        assert_eq!(candidate.operator, operator);
        assert_eq!(candidate.original, original);
        assert_eq!(candidate.replacement, replacement);
        let start = usize::try_from(candidate.span.start).unwrap();
        let end = start + usize::try_from(candidate.span.length).unwrap();
        assert_eq!(&source[start..end], original);
        apply_candidate_and_reparse(source, candidate);
    }
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
    assert_eq!(selected.candidates[0].operator, "binary_power");
    assert_eq!(selected.candidates[0].symbol.as_deref(), Some("selected"));

    let bounded = analyze_with(Utf8Path::new("pkg/sample.py"), &[], &[], 1, source);
    assert_eq!(bounded.candidates.len(), 1);
    assert_eq!(bounded.candidates[0].operator, "binary_power");
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
    let output = analyze(source);
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
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
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
        .filter(|candidate| candidate.operator == "boolean_literal")
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
            ("True", "False", "boolean_literal", 4, Some("selected")),
        ]
    );
}

#[test]
fn composite_comparisons_preserve_multiline_trivia_and_reparse() {
    for (source, operator, span, original, replacement, mutated) in [
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
    ] {
        let candidate = analyze(source)
            .candidates
            .into_iter()
            .find(|candidate| candidate.operator == operator)
            .expect("composite comparison candidate");

        assert_eq!(candidate.span, span);
        assert_eq!(candidate.original, original);
        assert_eq!(candidate.replacement, replacement);
        assert_eq!(apply_candidate_and_reparse(source, &candidate), mutated);
    }
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
    for name in operators.names() {
        let operator = MutationOperatorSelection::parse_selector(&name).unwrap()[0];
        if operator != MutationOperator::BooleanLiteral {
            operators.exclude(operator);
        }
    }
    for (shape, source) in [("single", single), ("multiline", multiline)] {
        let started = std::time::Instant::now();
        let output = analyze_source(
            &AnalyzeRequest {
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
        assert_eq!(output.candidates[0].operator, "boolean_literal");
        println!(
            "shape={shape} source_bytes={} discovered={} retained={} elapsed_ms={}",
            source.len(),
            COUNT,
            output.candidates.len(),
            elapsed.as_secs_f64() * 1_000.0
        );
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
        let comparisons_per_query = if index.facts == 0 {
            0
        } else {
            usize::BITS as usize - index.facts.leading_zeros() as usize
        };
        assert!(
            index.comparisons <= index.queries * comparisons_per_query,
            "stats={index:?}",
        );
    }
    assert_eq!(output.candidates.len(), FUNCTIONS * 5);
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
        Some(("+", "binary_add_sub", Some("scope_0511"))),
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

#[test]
fn line_index_reports_one_based_lines_and_unicode_scalar_columns() {
    let source = "alpha\nβeta\r\n終 = left == right\n";
    let line_index = LineIndex::new(source);

    for (offset, expected) in [
        (0, (1, 0)),
        (6, (2, 0)),
        (8, (2, 1)),
        (11, (2, 4)),
        (24, (3, 9)),
    ] {
        assert_eq!(
            line_index.line_and_column(source, offset),
            expected,
            "position at byte offset {offset}",
        );
    }
}

#[test]
fn line_index_ignores_only_a_leading_file_bom_in_columns() {
    let source = "\u{feff}ab\n\u{feff}c\n";
    let line_index = LineIndex::new(source);

    for (offset, expected) in [
        (0, (1, 0)),
        (3, (1, 0)),
        (5, (1, 2)),
        (6, (2, 0)),
        (9, (2, 1)),
        (10, (2, 2)),
    ] {
        assert_eq!(
            line_index.line_and_column(source, offset),
            expected,
            "position at byte offset {offset}",
        );
    }
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
    let source = format!("{}result = 1 + 2\n", "print(True)\n".repeat(100));
    let focused = analyze_with_profile(MutationProfile::Focused, 1, &source);

    assert_eq!(focused.candidates.len(), 1);
    assert_eq!(focused.candidates[0].line, 101);
    assert_eq!(focused.candidates[0].operator, "binary_add_sub");
    assert!(!focused.truncated);
    assert!(focused.diagnostics.is_empty());
    assert_eq!(focused.retention.producer_peaks[0], 1);
}

#[test]
fn bounded_collection_with_zero_limit_keeps_only_an_overflow_probe() {
    let output = analyze_with(Utf8Path::new("pkg/zero.py"), &[], &[], 0, "value = 1 + 2\n");

    assert!(output.candidates.is_empty());
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
    let source = "if __name__ == \"__main__\":\n    print(1 + 2)\n    assert 3 == 3\nelse:\n    fallback = 4 + 5\n\ndef f(flag=True, *, enabled=False):\n    return flag + enabled\n";
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
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

#[test]
fn focused_profile_accepts_only_exact_main_guard_shapes() {
    let source = "if \"__main__\" == __name__:\n    reversed = 1 + 2\nif __name__ != \"__main__\":\n    inequality = 3 + 4\nif __name__ == \"__main__\" == \"__main__\":\n    chained = 5 + 6\nif __name__ == \"entry\":\n    entry = 7 + 8\n";
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
    assert!(
        focused
            .candidates
            .iter()
            .all(|candidate| candidate.line != 2)
    );
    for line in [3, 4, 5, 6, 7, 8] {
        assert!(
            focused
                .candidates
                .iter()
                .any(|candidate| candidate.line == line),
            "expected an eligible candidate on line {line}",
        );
    }
}

#[test]
fn focused_profile_suppresses_only_bare_print_and_assert() {
    let source = "print(1 + 2)\nlogger.print(3 + 4)\nassert 5 + 6\nregular = 7 + 8\n";
    let focused = analyze_with_profile(MutationProfile::Focused, 10_000, source);
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
            .any(|candidate| candidate.operator.starts_with("type_"))
    );
    assert!(
        focused
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "boolean_literal")
    );
}

#[test]
fn focused_filter_runs_before_candidate_limit() {
    let focused = analyze_with_profile(
        MutationProfile::Focused,
        1,
        "def choose(enabled=True):\n    return 1 + 2\n",
    );
    assert_eq!(focused.candidates.len(), 1);
    assert_eq!(focused.candidates[0].line, 2);
    assert_eq!(focused.candidates[0].operator, "binary_add_sub");
}

#[test]
fn focused_profile_applies_after_line_and_symbol_selection() {
    let source = "def selected(enabled=True):\n    return 1 + 2\n\ndef ignored(enabled=True):\n    return 3 + 4\n";
    let lines = vec![LineRange { start: 2, end: 2 }];
    let symbols = vec!["pkg.sample:selected".to_owned()];
    let operators = MutationOperatorSelection::default();
    let focused = analyze_source(
        &AnalyzeRequest {
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
    assert_eq!(descriptors, vec![(2, "binary_add_sub")]);
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
    let output = analyze(&source);
    assert!(output.diagnostics.is_empty());
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

#[test]
fn ordinary_string_literals_ignore_every_mutable_operator_token() {
    assert!(
        analyze("label = 'ordinary operator-free text'\n")
            .candidates
            .is_empty()
    );
    for token in MUTABLE_OPERATOR_TOKENS {
        let source = format!("label = {token:?}\n");
        assert!(
            analyze(&source).candidates.is_empty(),
            "ordinary string content {token:?} produced a candidate"
        );
        assert_only_trailing_expression_changes(&source);
    }
}

#[test]
fn nested_quote_pairs_preserve_every_surrounding_string_byte() {
    for (name, benign, adversarial) in NESTED_QUOTE_PAIRS {
        for source in [benign, adversarial] {
            assert!(
                analyze(source).candidates.is_empty(),
                "nested quote fixture {name} produced a literal-content candidate"
            );
            assert_only_trailing_expression_changes(source);
        }
    }
}

#[test]
fn interpolated_string_pairs_skip_literals_and_retain_expressions() {
    for (flavor, literal, expression) in [
        (
            "f-string",
            "value = f\"literal == + True {plain}\"\n",
            "value = f\"literal == + True {left + right} {enabled is not None}\"\n",
        ),
        (
            "t-string",
            "value = t\"literal == + True {plain}\"\n",
            "value = t\"literal == + True {left + right} {enabled is not None}\"\n",
        ),
    ] {
        assert!(
            analyze(literal).candidates.is_empty(),
            "{flavor} literal content produced a candidate"
        );
        let observed = analyze(expression)
            .candidates
            .into_iter()
            .map(|candidate| {
                (
                    candidate.original,
                    candidate.replacement,
                    candidate.operator,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            [
                ("+".to_owned(), "-".to_owned(), "binary_add_sub".to_owned()),
                ("is not".to_owned(), "is".to_owned(), "identity".to_owned()),
            ],
            "{flavor} interpolation expressions were not analyzed exactly"
        );
    }
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
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
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
        .filter(|candidate| candidate.operator == "type_list_sequence")
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
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
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
        .filter(|candidate| candidate.operator == "type_list_sequence")
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
fn nullable_removal_preserves_multiline_annotation_syntax() {
    for (name, source, expected) in [
        (
            "Optional grouped union",
            "from typing import Optional\nx: Optional[(int\n | str)]\n",
            "(int\n | str)",
        ),
        (
            "None leading grouped union",
            "x: None | (int\n | str)\n",
            "(int\n | str)",
        ),
        (
            "None trailing grouped union",
            "x: (int\n | str) | None\n",
            "(int\n | str)",
        ),
        (
            "None trailing ungrouped multiline union",
            "x: (int\n | str\n | None)\n",
            "(int\n | str)",
        ),
        (
            "parameter Optional grouped union",
            "from typing import Optional\ndef f(value: Optional[(int\n | str)]):\n    pass\n",
            "(int\n | str)",
        ),
        (
            "return Optional grouped union",
            "from typing import Optional\ndef f() -> Optional[(int\n | str)]:\n    pass\n",
            "(int\n | str)",
        ),
        (
            "Optional union comments",
            "from typing import Optional\nx: Optional[\n    # first\n    int\n    | str  # second\n]\n",
            "(\n    # first\n    int\n    | str  # second\n)",
        ),
        (
            "Optional grouped union surrounding whitespace",
            "from typing import Optional\nx: Optional[\n    (int | str)\n]\n",
            "(\n    (int | str)\n)",
        ),
        (
            "None trailing hash string literal",
            "def resolve(value):\n    return int\nx: resolve(\"#\") | None\n",
            "resolve(\"#\")",
        ),
        (
            "Optional hash string literal",
            "from typing import Optional\ndef resolve(value):\n    return int\nx: Optional[resolve(\"#\")]\n",
            "resolve(\"#\")",
        ),
    ] {
        let output = analyze_types(source);
        let candidates = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_nullable_remove")
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 1, "{name}");
        assert_eq!(candidates[0].replacement, expected, "{name}");
        apply_candidate_and_reparse(source, candidates[0]);
    }
}

#[test]
fn typing_import_rebinding_linear() {
    for (name, source, expected) in [
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
    ] {
        let output = analyze_types(source);
        let actual: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_list_sequence")
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
        for candidate in &output.candidates {
            apply_candidate_and_reparse(source, candidate);
        }
    }
}

#[test]
fn typing_module_alias_rebinding_linear() {
    for (name, source, expected) in [
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
    ] {
        let output = analyze_types(source);
        let actual: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_list_sequence")
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
        for candidate in &output.candidates {
            apply_candidate_and_reparse(source, candidate);
        }
    }
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
        .filter(|candidate| candidate.operator == "type_list_sequence")
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
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }

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
        .filter(|candidate| candidate.operator == "type_list_sequence")
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

#[test]
#[allow(clippy::too_many_lines)]
fn typing_import_rebinding_control_flow() {
    for (name, source, expected) in [
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
            vec![(243, 11, None, "Sequence[str]")],
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
            vec![
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
            vec![(261, 11, None, "Sequence[str]")],
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
            vec![(471, 22, None, "Sequence[str]")],
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
            vec![(265, 12, None, "Sequence[str]")],
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
            vec![(412, 17, None, "Sequence[str]")],
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
            vec![(151, 6, None, "Sequence[str]")],
        ),
    ] {
        let output = analyze_types(source);
        let actual: Vec<_> = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_list_sequence")
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
        for candidate in &output.candidates {
            apply_candidate_and_reparse(source, candidate);
        }
    }
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
        let item = items
            .iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("missing internal-fixture case {id}"));
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
        if let Some(expected_loop_head) = &item.expected_loop_head {
            assert_eq!(
                binding_flow_loop_head_snapshot(&item.source, true),
                *expected_loop_head,
                "loop head case={id}"
            );
        }
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
fn qualified_annotation_spelling_uses_the_matching_provider_member() {
    let mut imports = super::KnownImports::default();
    imports
        .modules
        .insert("abc".into(), "collections.abc".into());
    imports.modules.insert("t".into(), "typing".into());
    imports
        .modules
        .insert("collections".into(), "collections".into());
    let targets = &["typing.AbstractSet", "collections.abc.Set"];
    for (source, expected) in [
        ("abc.Sequence", "abc.Set"),
        ("t.Sequence", "t.AbstractSet"),
        ("collections.abc.Sequence", "collections.abc.Set"),
    ] {
        assert_eq!(
            imports.spelling_for(source, targets).as_deref(),
            Some(expected)
        );
    }
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
            .any(|candidate| candidate.line == 9 && candidate.operator == "membership")
    );
    assert!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 10 && candidate.operator == "binary_mul_div")
    );
    assert!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 11 && candidate.operator == "bitwise_and_or")
    );
    assert!(
        !output
            .candidates
            .iter()
            .any(|candidate| candidate.line == 12 && candidate.operator == "membership")
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| { candidate.line == 13 && candidate.operator == "binary_mul_div" })
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
            prop_assert_eq!(&source[start..end], candidate.original);
            previous_start = start;
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
fn reports_columns_without_counting_a_leading_file_bom() {
    let source = "\u{feff}x = 1 + 2\n";
    let candidate = analyze(source)
        .candidates
        .into_iter()
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
    assert!(output.candidates.is_empty());
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
        let snapshot = name_resolution_test_snapshot(source, marker, "list")
            .unwrap_or_else(|error| panic!("{error}; case={case}"));
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
    for item in cases {
        assert_eq!(item.schema, 1, "{}", item.id);
        assert!(!item.scenario.is_empty(), "{}", item.id);
        assert!(item.expected_operator.is_none(), "{}", item.id);
        assert!(item.expected_original.is_none(), "{}", item.id);
        assert!(item.expected_replacement.is_none(), "{}", item.id);
        match item.observation_kind.as_str() {
            "annotation" => {
                let snapshot = annotation_site_test_snapshot(&item.source, &item.marker)
                    .unwrap_or_else(|error| panic!("{error}; case={}", item.id));
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
            "resolution" => {
                let snapshot = name_resolution_test_snapshot(&item.source, &item.marker, "list")
                    .unwrap_or_else(|error| panic!("{error}; case={}", item.id));
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
            other => panic!("infrastructure-error: unexpected private kind {other}"),
        }
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

    for item in cases {
        assert_eq!(item.schema, 1, "{}", item.id);
        assert!(matches!(item.family.as_str(), "handler" | "match-case"));
        assert!(!item.name.is_empty(), "{}", item.id);
        assert!(item.operator.is_none(), "{}", item.id);
        assert!(item.original.is_none(), "{}", item.id);
        assert!(item.replacement.is_none(), "{}", item.id);
        assert!(item.symbol.is_none(), "{}", item.id);
        match item.observation_kind.as_str() {
            "annotation" => {
                assert!(item.expected_resolution.is_none(), "{}", item.id);
                assert!(item.expected_exit_category.is_none(), "{}", item.id);
                let actual_facts = if item.id == "handler_type_before_target" {
                    binding_flow_marker_snapshot(&item.source, &item.marker)
                        .unwrap_or_else(|error| panic!("{error}; case={}", item.id))
                } else {
                    annotation_site_test_snapshot(&item.source, &item.marker)
                        .unwrap_or_else(|error| panic!("{error}; case={}", item.id))
                        .facts
                };
                assert_eq!(actual_facts, item.expected_facts, "{}", item.id);
                assert_eq!(
                    !actual_facts.is_empty(),
                    item.expected_present,
                    "{}",
                    item.id
                );
            }
            "exits" => {
                assert!(item.expected_resolution.is_none(), "{}", item.id);
                let category = match item.expected_exit_category.as_deref() {
                    Some(category @ ("fallthrough" | "break" | "continue" | "terminate")) => {
                        category
                    }
                    category => panic!("infrastructure-error: invalid exit {category:?}"),
                };
                let actual_facts =
                    binding_flow_handler_exit_snapshot(&item.source, &item.marker, category)
                        .unwrap_or_else(|error| panic!("{error}; case={}", item.id));
                assert_eq!(actual_facts, item.expected_facts, "{}", item.id);
                assert_eq!(
                    !actual_facts.is_empty(),
                    item.expected_present,
                    "{}",
                    item.id
                );
            }
            other => panic!("infrastructure-error: unexpected private kind {other}"),
        }
    }
}

#[test]
fn exception_match_binding_private_projection_detects_broken_transitions() {
    let cases = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .collect::<Vec<_>>();
    let case = |id: &str| {
        cases
            .iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("missing sensitivity case {id}"))
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

#[test]
fn exception_match_binding_handler_cleanup_rows_observe_post_cleanup_state() {
    let cases = EXCEPTION_MATCH_BINDING_CORPUS
        .lines()
        .map(|line| serde_json::from_str::<ExceptionMatchBindingCorpusCase>(line).unwrap())
        .filter(|item| item.observation_kind == "exits")
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 5);

    for item in cases {
        let category = item
            .expected_exit_category
            .as_deref()
            .expect("handler cleanup category");
        let mutation = match category {
            "fallthrough" => BindingFlowTestMutation::OmitHandlerFallthroughCleanup,
            "break" => BindingFlowTestMutation::OmitHandlerBreakCleanup,
            "continue" => BindingFlowTestMutation::OmitHandlerContinueCleanup,
            "terminate" => BindingFlowTestMutation::OmitHandlerTerminateCleanup,
            other => panic!("unexpected handler cleanup category {other}"),
        };
        assert_eq!(
            binding_flow_handler_exit_snapshot(&item.source, &item.marker, category)
                .unwrap_or_else(|error| panic!("{}: {error}", item.id)),
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
            .unwrap_or_else(|error| panic!("{}: {error}", item.id)),
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

#[test]
fn parenthesized_exception_removals_use_complete_expression_spans() {
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
    for (expression, operator, expected) in cases {
        let source = format!("try:\n    work()\nexcept {expression}:\n    pass\n");
        let mut operators = MutationOperatorSelection::default();
        operators.include(operator);
        let output = analyze_source(
            &AnalyzeRequest {
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
            .filter(|candidate| candidate.operator == operator.as_str())
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
}

#[test]
fn python_physical_lines_select_candidates_after_all_newline_forms() {
    for newline in ["\r", "\r\n", "\n"] {
        for final_newline in ["", newline] {
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
                .filter(|c| c.operator == "binary_add_sub")
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
    }
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

#[test]
fn comprehension_named_binding_execution_facts_and_rhs_order() {
    for (source, marker, expected) in [
        (
            "[(list := 0) for _ in []]\nlist(values)\n",
            "list(values)",
            "unknown",
        ),
        (
            "((list := 0) for _ in [0])\nlist(values)\n",
            "list(values)",
            "unknown",
        ),
        (
            "def f():\n    list(values)\n    [(list := 0) for _ in []]\n",
            "list(values)",
            "shadowed",
        ),
        (
            "list(values)\n[(list := 0) for _ in [0]]\n",
            "list(values)",
            "definitely-builtin",
        ),
        (
            "(list := list(values))\n",
            "list(values)",
            "definitely-builtin",
        ),
        (
            "[list(values) for list in [custom]]\n",
            "list(values)",
            "shadowed",
        ),
        (
            "[(unused := list(values)) for list in [custom]]\n",
            "list(values)",
            "shadowed",
        ),
        (
            "for _ in [0, 1]:\n    list(values)\n    [(list := custom) for _ in [0]]\n",
            "list(values)",
            "unknown",
        ),
    ] {
        let snapshot = name_resolution_test_snapshot(source, marker, "list").unwrap();
        assert_eq!(snapshot.resolution, expected, "{source}");
    }
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

#[test]
fn structure_negative_neighbors_cover_positions_spelling_and_range_limits() {
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
    for (expression, expected) in cases {
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

#[test]
fn implicit_finally_preserves_nested_and_deferred_boundaries() {
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
    for (label, source, expected) in cases {
        let actual = analyze_with_only_operator(source, MutationOperator::TypeListSequence);
        assert_eq!(actual.candidates.len(), expected, "{label}\n{source}");
    }
}

#[test]
fn implicit_finally_covers_operator_and_statement_exception_entries() {
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
    let mut mismatches = Vec::new();
    for (label, body) in bodies {
        let source = format!(
            "Sequence = set\ntry:\n    {body}\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n"
        );
        if !analyze_with_only_operator(&source, MutationOperator::TypeListSequence)
            .candidates
            .is_empty()
        {
            mismatches.push(label);
        }
    }
    for (label, source) in [
        (
            "class global",
            "from typing import Sequence\ntry:\n    class C:\n        global Sequence\n        Sequence = set\n        hazard()\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n",
        ),
        (
            "class nonlocal",
            "def f():\n    from typing import Sequence\n    try:\n        class C:\n            nonlocal Sequence\n            Sequence = set\n            hazard()\n        from typing import Sequence\n    finally:\n        value: Sequence[int]\n",
        ),
    ] {
        if !analyze_with_only_operator(source, MutationOperator::TypeListSequence)
            .candidates
            .is_empty()
        {
            mismatches.push(label);
        }
    }
    assert!(
        mismatches.is_empty(),
        "missed implicit exception entries: {mismatches:?}"
    );
}

#[test]
fn implicit_finally_covers_name_container_format_and_partial_binding_failures() {
    let bodies = [
        ("name load", "missing"),
        ("set hash", "{[]}"),
        ("dict hash", "{[]: 1}"),
        ("format protocol", "f'{value}'"),
        ("starred expansion", "[*values]"),
        ("unpack", "first, second = values"),
    ];
    let mut mismatches = Vec::new();
    for (label, body) in bodies {
        let source = format!(
            "Sequence = set\ntry:\n    {body}\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n"
        );
        if !analyze_with_only_operator(&source, MutationOperator::TypeListSequence)
            .candidates
            .is_empty()
        {
            mismatches.push(label);
        }
    }
    for (label, source) in [
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
    ] {
        if !analyze_with_only_operator(source, MutationOperator::TypeListSequence)
            .candidates
            .is_empty()
        {
            mismatches.push(label);
        }
    }
    assert!(
        mismatches.is_empty(),
        "missed ordinary exceptions: {mismatches:?}"
    );
}

#[test]
fn implicit_finally_covers_pattern_capture_before_guard_failure() {
    let source = "from typing import Sequence\ntry:\n    match subject:\n        case Sequence if hazard():\n            pass\n    from typing import Sequence\nfinally:\n    value: Sequence[int]\n";
    assert!(
        analyze_with_only_operator(source, MutationOperator::TypeListSequence)
            .candidates
            .is_empty()
    );
}

#[test]
fn with_suppression_joins_only_reachable_exception_states() {
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
    for (label, source, expected) in cases {
        let actual = analyze_with_only_operator(source, MutationOperator::TypeListSequence);
        assert_eq!(actual.candidates.len(), expected, "{label}\n{source}");
    }
}

#[test]
fn with_suppression_preserves_nested_flow_and_deferred_boundaries() {
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
    for (label, source, expected) in cases {
        let actual = analyze_with_only_operator(source, MutationOperator::TypeListSequence);
        assert_eq!(actual.candidates.len(), expected, "{label}\n{source}");
    }
}

#[test]
fn with_suppression_preserves_successful_abrupt_categories() {
    for abrupt in ["return", "break", "continue"] {
        let source = format!("from typing import Sequence\nwith manager:\n    {abrupt}\n");
        let actual = binding_flow_test_snapshot(&source);
        assert!(actual.fallthrough.is_empty(), "{abrupt}: {actual:?}");
        let states = match abrupt {
            "return" => actual.terminates,
            "break" => actual.breaks,
            _ => actual.continues,
        };
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
}

#[test]
fn with_suppression_makes_bare_raise_successor_reachable() {
    for body in [
        "    raise\n",
        "    try:\n        raise\n    finally:\n        pass\n",
    ] {
        let source = format!("from typing import Sequence\nwith manager:\n{body}");
        let actual = binding_flow_test_snapshot(&source);
        assert_eq!(
            actual.fallthrough,
            vec![vec!["direct:Sequence=typing.Sequence".to_owned()]],
            "{source}"
        );
        assert_eq!(actual.terminates.len(), 1, "possible propagation: {source}");
    }
}

#[test]
fn evaluation_order_preserves_only_pre_binding_builtin_lookups() {
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
    for (label, body, expected) in cases {
        let source = format!(
            "def custom(values): return 'custom'\ndef sink(*args, **kwargs): return kwargs\nslots = {{False: 0}}\n{body}\n"
        );
        let output = analyze(&source);
        let candidates = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "collection_any_all")
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), expected, "{label}: {candidates:#?}");
    }
}

#[test]
fn evaluation_order_applies_to_other_builtin_pairs_and_class_suites() {
    for (operator, source, destination) in [
        ("collection_list_tuple", "list", "tuple"),
        ("structure_sorted_reversed", "sorted", "reversed"),
    ] {
        let positive = analyze(&format!("{destination}, other = {source}([]), 7\n"));
        assert_eq!(
            positive
                .candidates
                .iter()
                .filter(|candidate| candidate.operator == operator && candidate.original == source)
                .count(),
            1
        );
        for binding in [source, destination] {
            for body in [
                format!("{binding}, slots[{source}([])] = custom, 7"),
                format!("slots[{source}([])] = ({binding} := custom)"),
                format!("sink(flag={source}([]), *[({binding} := custom)])"),
            ] {
                for indent in [false, true] {
                    let source = if indent {
                        format!("class Example:\n    {body}\n")
                    } else {
                        format!("{body}\n")
                    };
                    let output = analyze(&source);
                    assert!(
                        output
                            .candidates
                            .iter()
                            .all(|candidate| candidate.operator != operator
                                || !matches!(candidate.original.as_str(), "list" | "sorted")),
                        "{source}: {:#?}",
                        output.candidates
                    );
                }
            }
        }
    }
}

#[test]
fn evaluation_order_retains_lookups_before_later_deletions() {
    for body in [
        "del slots[any([])], all",
        "del (slots[any([])], all)",
        "def any(value=any([])):\n    pass",
        "class any:\n    value = any([])",
    ] {
        let output = analyze(body);
        let candidates = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "collection_any_all")
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 1, "{body}: {candidates:#?}");
    }
}

#[test]
fn nullable_builtin_provenance_rejects_shadowing_and_retains_builtins() {
    for (name, annotation) in [
        ("str", "str"),
        ("int", "int"),
        ("float", "float"),
        ("bool", "bool"),
        ("bytes", "bytes"),
        ("list", "list[int]"),
        ("set", "set[int]"),
        ("dict", "dict[str, int]"),
    ] {
        for (source, expected) in [
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
        ] {
            let output = analyze_types(&source);
            let candidates = output
                .candidates
                .iter()
                .filter(|candidate| candidate.operator == "type_nullable_add")
                .collect::<Vec<_>>();
            assert_eq!(candidates.len(), expected, "{source}: {candidates:#?}");
        }
    }
}

#[test]
fn nullable_builtin_provenance_checks_nested_arguments_and_preserves_import_aliases() {
    for (source, expected) in [
        ("int = custom\nvalue: list[int]\n", 0),
        ("str = custom\nvalue: dict[str, int]\n", 0),
        ("float = custom\nvalue: list[dict[str, float]]\n", 0),
        ("list = custom\nvalue: dict[str, list[int]]\n", 0),
        (
            "from typing import Sequence\nint = custom\nvalue: Sequence[int]\n",
            0,
        ),
        ("from typing import Sequence as list\nvalue: list[int]\n", 1),
        (
            "from typing import Mapping as dict\nvalue: dict[str, int]\n",
            1,
        ),
        (
            "class Owner:\n    int = custom\n    def method(value: int): pass\n",
            0,
        ),
        (
            "class Owner:\n    int = custom\n    def method():\n        value: int\n",
            1,
        ),
        ("def function(int):\n    value: int\n", 0),
        ("def function(int: int) -> int: pass\n", 2),
        ("def function():\n    global int\n    value: int\n", 1),
        (
            "int = custom\ndef function():\n    global int\n    value: int\n",
            0,
        ),
        (
            "def outer():\n    int = custom\n    def inner():\n        nonlocal int\n        value: int\n",
            0,
        ),
        (
            "class Outer:\n    int = custom\n    class Inner:\n        value: int\n",
            1,
        ),
        ("class Owner[int]:\n    def method(value: int): pass\n", 0),
        (
            "class Owner:\n    int = custom\n    def method[T](value: int): pass\n",
            0,
        ),
        (
            "from typing import Sequence as list\nint = custom\nvalue: list[int]\n",
            0,
        ),
    ] {
        let output = analyze_types(source);
        let candidates = output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_nullable_add")
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), expected, "{source}: {candidates:#?}");
    }
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
            .filter(|candidate| candidate.operator == "type_nullable_remove")
            .count(),
        2
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == "type_list_sequence")
            .count(),
        1
    );
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator != "type_nullable_add")
    );
}

#[test]
fn nullable_builtin_provenance_checks_starred_and_list_arguments() {
    for annotation in ["dict[*(str, int)]", "dict[*[str, int]]", "list[[int]]"] {
        for (prefix, expected) in [("", 1), ("int = custom\n", 0)] {
            let source = format!("{prefix}value: {annotation}\n");
            let output = analyze_types(&source);
            let candidates = output
                .candidates
                .iter()
                .filter(|candidate| candidate.operator == "type_nullable_add")
                .collect::<Vec<_>>();
            assert_eq!(candidates.len(), expected, "{source}: {candidates:#?}");
        }
    }
}
