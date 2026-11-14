use super::{analyze, apply_candidate_and_reparse};
use super::{analyze_selected, analyze_selected_with, analyze_selected_with_profile};
use camino::Utf8Path;
use hoimin_core::{LineRange, MutationProfile};

#[test]
fn sort_key_swap_is_default_for_builtin_sorted() {
    let source = "result = sorted(rows, key=lambda row: (row.primary, row.secondary))\n";

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "sort_key_swap")
        .collect();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].original, "row.primary, row.secondary");
    assert_eq!(candidates[0].replacement, "row.secondary, row.primary");
    apply_candidate_and_reparse(source, candidates[0]);
}

#[test]
fn encoding_errors_ignore_is_default_for_decode() {
    let source = "result = data.decode('utf-8', errors='surrogateescape')\n";

    let output = analyze(source);
    let candidates: Vec<_> = output
        .candidates
        .iter()
        .filter(|candidate| candidate.operator.as_str() == "encoding_errors_ignore")
        .collect();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].original, "'surrogateescape'");
    assert_eq!(candidates[0].replacement, "'ignore'");
    apply_candidate_and_reparse(source, candidates[0]);
}

#[test]
fn sort_key_swap_preserves_grouping_comments_and_utf8() {
    let source = "result = sorted(rows, key=lambda row: (\n    ((row.主)), # priority\n    (row['副']),\n), reverse=True)\n";

    let output = analyze_selected(&["sort_key_swap"], source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(
        output.candidates[0].original,
        "((row.主)), # priority\n    (row['副'])"
    );
    assert_eq!(
        output.candidates[0].replacement,
        "(row['副']), # priority\n    ((row.主))"
    );
    apply_candidate_and_reparse(source, &output.candidates[0]);
}

#[rstest::rstest]
#[case::first_second("(first, second)")]
#[case::row_a_row_b("(row.a, row.b)")]
#[case::row_0_row_1("(row[0], row[1])")]
#[case::row_a_row_b_4("(row['a'], row['b'])")]
fn sort_key_swap_accepts_names_attributes_and_literal_subscripts(#[case] key: &str) {
    let source = format!("result = sorted(rows, key=lambda row: {key})\n");

    let output = analyze_selected(&["sort_key_swap"], &source);

    assert_eq!(output.candidates.len(), 1, "{source}");
    apply_candidate_and_reparse(&source, &output.candidates[0]);
}

#[rstest::rstest]
#[case::sorted_custom_result_sorted_rows_key_lambda_row_row_a_row_b(
    "sorted = custom\nresult = sorted(rows, key=lambda row: (row.a, row.b))\n"
)]
#[case::def_f_sorted_return_sorted_rows_key_lambda_row_row_a_row_b(
    "def f(sorted):\n    return sorted(rows, key=lambda row: (row.a, row.b))\n"
)]
#[case::result_rows_sort_key_lambda_row_row_a_row_b(
    "result = rows.sort(key=lambda row: (row.a, row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_row_a(
    "result = sorted(rows, key=lambda row: (row.a, row.a))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_row_a_5(
    "result = sorted(rows, key=lambda row: (row.a, (row).a))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_row_a_6(
    "result = sorted(rows, key=lambda row: (row['a'], row[\"a\"]))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a(
    "result = sorted(rows, key=lambda row: (row.a,))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_row_b_row_c(
    "result = sorted(rows, key=lambda row: (row.a, row.b, row.c))\n"
)]
#[case::result_sorted_rows_key_lambda_row_read_row_row_b(
    "result = sorted(rows, key=lambda row: (read(row), row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_read_a_row_b(
    "result = sorted(rows, key=lambda row: (read().a, row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_1_row_b(
    "result = sorted(rows, key=lambda row: (row.a + 1, row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_index_row_b(
    "result = sorted(rows, key=lambda row: (row[index], row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_1_row_b(
    "result = sorted(rows, key=lambda row: (row[:1], row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_row_b(
    "result = sorted(rows, key=lambda row: (*row.a, row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_yield_row_a_row_b(
    "result = sorted(rows, key=lambda row: ((yield row.a), row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_1_row_b(
    "result = sorted(rows, key=lambda row: (1, row.b))\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_row_b_options(
    "result = sorted(rows, key=lambda row: (row.a, row.b), **options)\n"
)]
#[case::result_sorted_rows_key_lambda_row_row_a_row_b_18(
    "result = sorted(*rows, key=lambda row: (row.a, row.b))\n"
)]
#[case::result_sorted_rows_unknown_true_key_lambda_row_row_a_row_b(
    "result = sorted(rows, unknown=True, key=lambda row: (row.a, row.b))\n"
)]
#[case::key_lambda_row_row_a_row_b_result_sorted_rows_key_key(
    "key = lambda row: (row.a, row.b)\nresult = sorted(rows, key=key)\n"
)]
fn sort_key_swap_rejects_shadowed_builtins_and_unsupported_keys(#[case] source: &str) {
    let output = analyze_selected(&["sort_key_swap"], source);

    assert!(
        output.candidates.is_empty(),
        "{source}: {:?}",
        output.candidates
    );
}

