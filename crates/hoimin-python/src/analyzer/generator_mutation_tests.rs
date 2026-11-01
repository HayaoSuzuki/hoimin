use super::{
    AnalyzeRequest, AnalyzerCandidate, LineRange, MutationProfile, analyze_selected,
    analyze_selected_with, analyze_source_cancellable, parse_module,
};
use camino::Utf8Path;

const OPERATORS: &[&str] = &[
    "list_comprehension_to_generator",
    "generator_return_insert",
    "generator_skip_first",
    "generator_empty",
];

fn apply(source: &str, candidate: &AnalyzerCandidate) -> String {
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[start..end], candidate.original);
    let result = format!(
        "{}{}{}",
        &source[..start],
        candidate.replacement,
        &source[end..]
    );
    assert!(parse_module(&result).is_ok(), "{result}");
    result
}

#[rstest::rstest]
#[case::result_x_2_for_x_in_items_if_x(
    "result = [x * 2 for x in items if x]\n",
    "result = (x * 2 for x in items if x)\n"
)]
#[case::result_x_for_x_in("result = [x for x in []]\n", "result = (x for x in [])\n")]
#[case::result_x_for_x_in_items(
    "result = ([x for x in items])\n",
    "result = ((x for x in items))\n"
)]
#[case::async_def_f_items_return_x_async_for_x_in_items(
    "async def f(items):\n    return [x async for x in items]\n",
    "async def f(items):\n    return (x async for x in items)\n"
)]
fn list_comprehension_becomes_a_generator_with_original_interior(
    #[case] source: &str,
    #[case] expected: &str,
) {
    let output = analyze_selected(&["list_comprehension_to_generator"], source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(apply(source, &output.candidates[0]), expected);
}

#[rstest::rstest]
#[case::def_values_doc_yield_1(
    "def values():\n    \"doc\"\n    yield 1\n",
    "def values():\n    \"doc\"\n    return\n    yield 1\n"
)]
#[case::async_def_values_yield_1(
    "async def values(): yield 1\n",
    "async def values(): return; yield 1\n"
)]
#[case::def_values_doc_yield_1_3(
    "def values():\n    \"doc\"; yield 1\n",
    "def values():\n    \"doc\"; return; yield 1\n"
)]
#[case::decorate_def_values_yield_1(
    "@decorate\ndef values(): \\\n    yield 1\n",
    "@decorate\ndef values(): \\\n    return; yield 1\n"
)]
#[case::def_values_def_inner_value_yield_1_pass(
    "def values():\n    def inner(value=(yield 1)): pass\n",
    "def values():\n    return\n    def inner(value=(yield 1)): pass\n"
)]
#[case::def_values_yield_1("def values():\r    yield 1", "def values():\r    return\r    yield 1")]
fn generator_return_retains_yield_docstrings_and_async_kind_table_1(
    #[case] source: &str,
    #[case] expected: &str,
) {
    let output = analyze_selected(&["generator_return_insert"], source);

    assert_eq!(output.candidates.len(), 1, "{source}");
    assert_eq!(apply(source, &output.candidates[0]), expected);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("values"));
}

#[test]
fn generator_return_retains_yield_docstrings_and_async_kind() {
    let source = "def outer():\n    def inner(): yield 1\n    return inner\n";

    let output = analyze_selected(&["generator_return_insert"], source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("outer.inner"));
    assert_eq!(
        analyze_selected(
            &["generator_return_insert"],
            "def values(): return 0; yield 1\n"
        )
        .candidates
        .len(),
        0
    );
}

#[rstest::rstest]
#[case::result_consume_x_for_x_in_items("result = consume(x for x in items)\n")]
#[case::result_for_in_items("result = (値 for 値 in items)\n")]
#[case::result_x_comment_for_x_in_items("result = (x # comment\r\n          for x in items)\r\n")]
#[case::async_def_f_return_x_for_x_in_await_items(
    "async def f():\n    return (x for x in await items())\n"
)]
fn generator_wrappers_support_implicit_arguments_and_known_empty_exclusions_table_1(
    #[case] source: &str,
) {
    let output = analyze_selected(&["generator_skip_first", "generator_empty"], source);

    assert_eq!(output.candidates.len(), 2, "{source}");
    for candidate in &output.candidates {
        apply(source, candidate);
    }
}

#[rstest::rstest]
#[case::result_x_for_x_in("result = (x for x in ())\n")]
#[case::result_x_for_x_in_2("result = (x for x in [])\n")]
fn generator_wrappers_support_implicit_arguments_and_known_empty_exclusions_table_2(
    #[case] source: &str,
) {
    assert_eq!(
        analyze_selected(&["generator_skip_first", "generator_empty"], source)
            .candidates
            .len(),
        0
    );
}

#[rstest::rstest]
#[case::result_consume_list_xs_for_list_in_factories(
    "result = consume(list(xs) for list in factories)\n"
)]
#[case::result_consume_next_x_for_next_in_functions(
    "result = consume(next(x) for next in functions)\n"
)]
fn next_proof_uses_parent_scope_after_construction_without_name_collision_table_1(
    #[case] source: &str,
) {
    let output = analyze_selected(&["generator_skip_first", "collection_list_tuple"], source);

    assert_eq!(output.candidates.len(), 1, "{source}");
    assert_eq!(
        output.candidates[0].operator.as_str(),
        "generator_skip_first"
    );
    apply(source, &output.candidates[0]);
}

