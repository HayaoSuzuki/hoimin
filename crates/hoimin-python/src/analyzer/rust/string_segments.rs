//! Edit independent string segments and empty whole bytes literals.
use super::source_text;
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprBytesLiteral, FStringPartRef};
use ruff_text_size::{Ranged, TextRange};

/// Empty each eligible text segment independently, or the entire bytes value.
/// The collector excludes protected source roles and anchors selection at the
/// containing expression. A declined sink stops traversal before the next edit.
pub(super) fn emit(
    expression: &Expr,
    source: &str,
    operators: &MutationOperatorSelection,
    cancelled: &impl Fn() -> bool,
    mut sink: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    if let Expr::BytesLiteral(bytes) = expression {
        if operators.contains(MutationOperator::BytesLiteralEmpty)
            && let Some(replacement) = empty_bytes(bytes, source, cancelled)
        {
            sink(
                bytes.range(),
                replacement,
                MutationOperator::BytesLiteralEmpty,
            );
        }
        return;
    }
    if operators.contains(MutationOperator::StringSegmentEmpty) {
        emit_segments(expression, source, cancelled, |(range, replacement)| {
            sink(range, replacement, MutationOperator::StringSegmentEmpty)
        });
    }
}

fn emit_segments(
    expression: &Expr,
    source: &str,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut((TextRange, String)) -> bool,
) {
    match expression {
        Expr::StringLiteral(value) if value.value.is_implicit_concatenated() => {
            for part in &value.value {
                if cancelled() {
                    return;
                }
                if !part.value.is_empty() && !emit(empty_token(part.range(), "\"\"", source)) {
                    return;
                }
            }
        }
        Expr::FString(value) => {
            for part in &value.value {
                if cancelled() {
                    return;
                }
                match part {
                    FStringPartRef::Literal(part) if !part.value.is_empty() => {
                        if !emit(empty_token(part.range(), "\"\"", source)) {
                            return;
                        }
                    }
                    FStringPartRef::FString(part) => {
                        for element in &part.elements {
                            if cancelled() {
                                return;
                            }
                            let Some(literal) = element.as_literal() else {
                                continue;
                            };
                            if literal.value.is_empty() {
                                continue;
                            }
                            // AST literal ranges cover original escaped text. Debug
                            // text belongs to interpolation nodes.
                            let candidate = if part.elements.len() == 1 {
                                // Preserve the token boundary in spellings like f'a''b'.
                                empty_token(part.range(), "f\"\"", source)
                            } else {
                                (literal.range(), String::new())
                            };
                            if !emit(candidate) {
                                return;
                            }
                        }
                    }
                    FStringPartRef::Literal(_) => {}
                }
            }
        }
        _ => {}
    }
}

fn empty_bytes(
    bytes: &ExprBytesLiteral,
    source: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<String> {
    let mut replacement = String::new();
    let mut copied_end = bytes.start();
    for part in &bytes.value {
        if cancelled() {
            return None;
        }
        if !part.is_empty() {
            // Each empty bytes token has its own prefix, so adjacent quote
            // delimiters cannot merge. Preserve comments and token spacing.
            replacement.push_str(source_text(
                source,
                TextRange::new(copied_end, part.start()),
            )?);
            replacement.push_str("b\"\"");
            copied_end = part.end();
        }
    }
    if replacement.is_empty() || cancelled() {
        return None;
    }
    replacement.push_str(source_text(
        source,
        TextRange::new(copied_end, bytes.end()),
    )?);
    Some(replacement)
}

fn empty_token(range: TextRange, empty: &str, source: &str) -> (TextRange, String) {
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    // Add separators only against existing quote tokens. In particular, never
    // add indentation before a standalone expression at the start of a line.
    let leading = source[..start].ends_with(['\'', '"']);
    let trailing = source[end..].starts_with(['\'', '"']);
    (
        range,
        format!(
            "{}{empty}{}",
            if leading { " " } else { "" },
            if trailing { " " } else { "" }
        ),
    )
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run(
        expression: &Expr,
        source: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, String)> {
        let mut first = None;
        emit_segments(expression, source, &cancelled, |edit| {
            first = Some(edit);
            false
        });
        first
    }

    #[rstest::rstest]
    #[case("'a''b'", Some("\"\" 'b'"))]
    #[case("'' 'b'", Some("'' \"\""))]
    #[case("'a'", None)]
    #[case("f'a''b'", Some("f\"\" 'b'"))]
    #[case("''f'a{x}b'", Some("''f'{x}b'"))]
    #[case("f'{x}b'", Some("f'{x}'"))]
    #[case("f'{x}'", None)]
    #[case("f'' 'a'", Some("f'' \"\""))]
    #[case("'' 'a''b'", Some("'' \"\" 'b'"))]
    #[case("''f'a'", Some("'' f\"\""))]
    fn empties_the_first_nonempty_literal_without_merging_quote_tokens(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let result = run(&parsed.syntax().body, source, &|| false).map(|(range, replacement)| {
            let mut output = source.to_owned();
            output.replace_range(
                usize::from(range.start())..usize::from(range.end()),
                &replacement,
            );
            output
        });

        let actual = result.as_deref();

        assert_eq!(actual, expected);
    }

    #[rstest::rstest]
    #[case("'' 'a'", 0)]
    #[case("'' 'a'", 1)]
    #[case("''f'{x}a'", 0)]
    #[case("''f'{x}a'", 1)]
    #[case("''f'{x}a'", 2)]
    fn cancellation_stops_before_selecting_a_segment(
        #[case] source: &str,
        #[case] checkpoint: usize,
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let polls = Cell::new(0);

        let actual = run(&parsed.syntax().body, source, &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        let actual_2 = polls.get();

        assert_eq!(actual, None);
        assert_eq!(actual_2, checkpoint + 1);
    }
}
#[cfg(test)]
mod expanded_tests {
    use super::*;
    use hoimin_core::{MutationOperator, MutationOperatorSelection};

    #[rstest::rstest]
    #[case::nonempty_concat("'a''b'", MutationOperator::StringSegmentEmpty, &["\"\" 'b'", "'a' \"\""])]
    #[case::top_level_field("f'a{x}b'", MutationOperator::StringSegmentEmpty, &["f'{x}b'", "f'a{x}'"])]
    #[case::bytes("RB'\x41'", MutationOperator::BytesLiteralEmpty, &["b\"\""])]
    #[case::concat_bytes("(b'a' # retain comment\n br'b')", MutationOperator::BytesLiteralEmpty, &["(b\"\" # retain comment\n b\"\")"])]
    #[case::already_empty("b'' b\"\"", MutationOperator::BytesLiteralEmpty, &[])]
    fn emits_each_nonempty_string_segment_and_an_empty_bytes_literal(
        #[case] source: &str,
        #[case] operator: MutationOperator,
        #[case] expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let mut selection = MutationOperatorSelection::default();
        selection.include(operator);
        let mut actual = Vec::new();

        emit(
            &parsed.syntax().body,
            source,
            &selection,
            &|| false,
            |range, replacement, kind| {
                assert_eq!(kind, operator);
                let mut mutant = source.to_owned();
                mutant.replace_range(
                    usize::from(range.start())..usize::from(range.end()),
                    &replacement,
                );
                ruff_python_parser::parse_expression(&mutant).unwrap();
                actual.push(mutant);
                true
            },
        );

        assert_eq!(actual, expected);
    }
}
