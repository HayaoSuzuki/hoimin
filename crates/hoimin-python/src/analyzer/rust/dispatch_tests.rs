use super::super::{
    AnalyzeRequest, MutationOperatorSelection, MutationProfile, Utf8Path, analyze_source,
};
use rstest::rstest;

fn edits(source: &str, operator: &str) -> Vec<String> {
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([operator])).unwrap();
    let result = analyze_source(
        &AnalyzeRequest {
            payload: crate::payload::PayloadBudget::default(),
            path: Utf8Path::new("dispatch.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 256,
        },
        source,
    );
    result
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == operator)
        .map(|candidate| {
            let mut mutated = source.to_owned();
            let start = usize::try_from(candidate.span.start).unwrap();
            let length = usize::try_from(candidate.span.length).unwrap();
            mutated.replace_range(start..start + length, &candidate.replacement);
            ruff_python_parser::parse_module(&mutated)
                .unwrap_or_else(|error| panic!("{error}: {mutated}"));
            mutated
        })
        .collect()
}

#[rstest]
#[case::handlers(
    "exception_handler_swap",
    "try: fail()\nexcept LookupError: broad()\nexcept KeyError: narrow()\nexcept: fallback()\n",
    "try: fail()\nexcept KeyError: narrow()\nexcept LookupError: broad()\nexcept: fallback()\n"
)]
#[case::star_handlers(
    "exception_handler_swap",
    "try: fail()\nexcept* LookupError: broad()\nexcept* KeyError: narrow()\n",
    "try: fail()\nexcept* KeyError: narrow()\nexcept* LookupError: broad()\n"
)]
#[case::explicit_none(
    "exception_cause_omit",
    "raise (ValueError()) from (None)\n",
    "raise (ValueError())\n"
)]
#[case::complex_cause(
    "exception_cause_omit",
    "raise ValueError from (\n    make_cause(code()) # inner\n) # outside\n",
    "raise ValueError; (\n    make_cause(code()) # inner\n) # outside\n"
)]
#[case::named_binding(
    "exception_cause_omit",
    "def run():\n raise ValueError from (cause := make_cause())\n",
    "def run():\n raise ValueError; (cause := make_cause())\n"
)]
#[case::generator_shape(
    "exception_cause_omit",
    "def run():\n raise ValueError from (yield reason)\n",
    "def run():\n raise ValueError; (yield reason)\n"
)]
#[case::guarded_case(
    "match_case_swap",
    "match value:\n case y if ready: use(y)\n case [z]: use(z)\n case _: fallback()\n",
    "match value:\n case [z]: use(z)\n case y if ready: use(y)\n case _: fallback()\n"
)]
#[case::unrelated_global(
    "match_case_swap",
    "def run(value):\n match value:\n  case 1: global counter\n  case 2: other = 3\n",
    "def run(value):\n match value:\n  case 2: other = 3\n  case 1: global counter\n"
)]
#[case::sequence(
    "match_pattern_wildcard",
    "match value:\n case [1 as x, y]: use(x, y)\n",
    "match value:\n case [_ as x, y]: use(x, y)\n"
)]
#[case::qualified_pattern_retains_read(
    "match_pattern_wildcard",
    "match subject:\n case [K.value]: use()\n",
    "match subject:\n case [_] if True or K: use()\n"
)]
#[case::qualified_pattern_retains_guard(
    "match_pattern_wildcard",
    "match subject:\n case [K.value] if check := predicate(): use(check)\n",
    "match subject:\n case [_] if (check := predicate()) if True else K: use(check)\n"
)]
#[case::mapping(
    "match_pattern_wildcard",
    "match value:\n case {'key': 1 | 2, **rest}: use(rest)\n",
    "match value:\n case {'key': _, **rest}: use(rest)\n"
)]
#[case::class_keyword(
    "match_pattern_wildcard",
    "match value:\n case C(x, point=(1, 2)): use(x)\n",
    "match value:\n case C(x, point=_): use(x)\n"
)]
#[case::class_position(
    "match_class_position_swap",
    "match value:\n case C(x, 2, y, flag=True): use(x, y)\n",
    "match value:\n case C(y, 2, x, flag=True): use(x, y)\n"
)]
#[case::star_boundary(
    "exception_base_boundary",
    "try: fail()\nexcept* Exception: handle()\n",
    "try: fail()\nexcept* BaseException: handle()\n"
)]
#[case::star_tuple_add(
    "exception_tuple_add_pair",
    "try: fail()\nexcept* (ValueError,): handle()\n",
    "try: fail()\nexcept* (ValueError, TypeError): handle()\n"
)]
#[case::star_tuple_remove(
    "exception_tuple_remove_member",
    "try: fail()\nexcept* (ValueError, TypeError): handle()\n",
    "try: fail()\nexcept* (ValueError ): handle()\n"
)]
#[case::star_unparenthesized_add(
    "exception_tuple_add_pair",
    "try: fail()\nexcept* LookupError, ValueError: handle()\n",
    "try: fail()\nexcept* LookupError, ValueError, TypeError: handle()\n"
)]
#[case::star_singleton_remove(
    "exception_tuple_remove_member",
    "try: fail()\nexcept* (ValueError,): handle()\n",
    "try: fail()\nexcept* (): handle()\n"
)]
fn emits_valid_dispatch_mutation(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] expected: &str,
) {
    let mutants = edits(source, operator);

    assert!(
        mutants.iter().any(|mutant| mutant == expected),
        "{mutants:?}"
    );
}

