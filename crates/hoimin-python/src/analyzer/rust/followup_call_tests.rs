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

#[test]
fn sort_key_swap_accepts_names_attributes_and_literal_subscripts() {
    for key in [
        "(first, second)",
        "(row.a, row.b)",
        "(row[0], row[1])",
        "(row['a'], row['b'])",
    ] {
        let source = format!("result = sorted(rows, key=lambda row: {key})\n");
        let output = analyze_selected(&["sort_key_swap"], &source);
        assert_eq!(output.candidates.len(), 1, "{source}");
        apply_candidate_and_reparse(&source, &output.candidates[0]);
    }
}

#[test]
fn sort_key_swap_rejects_shadowed_builtins_and_unsupported_keys() {
    for source in [
        "sorted = custom\nresult = sorted(rows, key=lambda row: (row.a, row.b))\n",
        "def f(sorted):\n    return sorted(rows, key=lambda row: (row.a, row.b))\n",
        "result = rows.sort(key=lambda row: (row.a, row.b))\n",
        "result = sorted(rows, key=lambda row: (row.a, row.a))\n",
        "result = sorted(rows, key=lambda row: (row.a, (row).a))\n",
        "result = sorted(rows, key=lambda row: (row['a'], row[\"a\"]))\n",
        "result = sorted(rows, key=lambda row: (row.a,))\n",
        "result = sorted(rows, key=lambda row: (row.a, row.b, row.c))\n",
        "result = sorted(rows, key=lambda row: (read(row), row.b))\n",
        "result = sorted(rows, key=lambda row: (read().a, row.b))\n",
        "result = sorted(rows, key=lambda row: (row.a + 1, row.b))\n",
        "result = sorted(rows, key=lambda row: (row[index], row.b))\n",
        "result = sorted(rows, key=lambda row: (row[:1], row.b))\n",
        "result = sorted(rows, key=lambda row: (*row.a, row.b))\n",
        "result = sorted(rows, key=lambda row: ((yield row.a), row.b))\n",
        "result = sorted(rows, key=lambda row: (1, row.b))\n",
        "result = sorted(rows, key=lambda row: (row.a, row.b), **options)\n",
        "result = sorted(*rows, key=lambda row: (row.a, row.b))\n",
        "result = sorted(rows, unknown=True, key=lambda row: (row.a, row.b))\n",
        "key = lambda row: (row.a, row.b)\nresult = sorted(rows, key=key)\n",
    ] {
        let output = analyze_selected(&["sort_key_swap"], source);
        assert!(
            output.candidates.is_empty(),
            "{source}: {:?}",
            output.candidates
        );
    }
}

#[test]
fn encoding_errors_ignore_accepts_methods_and_trusted_codecs_aliases() {
    for source in [
        "result = text.encode('utf-8', errors='surrogateescape')\n",
        "result = holder.data.decode(errors='surrogateescape')\n",
        "result = data.decode(errors=('surrogateescape'))\n",
        "result = data.decode(errors=r'surrogateescape')\n",
        "import codecs\nresult = codecs.decode(data, 'utf-8', errors='surrogateescape')\n",
        "import codecs as c\nresult = c.encode(text, errors='surrogateescape')\n",
        "from codecs import decode as dec\nresult = dec(data, errors='surrogateescape')\n",
        "from codecs import encode\nresult = encode(text, errors='surrogateescape')\n",
    ] {
        let output = analyze_selected(&["encoding_errors_ignore"], source);
        assert_eq!(output.candidates.len(), 1, "{source}");
        assert_eq!(output.candidates[0].replacement, "'ignore'");
        apply_candidate_and_reparse(source, &output.candidates[0]);
    }
}

