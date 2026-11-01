use super::{analyze, analyze_selected, analyze_selected_with, apply_candidate_and_reparse};
use camino::Utf8Path;
use hoimin_core::LineRange;

#[rstest::rstest]
#[case::for_x_in_xs_if_x_continue_consume_x(
    "for x in xs:\n if x: continue\n consume(x)\n",
    "continue_remove",
    "continue",
    "pass"
)]
#[case::class_token_def_eq_self_other_return_notimplemented(
    "class Token:\n def __eq__(self, other):\n  return NotImplemented\n",
    "comparison_not_implemented",
    "NotImplemented",
    "False"
)]
#[case::raise_lookuperror_bad_from_source(
    "raise LookupError('bad') from source\n",
    "exception_cause_remove",
    "source",
    "None"
)]
fn behavioral_research_adopted_mutations_are_default(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] original: &str,
    #[case] replacement: &str,
) {
    let output = analyze(source);
    let rows: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == operator)
        .collect();

    assert_eq!(rows.len(), 1, "{operator}");
    assert_eq!(rows[0].original, original);
    assert_eq!(rows[0].replacement, replacement);
    apply_candidate_and_reparse(source, rows[0]);
}

#[rstest::rstest]
#[case::def_eq_self_other_return_notimplemented(
    "def __eq__(self, other):\n return NotImplemented\n"
)]
#[case::class_a_def_f_self_return_notimplemented(
    "class A:\n def f(self):\n  return NotImplemented\n"
)]
#[case::class_a_def_add_self_other_return_notimplemented(
    "class A:\n def __add__(self, other):\n  return NotImplemented\n"
)]
#[case::class_a_def_eq_self_notimplemented_return_notimplemented(
    "class A:\n def __eq__(self, NotImplemented):\n  return NotImplemented\n"
)]
#[case::notimplemented_sentinel_class_a_def_eq_self_other_return_notimple(
    "NotImplemented = sentinel\nclass A:\n def __eq__(self, other):\n  return NotImplemented\n"
)]
#[case::class_a_def_eq_self_other_def_nested_return_notimplemented_return(
    "class A:\n def __eq__(self, other):\n  def nested():\n   return NotImplemented\n  return nested()\n"
)]
#[case::class_a_staticmethod_def_eq_other_return_notimplemented(
    "class A:\n @staticmethod\n def __eq__(other):\n  return NotImplemented\n"
)]
#[case::class_a_async_def_eq_self_other_return_notimplemented(
    "class A:\n async def __eq__(self, other):\n  return NotImplemented\n"
)]
#[case::class_a_def_eq_self_other_yield_other_return_notimplemented(
    "class A:\n def __eq__(self, other):\n  yield other\n  return NotImplemented\n"
)]
#[case::from_somewhere_import_class_a_def_eq_self_other_return_notimpleme(
    "from somewhere import *\nclass A:\n def __eq__(self, other):\n  return NotImplemented\n"
)]
#[case::exec_code_class_a_def_eq_self_other_return_notimplemented(
    "exec(code)\nclass A:\n def __eq__(self, other):\n  return NotImplemented\n"
)]
fn comparison_mutation_excludes_non_methods_shadowing_and_generators(#[case] source: &str) {
    assert!(
        analyze_selected(&["comparison_not_implemented"], source)
            .candidates
            .is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::raise_error_from_none_1(0)]
#[case::raise_error_from_cause_2(1)]
#[case::raise_error_from_holder_cause_3(2)]
#[case::raise_error_from_a_if_flag_else_b_4(3)]
#[case::raise_error_5(4)]
fn named_causes_and_continue_keep_comments_and_utf8_scenario_1(#[case] scenario_index: usize) {
    let source = "結果 = 1\nfor item in xs:\n with resource():\n  try:\n   if item: continue # next item\n   consume(item)\n  finally:\n   close(item)\nelse:\n finish()\ntry:\n read()\nexcept ValueError as reason:\n raise LookupError('bad') from ( # preserve\n  reason # cause\n )\n";

    let output = analyze_selected(&["continue_remove", "exception_cause_remove"], source);

    assert_eq!(output.candidates.len(), 2);
    for candidate in &output.candidates {
        let changed = apply_candidate_and_reparse(source, candidate);
        for comment in ["# next item", "# preserve", "# cause"] {
            assert!(changed.contains(comment));
        }
    }

    let source = [
        "raise Error from None\n",
        "raise Error from cause()\n",
        "raise Error from holder.cause\n",
        "raise Error from (a if flag else b)\n",
        "raise Error\n",
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    assert!(
        analyze_selected(&["exception_cause_remove"], source)
            .candidates
            .is_empty(),
        "{source}"
    );
}

#[test]
fn named_causes_and_continue_keep_comments_and_utf8() {
    let source = "結果 = 1\nfor item in xs:\n with resource():\n  try:\n   if item: continue # next item\n   consume(item)\n  finally:\n   close(item)\nelse:\n finish()\ntry:\n read()\nexcept ValueError as reason:\n raise LookupError('bad') from ( # preserve\n  reason # cause\n )\n";

    let output = analyze_selected(&["continue_remove", "exception_cause_remove"], source);

    assert_eq!(output.candidates.len(), 2);
    for candidate in &output.candidates {
        let changed = apply_candidate_and_reparse(source, candidate);
        for comment in ["# next item", "# preserve", "# cause"] {
            assert!(changed.contains(comment));
        }
    }
}

#[test]
fn behavioral_research_caps_selection_and_cancellation() {
    let source =
        "def first(xs):\n for x in xs:\n  continue\ndef second(xs):\n for x in xs:\n  continue\n";

    let output = analyze_selected_with(
        &["continue_remove"],
        Utf8Path::new("a.py"),
        &[],
        &[],
        1,
        source,
    );

    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);

    let output = analyze_selected_with(
        &["continue_remove"],
        Utf8Path::new("a.py"),
        &[LineRange { start: 6, end: 6 }],
        &["second".to_owned()],
        10,
        source,
    );

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].line, 6);
    let operators = serde_json::from_value(serde_json::json!([
        "continue_remove",
        "comparison_not_implemented",
        "exception_cause_remove"
    ]))
    .unwrap();
    let request = super::super::AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("a.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: hoimin_core::MutationProfile::Full,
        max_candidates: 1,
    };
    assert!(super::super::analyze_source_cancellable(&request, source, || true).is_err());
}

#[rstest::rstest]
#[case::eq("__eq__")]
#[case::ne("__ne__")]
#[case::lt("__lt__")]
#[case::le("__le__")]
#[case::gt("__gt__")]
#[case::ge("__ge__")]
fn comparison_methods_restore_scope_after_nested_definitions(#[case] method: &str) {
    let source = format!(
        "class A:\n def {method}(self, other):\n  def nested():\n   return NotImplemented\n  return (NotImplemented) # delegate\n"
    );

    let output = analyze_selected(&["comparison_not_implemented"], &source);

    assert_eq!(output.candidates.len(), 1, "{method}");
    assert!(
        apply_candidate_and_reparse(&source, &output.candidates[0])
            .contains("return (False) # delegate")
    );
}