#[rstest::rstest]
#[case::result_text_encode_utf_8_errors_surrogateescape(
    "result = text.encode('utf-8', errors='surrogateescape')\n"
)]
#[case::result_holder_data_decode_errors_surrogateescape(
    "result = holder.data.decode(errors='surrogateescape')\n"
)]
#[case::result_data_decode_errors_surrogateescape(
    "result = data.decode(errors=('surrogateescape'))\n"
)]
#[case::result_data_decode_errors_r_surrogateescape(
    "result = data.decode(errors=r'surrogateescape')\n"
)]
#[case::import_codecs_result_codecs_decode_data_utf_8_errors_surrogateesc(
    "import codecs\nresult = codecs.decode(data, 'utf-8', errors='surrogateescape')\n"
)]
#[case::import_codecs_as_c_result_c_encode_text_errors_surrogateescape(
    "import codecs as c\nresult = c.encode(text, errors='surrogateescape')\n"
)]
#[case::from_codecs_import_decode_as_dec_result_dec_data_errors_surrogate(
    "from codecs import decode as dec\nresult = dec(data, errors='surrogateescape')\n"
)]
#[case::from_codecs_import_encode_result_encode_text_errors_surrogateesca(
    "from codecs import encode\nresult = encode(text, errors='surrogateescape')\n"
)]
fn encoding_errors_ignore_accepts_methods_and_trusted_codecs_aliases(#[case] source: &str) {
    let output = analyze_selected(&["encoding_errors_ignore"], source);

    assert_eq!(output.candidates.len(), 1, "{source}");
    assert_eq!(output.candidates[0].replacement, "'ignore'");
    apply_candidate_and_reparse(source, &output.candidates[0]);
}

