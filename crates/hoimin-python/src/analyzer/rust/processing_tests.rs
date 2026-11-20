use super::*;
use rstest::rstest;

fn edits(source: &str, operator: &str) -> Vec<(String, String)> {
    let operators = MutationOperatorSelection::default();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 256,
    };
    analyze_source(&request, source)
        .candidates
        .into_iter()
        .filter(|candidate| candidate.operator.as_str() == operator)
        .map(|candidate| (candidate.original.clone(), candidate.replacement.clone()))
        .collect()
}

#[rstest]
#[case::filter_left_token("[x for x in xs if[x]]", "comprehension_filter_remove", "[x]", " True")]
#[case::inverted_left_token(
    "[x for x in xs if[x]]",
    "comprehension_filter_invert",
    "[x]",
    " not ([x])"
)]
#[case::filter_async_token(
    "async def f(xs): return [x for x in xs if pred(x)async for i in x]",
    "comprehension_filter_remove",
    "pred(x)",
    "True "
)]
#[case::filter_right_token(
    "[x for x in xs if pred(x)if ok(x)]",
    "comprehension_filter_remove",
    "pred(x)",
    "True "
)]
#[case::sum_destination_alias(
    "from builtins import sum as total\ndef f(sum): return len([2,4])",
    "aggregate_sum_count",
    "len",
    "total"
)]
#[case::filter_destination_module(
    "import builtins as b\nfrom itertools import filterfalse as opposite\ndef f(filter): return opposite(None, xs)",
    "filter_predicate_swap",
    "opposite",
    "b.filter"
)]
#[case::round_destination_alias(
    "from builtins import round as nearest\nfrom math import floor\ndef f(round): return floor(x)",
    "rounding_function_swap",
    "floor",
    "nearest"
)]
#[case::decimal_list_coefficient(
    "from decimal import Decimal as D\nout=D('2.45').quantize(D((0,[1],-2)))",
    "decimal_quantum_shift",
    "-2",
    "-3"
)]
#[case::decimal_list_outer(
    "from decimal import Decimal as D\nout=D('2.45').quantize(D([0,(1,),-2]))",
    "decimal_quantum_shift",
    "-2",
    "-3"
)]
#[case::decimal_list_both(
    "from decimal import Decimal as D\nout=D('2.45').quantize(D([0,[1],-2]))",
    "decimal_quantum_shift",
    "-2",
    "-3"
)]
#[case::list_filter(
    "[x for x in xs if pred(x)]",
    "comprehension_filter_remove",
    "pred(x)",
    "True"
)]
#[case::set_filter(
    "{x for x in xs if pred(x)}",
    "comprehension_filter_invert",
    "pred(x)",
    "not (pred(x))"
)]
#[case::dict_filter(
    "{x: x for x in xs if x > 0}",
    "comprehension_filter_remove",
    "x > 0",
    "True"
)]
#[case::generator_filter(
    "(x for x in xs if x == other)",
    "comprehension_filter_invert",
    "x == other",
    "not (x == other)"
)]
#[case::async_filter(
    "async def f(xs): return [x async for x in xs if x is None]",
    "comprehension_filter_remove",
    "x is None",
    "True"
)]
#[case::chained_condition(
    "[x for x in xs if x is y is z]",
    "comprehension_filter_invert",
    "x is y is z",
    "not (x is y is z)"
)]
#[case::filter_false(
    "from itertools import filterfalse as ff\nout=ff(None, xs)",
    "filter_predicate_swap",
    "ff",
    "filter"
)]
#[case::set_literal(
    "out={1, 2}.union(xs)",
    "set_union_intersection_swap",
    "union",
    "intersection"
)]
#[case::local_frozenset(
    "def f(xs):\n values=frozenset((1, 2))\n return values.intersection(xs)",
    "set_union_intersection_swap",
    "intersection",
    "union"
)]
#[case::sum_count("out=sum(xs)", "aggregate_sum_count", "xs", "1 for _ in (xs)")]
#[case::module_set(
    "items={1,2}\ndef f(xs): return items.union(xs)",
    "set_union_intersection_swap",
    "union",
    "intersection"
)]
#[case::module_decimal(
    "from decimal import Decimal\nd=Decimal('2.45')\ndef f(): return d.quantize(Decimal('.1'))",
    "decimal_quantum_shift",
    "'.1'",
    "'1e-2'"
)]
#[case::count_sum("out=len([1,-2,3])", "aggregate_sum_count", "len", "sum")]
#[case::already_imported_itertools(
    "import itertools as it\nout=filter(None, xs)",
    "filter_predicate_swap",
    "filter",
    "it.filterfalse"
)]
#[case::lexical_import_shadow_existing_alias(
    "import math as m\ndef f(__import__): return round(x)",
    "rounding_function_swap",
    "round",
    "m.ceil"
)]
#[case::already_imported_math(
    "import math as m\nout=round(x)",
    "rounding_function_swap",
    "round",
    "m.ceil"
)]
#[case::destination_alias(
    "from math import floor, ceil as upper\nout=floor(x)",
    "rounding_function_swap",
    "floor",
    "upper"
)]
#[case::destination_mode_alias(
    "from decimal import Context, ROUND_HALF_EVEN, ROUND_HALF_UP as up\nout=Context(rounding=ROUND_HALF_EVEN)",
    "decimal_rounding_mode_swap",
    "ROUND_HALF_EVEN",
    "up"
)]
#[case::floor_round(
    "from math import floor as lower\nout=lower(x)",
    "rounding_function_swap",
    "lower",
    "round"
)]
#[case::round_digits(
    "out=round(ndigits=effect(), number=x)",
    "round_digits_remove",
    "effect()",
    "(effect(), None)[1]"
)]
#[case::mode_constructor(
    "from decimal import Context, ROUND_HALF_EVEN as bankers\nout=Context(rounding=bankers)",
    "decimal_rounding_mode_swap",
    "bankers",
    "__import__('decimal').ROUND_HALF_UP"
)]
#[case::mode_quantize(
    "import decimal as d\nout=d.Decimal('2.45').quantize(d.Decimal('.1'), d.ROUND_HALF_EVEN)",
    "decimal_rounding_mode_swap",
    "ROUND_HALF_EVEN",
    "ROUND_HALF_UP"
)]
#[case::unicode_quantum(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('٠.١٠'))",
    "decimal_quantum_shift",
    "'٠.١٠'",
    "'1e-3'"
)]
#[case::tuple_quantum(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal((1,(0,1),-2)))",
    "decimal_quantum_shift",
    "-2",
    "-3"
)]
#[case::context_precision_assignment(
    "from decimal import getcontext\ngetcontext().prec=3",
    "decimal_context_precision",
    "3",
    "1"
)]
#[case::quantum(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('00.0100e1'))",
    "decimal_quantum_shift",
    "'00.0100e1'",
    "'1e-4'"
)]
#[case::positional_precision(
    "from decimal import Context\nout=Context(6)",
    "decimal_context_precision",
    "6",
    "1"
)]
#[case::scoped_precision(
    "from decimal import localcontext\nwith localcontext() as ctx:\n ctx.prec=2",
    "decimal_context_precision",
    "2",
    "4"
)]
#[case::scoped_mode(
    "import decimal as d\nwith d.localcontext() as ctx:\n ctx.rounding=d.ROUND_CEILING",
    "decimal_rounding_mode_swap",
    "ROUND_CEILING",
    "ROUND_FLOOR"
)]
fn research_processing_edits(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] original: &str,
    #[case] replacement: &str,
) {
    let expected = (original.to_owned(), replacement.to_owned());

    let actual = edits(source, operator);

    assert!(actual.contains(&expected), "{actual:?}");
}

