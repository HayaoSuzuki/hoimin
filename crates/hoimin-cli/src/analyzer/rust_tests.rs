use super::{AnalyzeRequest, analyze_source};
use crate::analyzer::AnalyzerDiagnosticCode;
use camino::Utf8Path;
use hoimin_core::{ByteSpan, LineRange};
use proptest::prelude::*;

fn analyze(source: &str) -> super::AnalyzerOutput {
    analyze_with(Utf8Path::new("pkg/sample.py"), &[], &[], 10_000, source)
}

fn analyze_with(
    path: &Utf8Path,
    lines: &[LineRange],
    symbols: &[String],
    max_candidates: usize,
    source: &str,
) -> super::AnalyzerOutput {
    analyze_source(
        &AnalyzeRequest {
            path,
            lines,
            symbols,
            max_candidates,
        },
        source,
    )
}

proptest! {
    #[test]
    fn arbitrary_python_input_has_ordered_in_bounds_candidates(source in ".{0,4096}") {
        let output = analyze(&source);
        let mut previous_end = 0;
        for candidate in output.candidates {
            let start = usize::try_from(candidate.span.start).expect("span start fits usize");
            let length = usize::try_from(candidate.span.length).expect("span length fits usize");
            let end = start.checked_add(length).expect("candidate span does not overflow");
            prop_assert!(previous_end <= start);
            prop_assert!(end <= source.len());
            prop_assert_eq!(&source[start..end], candidate.original);
            previous_end = end;
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
fn filters_candidates_by_line_or_symbol() {
    let source = "def first(a, b):\n    return a == b\n\ndef second(a, b):\n    return a == b\n";
    let output = analyze_with(
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 2, end: 2 }],
        &["pkg.sample:second".to_owned()],
        10_000,
        source,
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| candidate.symbol.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("first"), Some("second")]
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