#[rstest::rstest]
#[case::result_unrelated_errors_surrogateescape("result = unrelated(errors='surrogateescape')\n")]
#[case::result_data_other_errors_surrogateescape("result = data.other(errors='surrogateescape')\n")]
#[case::result_get_data_decode_errors_surrogateescape(
    "result = get_data().decode(errors='surrogateescape')\n"
)]
#[case::result_get_data_value_decode_errors_surrogateescape(
    "result = get_data().value.decode(errors='surrogateescape')\n"
)]
#[case::result_a_b_decode_errors_surrogateescape(
    "result = (a + b).decode(errors='surrogateescape')\n"
)]
#[case::result_data_decode_utf_8_surrogateescape(
    "result = data.decode('utf-8', 'surrogateescape')\n"
)]
#[case::result_data_decode_errors_ignore("result = data.decode(errors='ignore')\n")]
#[case::result_data_decode_errors_surrogate_escape(
    "result = data.decode(errors='surrogate' 'escape')\n"
)]
#[case::result_data_decode_errors_f_surrogateescape(
    "result = data.decode(errors=f'surrogateescape')\n"
)]
#[case::result_data_decode_errors_b_surrogateescape(
    "result = data.decode(errors=b'surrogateescape')\n"
)]
#[case::result_data_decode_errors_policy("result = data.decode(errors=policy)\n")]
#[case::result_data_decode_errors_surrogateescape_options(
    "result = data.decode(errors='surrogateescape', **options)\n"
)]
#[case::result_data_decode_args_errors_surrogateescape(
    "result = data.decode(*args, errors='surrogateescape')\n"
)]
#[case::import_codecs_as_c_c_custom_result_c_decode_data_errors_surrogate(
    "import codecs as c\nc = custom\nresult = c.decode(data, errors='surrogateescape')\n"
)]
#[case::import_codecs_as_c_def_f_c_return_c_decode_data_errors_surrogatee(
    "import codecs as c\ndef f(c):\n    return c.decode(data, errors='surrogateescape')\n"
)]
#[case::from_codecs_import_decode_as_dec_dec_custom_result_dec_data_error(
    "from codecs import decode as dec\ndec = custom\nresult = dec(data, errors='surrogateescape')\n"
)]
#[case::from_codecs_import_result_decode_data_errors_surrogateescape(
    "from codecs import *\nresult = decode(data, errors='surrogateescape')\n"
)]
#[case::import_codecs_as_c_exec_untrusted_result_c_decode_data_errors_sur(
    "import codecs as c\nexec('untrusted')\nresult = c.decode(data, errors='surrogateescape')\n"
)]
#[case::import_codecs_as_c_alias_c_result_c_decode_data_errors_surrogatee(
    "import codecs as c\nalias = c\nresult = c.decode(data, errors='surrogateescape')\n"
)]
#[case::def_f_import_codecs_as_c_return_c_decode_data_errors_surrogateesc(
    "def f():\n    import codecs as c\n    return c.decode(data, errors='surrogateescape')\n"
)]
fn encoding_errors_ignore_rejects_uncertain_identity_and_unsupported_literals(
    #[case] source: &str,
) {
    let output = analyze_selected(&["encoding_errors_ignore"], source);

    assert!(
        output.candidates.is_empty(),
        "{source}: {:?}",
        output.candidates
    );
}

#[test]
fn followup_calls_exclude_annotation_expressions() {
    let source = "def f(value: data.decode(errors='surrogateescape')) -> sorted(rows, key=lambda row: (row.a, row.b)):\n    pass\n";

    let output = analyze_selected(&["sort_key_swap", "encoding_errors_ignore"], source);

    assert!(
        output.candidates.is_empty(),
        "annotation expressions produced {:?}",
        output.candidates
    );
}

#[rstest::rstest]
#[case::mutationprofile_full(MutationProfile::Full)]
#[case::mutationprofile_focused(MutationProfile::Focused)]
fn followup_calls_honor_selection_profiles_and_candidate_limit_scenario_1(
    #[case] profile: MutationProfile,
) {
    let source = "def f():\n    a = sorted(rows, key=lambda row: (row.a, row.b))\n    b = data.decode(errors='surrogateescape')\n";

    let output = analyze_selected_with_profile(
        &["sort_key_swap", "encoding_errors_ignore"],
        profile,
        1,
        source,
    );

    assert_eq!(output.candidates.len(), 1);
    apply_candidate_and_reparse(source, &output.candidates[0]);
}

#[test]
fn followup_calls_honor_selection_profiles_and_candidate_limit() {
    let source = "def f():\n    a = sorted(rows, key=lambda row: (row.a, row.b))\n    b = data.decode(errors='surrogateescape')\n";

    let selected_line = [LineRange { start: 3, end: 3 }];

    let output = analyze_selected_with(
        &["sort_key_swap", "encoding_errors_ignore"],
        Utf8Path::new("pkg/sample.py"),
        &selected_line,
        &[],
        10,
        source,
    );

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(
        output.candidates[0].operator.as_str(),
        "encoding_errors_ignore"
    );

    let output = analyze_selected_with(
        &["sort_key_swap", "encoding_errors_ignore"],
        Utf8Path::new("pkg/sample.py"),
        &[],
        &["missing".to_owned()],
        10,
        source,
    );

    assert!(
        output.candidates.is_empty(),
        "unselected symbol produced {:?}",
        output.candidates
    );
}

