use camino::Utf8Path;
use hoimin_core::{LineRange, MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{
    AnalyzeRequest, AnalyzerOutput, analyze_source_cancellable, payload::PayloadBudget,
};
use proptest::prelude::*;
use rstest::rstest;

fn analyze(
    source: &str,
    operator: MutationOperator,
    limit: usize,
    lines: &[LineRange],
) -> AnalyzerOutput {
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        operators.exclude(MutationOperator::from_name(&name).unwrap());
    }
    operators.include(operator);
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines,
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: limit,
    };
    analyze_source_cancellable(&request, source, || false).unwrap()
}

fn replacements(source: &str, operator: MutationOperator, limit: usize) -> Vec<String> {
    analyze(source, operator, limit, &[])
        .candidates
        .into_iter()
        .map(|candidate| {
            assert_eq!(candidate.operator, operator);
            let start = usize::try_from(candidate.span.start).unwrap();
            let length = usize::try_from(candidate.span.length).unwrap();
            let mut edited = source.to_owned();
            edited.replace_range(start..start + length, &candidate.replacement);
            edited
        })
        .collect()
}

#[rstest]
#[case::index(
    MutationOperator::StructureIndexNeighbor,
    "values[\n    compute_index()\n]",
    2,
    2
)]
#[case::slice_upper_bound(
    MutationOperator::StructureSliceNeighbor,
    "values[\n    lower():\n    upper():\n    step()\n]",
    3,
    2
)]
#[case::receiver_line_alone(
    MutationOperator::StructureIndexNeighbor,
    "values[\n    compute_index()\n]",
    1,
    0
)]
fn computed_neighbors_select_the_operand_line_and_replace_the_full_subscript(
    #[case] operator: MutationOperator,
    #[case] subscript: &str,
    #[case] line: u32,
    #[case] count: usize,
) {
    let source = format!("out = {subscript}\n");
    let lines = [LineRange {
        start: line,
        end: line,
    }];

    let output = analyze(&source, operator, 10, &lines);

    assert_eq!(output.candidates.len(), count);
    for candidate in output.candidates {
        assert_eq!(candidate.operator, operator);
        assert_eq!(candidate.original, subscript);
        assert_eq!((candidate.line, candidate.column), (1, 6));
        let start = usize::try_from(candidate.span.start).unwrap();
        let length = usize::try_from(candidate.span.length).unwrap();
        assert_eq!(&source[start..start + length], subscript);
        let mut edited = source.clone();
        edited.replace_range(start..start + length, &candidate.replacement);
        assert!(
            ruff_python_parser::parse_module(&edited).is_ok(),
            "{edited}"
        );
    }
}

#[rstest]
#[case::zip_omitted(MutationOperator::ZipStrictToggle, "out = zip(*iterables,)\n", 1)]
#[case::zip_generator(MutationOperator::ZipStrictToggle, "out = zip(x for x in data)\n", 1)]
#[case::zip_computed(
    MutationOperator::ZipStrictToggle,
    "out = zip(*iterables, strict=flag())\n",
    1
)]
#[case::zip_literal(
    MutationOperator::ZipStrictToggle,
    "out = zip(data, strict=False)\n",
    0
)]
#[case::zip_keyword_unpack(MutationOperator::ZipStrictToggle, "out = zip(data, **options)\n", 0)]
#[case::zip_local(MutationOperator::ZipStrictToggle, "def f(zip): return zip(data)\n", 0)]
#[case::zip_alias(
    MutationOperator::ZipStrictToggle,
    "from builtins import zip as paired\nout = paired(data)\n",
    1
)]
#[case::range_bound(MutationOperator::RangeBoundNeighbor, "out = range(read_limit())\n", 2)]
#[case::range_bounds_step(
    MutationOperator::RangeBoundNeighbor,
    "out = range(start(), stop(), step())\n",
    6
)]
#[case::range_unpack(MutationOperator::RangeBoundNeighbor, "out = range(*bounds)\n", 0)]
#[case::range_local(
    MutationOperator::RangeBoundNeighbor,
    "def f(range): return range(end)\n",
    0
)]
#[case::range_alias(
    MutationOperator::RangeBoundNeighbor,
    "import builtins as bi\nout = bi.range(end)\n",
    2
)]
#[case::computed_index(
    MutationOperator::StructureIndexNeighbor,
    "out = values[1 + offset()]\n",
    2
)]
#[case::computed_index_no_trusted_range(
    MutationOperator::StructureIndexNeighbor,
    "def f(range): return values[offset()]\n",
    0
)]
#[case::computed_index_trusted_qualified_helpers(
    MutationOperator::StructureIndexNeighbor,
    "import builtins as bi\ndef f(range, type, int): return values[offset()]\n",
    2
)]
#[case::computed_index_prepared_class_shadows_qualified_helpers(
    MutationOperator::StructureIndexNeighbor,
    "import builtins as bi\nclass Meta(type):\n    @classmethod\n    def __prepare__(cls, name, bases): return {'bi': None}\nclass Subject(metaclass=Meta):\n    out = [0, 1, 2][1 + 0]\n",
    0
)]
#[case::computed_index_method_uses_module_helpers_bypassing_prepared_class(
    MutationOperator::StructureIndexNeighbor,
    "import builtins as bi\nclass Meta(type):\n    @classmethod\n    def __prepare__(cls, name, bases): return {'bi': None}\nclass Subject(metaclass=Meta):\n    def lookup(self, range, type, int): return [0, 1, 2][1 + 0]\n",
    2
)]
#[case::literal_index_no_helpers_needed(
    MutationOperator::StructureIndexNeighbor,
    "def f(range, type, int): return values[1]\n",
    2
)]
#[case::comparison_in_annotation(MutationOperator::CompareDirection, "value: 1 < 2\n", 0)]
#[case::literal_string_key(MutationOperator::StructureIndexNeighbor, "out = values['name']\n", 0)]
#[case::computed_slice(
    MutationOperator::StructureSliceNeighbor,
    "out = values[start():stop:-stride()]\n",
    6
)]
#[case::compare_chain(
    MutationOperator::CompareDirection,
    "out = (a < b) <= f() is not c > d\n",
    3
)]
#[case::tuple_calls(
    MutationOperator::ReturnTupleSwap,
    "def f(): return left(), middle(), right()\n",
    2
)]
#[case::boolean_value(
    MutationOperator::ConditionClauseDelete,
    "out = (x := first()) or second() or third()\n",
    3
)]
#[case::while_test(
    MutationOperator::ConditionClauseDelete,
    "while check() and (x := read()): pass\n",
    2
)]
fn selected_operator_emits_independently_parseable_candidates(
    #[case] operator: MutationOperator,
    #[case] source: &str,
    #[case] count: usize,
) {
    let variants = replacements(source, operator, 20);

    assert_eq!(variants.len(), count, "{variants:?}");
    for variant in variants {
        assert!(
            ruff_python_parser::parse_module(&variant).is_ok(),
            "{variant}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn tuple_swap_candidates_preserve_the_bounded_prefix(
        size in 3..14_usize,
        budget in 1..8_usize,
    ) {
        let elements = (0..size).map(|index| format!("arg({index})"))
            .collect::<Vec<_>>().join(", ");
        let source = format!("def f(): return {elements}\n");

        let all = replacements(&source, MutationOperator::ReturnTupleSwap, size);
        let bounded = replacements(&source, MutationOperator::ReturnTupleSwap, budget);

        prop_assert_eq!(all.len(), size - 1);
        prop_assert_eq!(bounded.as_slice(), &all[..(size - 1).min(budget)]);
    }
}
