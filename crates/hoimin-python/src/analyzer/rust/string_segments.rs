//! Empty the first nonempty top-level segment, never an interpolation field.
use ruff_python_ast::{Expr, FStringPartRef};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn replacement(
    expression: &Expr,
    source: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<(TextRange, String)> {
    match expression {
        Expr::StringLiteral(value) if value.value.is_implicit_concatenated() => value
            .value
            .iter()
            .take_while(|_| !cancelled())
            .find(|part| !part.value.is_empty())
            .map(|part| empty_token(part.range(), "\"\"", source)),
        Expr::FString(value) => value
            .value
            .iter()
            .take_while(|_| !cancelled())
            .find_map(|part| match part {
                FStringPartRef::Literal(part) if !part.value.is_empty() => {
                    Some(empty_token(part.range(), "\"\"", source))
                }
                FStringPartRef::FString(part) => part
                    .elements
                    .literals()
                    .take_while(|_| !cancelled())
                    .find(|literal| !literal.value.is_empty())
                    // AST literal ranges cover the original escaped text (including both
                    // braces of {{/}}), not its decoded length. Debug text is carried by
                    // interpolation nodes and therefore cannot be selected here.
                    .map(|literal| {
                        if part.elements.len() == 1 {
                            // f'a''b' would become f'''b' if just its body were deleted.
                            // With no interpolation, normalize the entire token to an
                            // empty f-string and separate adjacent quote tokens safely.
                            empty_token(part.range(), "f\"\"", source)
                        } else {
                            (literal.range(), String::new())
                        }
                    }),
                FStringPartRef::Literal(_) => None,
            }),
        _ => None,
    }
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
        replacement(expression, source, &cancelled)
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
        assert_eq!(result.as_deref(), expected);
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
        assert_eq!(
            run(&parsed.syntax().body, source, &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            }),
            None
        );
        assert_eq!(polls.get(), checkpoint + 1);
    }
}