#[rstest]
#[case::bare_handler(
    "exception_handler_swap",
    "try: fail()\nexcept ValueError: recover()\nexcept: fallback()\n"
)]
#[case::terminal_capture(
    "match_case_swap",
    "match value:\n case 1: first()\n case otherwise: use(otherwise)\n"
)]
#[case::capture_preserved("match_pattern_wildcard", "match value:\n case [C(x)]: use(x)\n")]
#[case::global_order(
    "match_case_swap",
    "def run(value):\n match value:\n  case 1: global counter\n  case 2: counter = 3\n"
)]
#[case::group_type(
    "exception_tuple_remove_member",
    "try: fail()\nexcept* (ValueError, ExceptionGroup): handle()\n"
)]
#[case::shadowed_tuple_member(
    "exception_tuple_add_pair",
    "LookupError = Custom\ntry: fail()\nexcept* LookupError, ValueError: handle()\n"
)]
#[case::star_import_tuple_member(
    "exception_tuple_add_pair",
    "from helpers import *\ntry: fail()\nexcept* LookupError, ValueError: handle()\n"
)]
#[case::dynamic_tuple_member(
    "exception_tuple_add_pair",
    "exec(code)\ntry: fail()\nexcept* LookupError, ValueError: handle()\n"
)]
fn rejects_unsafe_dispatch_mutation(#[case] operator: &str, #[case] source: &str) {
    let mutants = edits(source, operator);

    assert!(mutants.is_empty(), "{mutants:?}");
}

#[rstest]
#[case::one_level_available(198, 1)]
#[case::no_level_available(199, 0)]
fn wildcard_preserving_guard_respects_delimiter_limit(
    #[case] groups: usize,
    #[case] expected: usize,
) {
    let source = format!(
        "match value:\n case [K.x] if check({}arg{}): use()\n",
        "(".repeat(groups),
        ")".repeat(groups)
    );

    let mutants = edits(&source, "match_pattern_wildcard");

    assert_eq!(mutants.len(), expected);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(32))]
    #[test]
    fn grouped_class_positions_swap_without_moving_the_keyword(
        left in 0u8..32,
        right in 32u8..64,
        grouped in proptest::bool::ANY,
    ) {
        let left = format!("[{left}, x]");
        let right = format!("{{'key': {right} as y}}");
        let (open, close) = if grouped { ("(", ")") } else { ("", "") };
        let source = format!(
            "match subject:\n case C({open}{left}{close}, {right}, keep=flag): use(x, y, flag)\n"
        );
        let expected = format!(
            "match subject:\n case C({right}, {open}{left}{close}, keep=flag): use(x, y, flag)\n"
        );

        let mutants = edits(&source, "match_class_position_swap");

        proptest::prop_assert_eq!(mutants, [expected]);
    }
}
