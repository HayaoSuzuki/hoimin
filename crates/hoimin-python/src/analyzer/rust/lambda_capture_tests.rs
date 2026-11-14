use super::{analyze, analyze_selected, analyze_selected_with, apply_candidate_and_reparse};

#[test]
fn lambda_capture_late_is_default_for_matching_loop_binding() {
    let source = "def build(values):\n callbacks=[]\n for value in values:\n  callbacks.append(lambda value=value: value)\n return callbacks\n";

    let output = analyze(source);
    let rows: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "lambda_capture_late")
        .collect();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].original, "lambda value=value: value");
    apply_candidate_and_reparse(source, rows[0]);
}

#[rstest::rstest]
#[case::value("value")]
#[case::hoimin_get("_hoimin_get")]
#[case::hoimin_missing("_hoimin_missing")]
#[case::value("値")]
fn lambda_capture_preserves_grouping_utf8_comments_and_hygiene(#[case] name: &str) {
    let source = format!(
        "def build(values):\n for {name} in values:\n  callback = (lambda {name}=( # captured\n   {name} # default\n  ): ( # body\n   {name}\n  ))\n"
    );

    let output = analyze_selected(&["lambda_capture_late"], &source);

    assert_eq!(output.candidates.len(), 1, "{source}");
    let changed = apply_candidate_and_reparse(&source, &output.candidates[0]);
    for comment in ["# captured", "# default", "# body"] {
        assert!(changed.contains(comment), "{changed}");
    }
    if name == "_hoimin_get" {
        assert!(changed.contains("lambda _hoimin_get_, _hoimin_missing:"));
    }
    if name == "_hoimin_missing" {
        assert!(changed.contains("lambda _hoimin_get, _hoimin_missing_:"));
    }
}

#[rstest::rstest]
#[case::callback_lambda_value_value_value("callback=lambda value=value:value\n")]
#[case::for_other_in_values_callback_lambda_value_value_value(
    "for other in values:\n callback=lambda value=value:value\n"
)]
#[case::for_value_in_values_callback_lambda_value_other_value(
    "for value in values:\n callback=lambda value=other:value\n"
)]
#[case::for_value_in_values_callback_lambda_value_value_consume_value(
    "for value in values:\n callback=lambda value=value:consume(value)\n"
)]
#[case::for_value_in_values_callback_lambda_value_value_other_1_value(
    "for value in values:\n callback=lambda value=value,other=1:value\n"
)]
#[case::for_value_in_values_callback_lambda_value_value_value(
    "for value in values:\n callback=lambda value=value,/:value\n"
)]
#[case::for_value_in_values_callback_lambda_value_value_value_7(
    "for value in values:\n callback=lambda *,value=value:value\n"
)]
#[case::for_value_in_values_callback_lambda_value_value_args_value(
    "for value in values:\n callback=lambda value=value,*args:value\n"
)]
#[case::for_value_in_values_def_nested_return_lambda_value_value_value(
    "for value in values:\n def nested():\n  return lambda value=value:value\n"
)]
#[case::for_value_in_values_callback_lambda_lambda_value_value_value(
    "for value in values:\n callback=lambda:lambda value=value:value\n"
)]
#[case::for_value_in_values_callbacks_lambda_value_value_value_for_value(
    "for value in values:\n callbacks=[lambda value=value:value for value in values]\n"
)]
#[case::for_value_in_values_pass_else_callback_lambda_value_value_value(
    "for value in values:\n pass\nelse:\n callback=lambda value=value:value\n"
)]
#[case::async_def_build_values_async_for_value_in_values_callback_lambda(
    "async def build(values):\n async for value in values:\n  callback=lambda value=value:value\n"
)]
#[case::for_value_in_values_for_value_other_in_pairs_callback_lambda_valu(
    "for value in values:\n for value,other in pairs:\n  callback=lambda value=value:value\n"
)]
#[case::for_value_in_values_annotation_lambda_value_value_value(
    "for value in values:\n annotation: (lambda value=value:value)\n"
)]
#[case::for_value_in_values_type_alias_lambda_value_value_value(
    "for value in values:\n type Alias = lambda value=value:value\n"
)]
fn lambda_capture_rejects_nonmatching_scopes_and_shapes(#[case] source: &str) {
    let output = analyze_selected(&["lambda_capture_late"], source);

    assert!(
        output.candidates.is_empty(),
        "{source}: {:?}",
        output.candidates
    );
}

const OUTER_LOOP_CAPTURES: &str = "def build(values):\n for value in values:\n  first=lambda value=value:value\n  for other in values:\n   second=lambda value=value:value\n";

#[test]
fn lambda_capture_supports_outer_loop() {
    let output = analyze_selected(&["lambda_capture_late"], OUTER_LOOP_CAPTURES);

    let actual = output.candidates.len();

    assert_eq!(actual, 2);
}

#[test]
fn lambda_capture_respects_candidate_caps() {
    let output = analyze_selected_with(
        &["lambda_capture_late"],
        camino::Utf8Path::new("a.py"),
        &[],
        &[],
        1,
        OUTER_LOOP_CAPTURES,
    );

    let actual = output.candidates.len();

    assert_eq!(actual, 1);
    assert!(output.truncated);
}

#[test]
fn lambda_capture_respects_line_and_symbol_selection() {
    let output = analyze_selected_with(
        &["lambda_capture_late"],
        camino::Utf8Path::new("a.py"),
        &[hoimin_core::LineRange { start: 5, end: 5 }],
        &["build".to_owned()],
        10,
        OUTER_LOOP_CAPTURES,
    );

    let actual = output.candidates.len();

    assert_eq!(actual, 1);
    assert_eq!(output.candidates[0].line, 5);
}

#[rstest::rstest]
#[case(true, 0)]
#[case(false, 1)]
fn lambda_capture_stops_on_cancellation_or_callback_rejection(
    #[case] cancelled: bool,
    #[case] expected_count: usize,
) {
    let parsed = ruff_python_parser::parse_module(OUTER_LOOP_CAPTURES).unwrap();
    let mut count = 0;
    let result = super::super::lambda_capture::emit(
        parsed.syntax(),
        OUTER_LOOP_CAPTURES,
        &|| cancelled,
        |_| true,
        |_, _| {
            count += 1;
            false
        },
    );

    let actual = result.is_err();

    assert_eq!(actual, cancelled);
    assert_eq!(count, expected_count);
}

#[test]
fn lambda_capture_excludes_class_namespace_loops_but_allows_method_locals() {
    let source = "class Example:\n callbacks=[]\n for value in [4,9]:\n  callbacks.append(lambda value=value: value)\n def build(self,values):\n  for value in values:\n   callback=lambda value=value: value\n";

    let output = analyze_selected(&["lambda_capture_late"], source);

    assert_eq!(
        output.candidates.len(),
        1,
        "class-local names are not closure cells"
    );
    assert_eq!(output.candidates[0].line, 7);
}