#[rstest]
#[case::rebound_sum_alias(
    "from builtins import sum as total\ntotal = custom\ndef f(sum): return len([2,4])",
    "aggregate_sum_count"
)]
#[case::rebound_filter_alias(
    "from builtins import filter as choose\nfrom itertools import filterfalse as other\nchoose = custom\ndef f(filter): return other(None,xs)",
    "filter_predicate_swap"
)]
#[case::invalid_list_digit(
    "from decimal import Decimal as D\nout=D('2.45').quantize(D([0,[10],-2]))",
    "decimal_quantum_shift"
)]
#[case::spread_list_digit(
    "from decimal import Decimal as D\nout=D('2.45').quantize(D([0,[*xs],-2]))",
    "decimal_quantum_shift"
)]
#[case::shadowed_decimal_round(
    "from decimal import Decimal as D\ndef f(round): return (round(D('2.45'),1)*3).quantize(D('.1'))",
    "intermediate_rounding_remove"
)]
#[case::omitted_decimal_digits(
    "from decimal import Decimal as D\nout=(round(D('2.45'))*3).quantize(D('.1'))",
    "intermediate_rounding_remove"
)]
#[case::none_decimal_digits(
    "from decimal import Decimal as D\nout=(round(D('2.45'),None)*3).quantize(D('.1'))",
    "intermediate_rounding_remove"
)]
#[case::bool_remove("[x for x in xs if False]", "comprehension_filter_remove")]
#[case::bool_invert("[x for x in xs if True]", "comprehension_filter_invert")]
#[case::existing_not("[x for x in xs if not good(x)]", "comprehension_filter_invert")]
#[case::identity("[x for x in xs if x is other]", "comprehension_filter_invert")]
#[case::membership("[x for x in xs if x not in ys]", "comprehension_filter_invert")]
#[case::walrus_filter("[saved for x in xs if (saved := x)]", "comprehension_filter_remove")]
#[case::shadow_filter("def f(filter): return filter(None, xs)", "filter_predicate_swap")]
#[case::custom_set("def f(s: set): return s.union(xs)", "set_union_intersection_swap")]
#[case::conditional_set(
    "def f(flag):\n if flag: s=set()\n return s.union(xs)",
    "set_union_intersection_swap"
)]
#[case::rebound_set(
    "def f():\n s=set()\n s=custom()\n return s.union(xs)",
    "set_union_intersection_swap"
)]
#[case::module_before_initialization(
    "def f(xs): return items.union(xs)\nearly=f(())\nitems={1,2}",
    "set_union_intersection_swap"
)]
#[case::shadowed_module_receiver(
    "items={1,2}\ndef f(items): return items.union(xs)",
    "set_union_intersection_swap"
)]
#[case::start("out=sum(xs, 10)", "aggregate_sum_count")]
#[case::sum_named("out=sum((items := source()))", "aggregate_sum_count")]
#[case::sum_yield("def f(): return sum((yield xs))", "aggregate_sum_count")]
#[case::count_dynamic("out=len(xs)", "aggregate_sum_count")]
#[case::count_non_numeric("out=len([1, unknown])", "aggregate_sum_count")]
#[case::none_digits("out=round(x, None)", "round_digits_remove")]
#[case::invalid_round("out=round(x, invalid=4)", "round_digits_remove")]
#[case::lexical_import_shadow_no_destination(
    "def f(__import__): return round(x)",
    "rounding_function_swap"
)]
#[case::shadow_round("def f(round): return round(x)", "rounding_function_swap")]
#[case::mode_mutation(
    "import decimal as d\nd.ROUND_HALF_EVEN=0\nout=d.Context(rounding=d.ROUND_HALF_EVEN)",
    "decimal_rounding_mode_swap"
)]
#[case::module_escape(
    "import decimal as d\nunknown(d)\nout=d.Context(prec=5)",
    "decimal_context_precision"
)]
#[case::custom_quantize("out=value.quantize(Decimal('1'))", "decimal_quantum_shift")]
#[case::nonfinite_quantum(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('NaN'))",
    "decimal_quantum_shift"
)]
#[case::invalid_adjusted_exponent(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('10e999999999999999999'))",
    "decimal_quantum_shift"
)]
#[case::whitespace_inside_quantum(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('_ 1'))",
    "decimal_quantum_shift"
)]
#[case::float_quantum_conversion(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal(0.5))",
    "decimal_quantum_shift"
)]
#[case::invalid_quantum(
    "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('1e'))",
    "decimal_quantum_shift"
)]
#[case::ambiguous_getcontext(
    "from decimal import getcontext,setcontext,Context\nclass Custom(Context): pass\nsetcontext(Custom())\ngetcontext().prec=4",
    "decimal_context_precision"
)]
#[case::module_reflective_escape(
    "import math as m\nuse(m.__dict__)\nout=round(x)",
    "rounding_function_swap"
)]
#[case::set_nonlocal_rebind(
    "def f():\n s=set()\n def reset():\n  nonlocal s\n  s=custom()\n return s.union(xs)",
    "set_union_intersection_swap"
)]
#[case::dead_local_receiver(
    "def f():\n s=set()\n del s\n return s.union(xs)",
    "set_union_intersection_swap"
)]
#[case::invalid_context(
    "from decimal import Context\nout=Context(6, prec=5)",
    "decimal_context_precision"
)]
#[case::non_arithmetic("out=round(round(x, 2), 1)", "intermediate_rounding_remove")]
fn rejects_unproven_or_duplicate_processing_edit(#[case] source: &str, #[case] operator: &str) {
    let expected = Vec::new();

    let actual = edits(source, operator);

    assert_eq!(actual, expected);
}