#[test]
fn next_proof_preserves_shadowed_outer_comprehension_binding() {
    let source = "result = consume(x for next in callbacks for x in (y for y in values))\n";

    let output = analyze_selected(&["generator_skip_first"], source);

    assert_eq!(output.candidates.len(), 1);
    assert!(
        output.candidates[0]
            .original
            .contains("for next in callbacks")
    );
}

#[rstest::rstest]
#[case::next_custom_result_x_for_x_in_items("next = custom\nresult = (x for x in items)\n")]
#[case::custom_result_x_for_x_in_items("ｎｅｘｔ = custom\nresult = (x for x in items)\n")]
#[case::result_next_x_for_x_in_items("result = ((next := x) for x in items)\n")]
#[case::def_f_next_items_return_x_for_x_in_items("def f(next, items): return (x for x in items)\n")]
#[case::def_f_next_items_return_x_for_x_in_items_5(
    "def f[next](items): return (x for x in items)\n"
)]
#[case::from_provider_import_result_x_for_x_in_items(
    "from provider import *\nresult = (x for x in items)\n"
)]
#[case::exec_code_result_x_for_x_in_items("exec(code)\nresult = (x for x in items)\n")]
#[case::for_item_in_rounds_result_x_for_x_in_values_next_custom(
    "for item in rounds:\n    result = (x for x in values)\n    next = custom\n"
)]
fn next_proof_uses_parent_scope_after_construction_without_name_collision(#[case] source: &str) {
    assert_eq!(
        analyze_selected(&["generator_skip_first"], source)
            .candidates
            .len(),
        0,
        "{source}"
    );
}

#[rstest::rstest]
#[case::async_def_f_items_return_x_async_for_x_in_items(
    "async def f(items): return (x async for x in items)\n"
)]
#[case::async_def_f_items_return_await_convert_x_for_x_in_items(
    "async def f(items): return (await convert(x) for x in items)\n"
)]
#[case::async_def_f_items_values_return_x_async_for_x_in_items_for_y_in_v(
    "async def f(items, values): return ([x async for x in items] for y in values)\n"
)]
fn wrappers_distinguish_nested_async_frames_table_1(#[case] source: &str) {
    assert_eq!(
        analyze_selected(&["generator_skip_first", "generator_empty"], source)
            .candidates
            .len(),
        0,
        "{source}"
    );
}

#[test]
fn wrappers_distinguish_nested_async_frames() {
    let source = "async def f(items, values): return ((x async for x in items) for y in values)\n";

    let output = analyze_selected(&["generator_skip_first", "generator_empty"], source);

    assert_eq!(output.candidates.len(), 2);
    for candidate in &output.candidates {
        apply(source, candidate);
    }
}

#[rstest::rstest]
#[case::zero(0)]
#[case::limit_1(1)]
#[case::limit_2(2)]
#[case::limit_3(3)]
#[case::at_capacity(4)]
fn generator_candidates_preserve_every_prefix_selection_and_payload_limit_prefix_boundary(
    #[case] cap: usize,
) {
    let source = "def values(items):\n    yield [x for x in items]\n    yield (x for x in items)\n";
    let all = analyze_selected(OPERATORS, source);

    assert_eq!(all.candidates.len(), 4);

    let prefix = analyze_selected_with(
        OPERATORS,
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        cap,
        source,
    );
    assert_eq!(prefix.candidates, all.candidates[..cap]);
    assert_eq!(prefix.truncated, cap < all.candidates.len());
}

proptest::proptest! {
    #[test]
    fn generator_candidates_preserve_every_prefix_selection_and_payload_limit_generated_prefix(cap in 0usize..128) {
    let source = "def values(items):\n    yield [x for x in items]\n    yield (x for x in items)\n";
    let all = analyze_selected(OPERATORS, source);

    assert_eq!(all.candidates.len(), 4);

    let prefix = analyze_selected_with(
        OPERATORS,
        Utf8Path::new("pkg/sample.py"),
        &[],
        &[],
        cap,
        source,
    );
    proptest::prop_assert_eq!(&prefix.candidates[..], &all.candidates[..cap.min(all.candidates.len())]);
    proptest::prop_assert_eq!(prefix.truncated, cap < all.candidates.len());

    }
}

#[test]
fn generator_candidates_preserve_every_prefix_selection_and_payload_limit() {
    let source = "def values(items):\n    yield [x for x in items]\n    yield (x for x in items)\n";
    let all = analyze_selected(OPERATORS, source);

    assert_eq!(all.candidates.len(), 4);

    let chosen = analyze_selected_with(
        OPERATORS,
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 3, end: 3 }],
        &["values".to_owned()],
        10,
        source,
    );
    assert_eq!(chosen.candidates.len(), 2);
    let operators = serde_json::from_value(serde_json::json!(OPERATORS)).unwrap();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::new(1),
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