#[rstest::rstest]
#[case(
    "sort_key_swap",
    "sorted(rows, key=lambda row: (row.a, row.b))\n",
    0,
    true,
    0
)]
#[case(
    "sort_key_swap",
    "sorted(rows, key=lambda row: (row.a, row.b))\n",
    2,
    true,
    0
)]
#[case(
    "sort_key_swap",
    "sorted(rows, key=lambda row: (row.a, row.b))\n",
    usize::MAX,
    false,
    1
)]
#[case(
    "encoding_errors_ignore",
    "data.decode(errors='surrogateescape')\n",
    0,
    true,
    0
)]
#[case(
    "encoding_errors_ignore",
    "data.decode(errors='surrogateescape')\n",
    2,
    true,
    0
)]
#[case(
    "encoding_errors_ignore",
    "data.decode(errors='surrogateescape')\n",
    usize::MAX,
    false,
    1
)]
fn followup_call_builders_honor_cancellation_and_emit_stop(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] cancel_after: usize,
    #[case] accept: bool,
    #[case] expected_emissions: usize,
) {
    use super::super::followup_calls::Imports;
    use hoimin_core::MutationOperatorSelection;
    use std::cell::Cell;
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([operator])).unwrap();
    let parsed = super::parse_module(source).unwrap();
    let imports = Imports::build(parsed.syntax(), &operators, &|| false).unwrap();
    let call = parsed.syntax().body[0]
        .as_expr_stmt()
        .unwrap()
        .value
        .as_call_expr()
        .unwrap();
    let emitted = Cell::new(0);
    let polls = Cell::new(0);
    assert!(!imports.emit(
        call,
        source,
        parsed.tokens(),
        &operators,
        true,
        &|| {
            polls.set(polls.get() + 1);
            polls.get() > cancel_after
        },
        |_| true,
        |_, _, _| {
            emitted.set(emitted.get() + 1);
            accept
        }
    ));
    assert_eq!(emitted.get(), expected_emissions);
}

#[rstest::rstest]
#[case("sort_key_swap", "sorted(rows, key=lambda row: (row.a, row.b))\n")]
#[case("encoding_errors_ignore", "data.decode(errors='surrogateescape')\n")]
fn followup_call_index_build_rejects_cancellation(#[case] operator: &str, #[case] source: &str) {
    use super::super::followup_calls::Imports;
    use hoimin_core::MutationOperatorSelection;
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([operator])).unwrap();
    let parsed = super::parse_module(source).unwrap();

    let actual = Imports::build(parsed.syntax(), &operators, &|| true).is_err();

    assert!(actual);
}

fn assert_multiline_edit_selection(operator: &str, source: &str, line: u32) {
    let output = analyze_selected_with(
        &[operator],
        Utf8Path::new("a.py"),
        &[LineRange {
            start: line,
            end: line,
        }],
        &["run".to_owned()],
        10,
        source,
    );
    assert_eq!(
        output.candidates.len(),
        1,
        "{operator}: selected edit line {line}"
    );
    assert_eq!(output.candidates[0].line, line);
    apply_candidate_and_reparse(source, &output.candidates[0]);
}

#[test]
fn multiline_sort_selects_the_actual_replacement_start_line() {
    assert_multiline_edit_selection(
        "sort_key_swap",
        "def run(rows):\n return sorted(\n  rows,\n  key=lambda row:(\n   row.a,\n   row.b,\n  ),\n )\n",
        5,
    );
}

#[test]
fn multiline_encoding_selects_the_actual_replacement_start_line() {
    assert_multiline_edit_selection(
        "encoding_errors_ignore",
        "def run(data):\n return data.decode(\n  'utf-8',\n  errors='surrogateescape',\n )\n",
        4,
    );
}