#[rstest]
#[case::cross_family(
    "from decimal import Decimal as D\nout=(round(D('2.45'),1)*3).quantize(D('.1'))",
    "round(D('2.45'),1)"
)]
#[case::round("out=round(round(x, effect()) * 3, 1)", "round(x, effect())")]
#[case::decimal(
    "from decimal import Decimal\nx=Decimal('2.45')\nout=(x.quantize(Decimal('.1')) * 3).quantize(Decimal('.1'))",
    "x.quantize(Decimal('.1'))"
)]
fn selects_the_inner_rounding_site(#[case] source: &str, #[case] original: &str) {
    let expected = original;

    let actual = edits(source, "intermediate_rounding_remove");

    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].0, expected);
}

fn analyze_case(
    source: &str,
    operator: &str,
    lines: &[LineRange],
    max_candidates: usize,
) -> AnalyzerOutput {
    let operators = serde_json::from_value(serde_json::json!([operator])).unwrap();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines,
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates,
    };
    analyze_source(&request, source)
}

#[test]
fn quantum_selection_uses_the_tuple_exponent_line() {
    let source = "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal((\n 0, (1,),\n -2,\n)))\n";
    let lines = [LineRange { start: 4, end: 4 }];

    let result = analyze_case(source, "decimal_quantum_shift", &lines, 16);

    assert_eq!(result.candidates.len(), 2);
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| candidate.original == "-2")
    );
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(32))]

    #[test]
    fn one_filter_edit_keeps_other_clauses_and_the_unlimited_prefix(
        count in 2usize..12,
        keep in 1usize..8,
    ) {
        use proptest::prop_assert_eq;
        let mut source = "[x for x in xs".to_owned();
        for index in 0..count {
            use std::fmt::Write;
            write!(&mut source, "\n if check_{index}(x)").unwrap();
        }
        source.push_str("\n]");
        let operator = "comprehension_filter_invert";

        let all = analyze_case(&source, operator, &[], count + 1);
        let limited = analyze_case(&source, operator, &[], keep);

        prop_assert_eq!(all.candidates.len(), count);
        let expected: Vec<_> = all.candidates.iter().take(keep).map(|candidate| candidate.span.start).collect();
        let actual: Vec<_> = limited.candidates.iter().map(|candidate| candidate.span.start).collect();
        prop_assert_eq!(actual, expected);
        prop_assert_eq!(limited.truncated, count > keep);
        for (index, candidate) in all.candidates.iter().enumerate() {
            let original = format!("check_{index}(x)");
            let replacement = format!("not ({original})");
            prop_assert_eq!(&candidate.original, &original);
            prop_assert_eq!(&candidate.replacement, &replacement);
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            let changed = format!("{}{}{}", &source[..start], candidate.replacement, &source[end..]);
            proptest::prop_assert!(ruff_python_parser::parse_module(&changed).is_ok());
        }
    }

    #[test]
    fn decimal_quantum_changes_each_literal_scale_by_one_place(
        fraction in 0..10_000u32,
        zeros in 0..5usize,
        power in -12..13i64,
    ) {
        use proptest::prop_assert_eq;
        let source = format!(
            "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('000.{}{:04}e{power}'))",
            "0".repeat(zeros), fraction
        );
        let original_scale = power - 4 - i64::try_from(zeros).unwrap();
        let expected = [format!("'1e{}'", original_scale - 1), format!("'1e{}'", original_scale + 1)];

        let result = analyze_case(&source, "decimal_quantum_shift", &[], 16);

        let alternatives: std::collections::BTreeSet<_> =
            result.candidates.iter().map(|candidate| candidate.replacement.as_str()).collect();
        prop_assert_eq!(alternatives, expected.iter().map(String::as_str).collect());
    }
}

