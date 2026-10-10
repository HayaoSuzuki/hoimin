//! Empty the first nonempty top-level segment, never an interpolation field.
use ruff_python_ast::{Expr, FStringPart};
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
                FStringPart::Literal(part) if !part.value.is_empty() => {
                    Some(empty_token(part.range(), "\"\"", source))
                }
                FStringPart::FString(part) => part
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
                FStringPart::Literal(_) => None,
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