#[test]
fn encoding_errors_ignore_rejects_uncertain_identity_and_unsupported_literals() {
    for source in [
        "result = unrelated(errors='surrogateescape')\n",
        "result = data.other(errors='surrogateescape')\n",
        "result = get_data().decode(errors='surrogateescape')\n",
        "result = get_data().value.decode(errors='surrogateescape')\n",
        "result = (a + b).decode(errors='surrogateescape')\n",
        "result = data.decode('utf-8', 'surrogateescape')\n",
        "result = data.decode(errors='ignore')\n",
        "result = data.decode(errors='surrogate' 'escape')\n",
        "result = data.decode(errors=f'surrogateescape')\n",
        "result = data.decode(errors=b'surrogateescape')\n",
        "result = data.decode(errors=policy)\n",
        "result = data.decode(errors='surrogateescape', **options)\n",
        "result = data.decode(*args, errors='surrogateescape')\n",
        "import codecs as c\nc = custom\nresult = c.decode(data, errors='surrogateescape')\n",
        "import codecs as c\ndef f(c):\n    return c.decode(data, errors='surrogateescape')\n",
        "from codecs import decode as dec\ndec = custom\nresult = dec(data, errors='surrogateescape')\n",
        "from codecs import *\nresult = decode(data, errors='surrogateescape')\n",
        "import codecs as c\nexec('untrusted')\nresult = c.decode(data, errors='surrogateescape')\n",
        "import codecs as c\nalias = c\nresult = c.decode(data, errors='surrogateescape')\n",
        "def f():\n    import codecs as c\n    return c.decode(data, errors='surrogateescape')\n",
    ] {
        let output = analyze_selected(&["encoding_errors_ignore"], source);
        assert!(
            output.candidates.is_empty(),
            "{source}: {:?}",
            output.candidates
        );
    }
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

#[test]
fn followup_calls_honor_selection_profiles_and_candidate_limit() {
    let source = "def f():\n    a = sorted(rows, key=lambda row: (row.a, row.b))\n    b = data.decode(errors='surrogateescape')\n";
    for profile in [MutationProfile::Full, MutationProfile::Focused] {
        let output = analyze_selected_with_profile(
            &["sort_key_swap", "encoding_errors_ignore"],
            profile,
            1,
            source,
        );
        assert_eq!(output.candidates.len(), 1);
        apply_candidate_and_reparse(source, &output.candidates[0]);
    }
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

#[test]
fn followup_call_builders_honor_cancellation_and_emit_stop() {
    use super::super::followup_calls::Imports;
    use hoimin_core::MutationOperatorSelection;
    use ruff_python_ast::{Expr, Stmt};
    use std::cell::Cell;

    for (operator, source) in [
        (
            "sort_key_swap",
            "sorted(rows, key=lambda row: (row.a, row.b))\n",
        ),
        (
            "encoding_errors_ignore",
            "data.decode(errors='surrogateescape')\n",
        ),
    ] {
        let operators: MutationOperatorSelection =
            serde_json::from_value(serde_json::json!([operator])).unwrap();
        let parsed = super::parse_module(source).unwrap();
        let imports = Imports::build(parsed.syntax(), &operators, &|| false).unwrap();
        assert!(Imports::build(parsed.syntax(), &operators, &|| true).is_err());
        let Stmt::Expr(statement) = &parsed.syntax().body[0] else {
            panic!("expression statement")
        };
        let Expr::Call(call) = statement.value.as_ref() else {
            panic!("call")
        };
        let emitted = Cell::new(0);
        assert!(!imports.emit(
            call,
            source,
            parsed.tokens(),
            &operators,
            true,
            &|| true,
            |_| true,
            |_, _, _| {
                emitted.set(emitted.get() + 1);
                true
            }
        ));
        assert_eq!(emitted.get(), 0);
        let checks = Cell::new(0);
        assert!(!imports.emit(
            call,
            source,
            parsed.tokens(),
            &operators,
            true,
            &|| {
                checks.set(checks.get() + 1);
                checks.get() >= 3
            },
            |_| true,
            |_, _, _| {
                emitted.set(emitted.get() + 1);
                true
            }
        ));
        assert_eq!(emitted.get(), 0);
        assert!(!imports.emit(
            call,
            source,
            parsed.tokens(),
            &operators,
            true,
            &|| false,
            |_| true,
            |_, _, _| {
                emitted.set(emitted.get() + 1);
                false
            }
        ));
        assert_eq!(emitted.get(), 1);
    }
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