#[rstest]
#[case::upper("1e999999999999999999", "'1e999999999999999998'")]
#[case::lower("1e-1999999999999999997", "'1e-1999999999999999996'")]
fn quantum_shift_stays_within_decimal_constructor_bounds(
    #[case] literal: &str,
    #[case] expected: &str,
) {
    let source =
        format!("from decimal import Decimal\nout=Decimal(1).quantize(Decimal('{literal}'))");

    let result = analyze_case(&source, "decimal_quantum_shift", &[], 16);

    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].replacement, expected);
}

#[rstest]
#[case::filter("comprehension_filter_invert", "[x for x in xs if predicate(x)]")]
#[case::aggregate("aggregate_sum_count", "out=sum(xs)")]
#[case::numeric("intermediate_rounding_remove", "out=round(round(2.45,1)*3,1)")]
fn processing_candidates_use_the_strict_payload_boundary(#[case] op: &str, #[case] source: &str) {
    let operators = serde_json::from_value(serde_json::json!([op])).unwrap();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::new(0),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 4,
    };

    let result = analyze_source_cancellable(&request, source, || false);

    assert!(matches!(
        result,
        Err(super::super::AnalysisError::PayloadLimit { limit: 0 })
    ));
}

#[test]
fn unselected_decimal_literal_does_not_charge_the_payload() {
    let source = "from decimal import Decimal\nout=Decimal('2.45').quantize(Decimal('.001'))";
    let operators = serde_json::from_value(serde_json::json!(["decimal_quantum_shift"])).unwrap();
    let lines = [LineRange { start: 1, end: 1 }];
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::new(0),
        path: Utf8Path::new("subject.py"),
        lines: &lines,
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 4,
    };

    let result = analyze_source_cancellable(&request, source, || false).unwrap();

    assert!(result.candidates.is_empty());
}

#[test]
fn standalone_quantum_operator_proves_round_returning_decimal() {
    let source = "from decimal import Decimal as D\nout=round(D('2.45'),1).quantize(D('.1'))";

    let result = analyze_case(source, "decimal_quantum_shift", &[], 16);

    assert_eq!(result.candidates.len(), 2);
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| candidate.original == "'.1'")
    );
}
