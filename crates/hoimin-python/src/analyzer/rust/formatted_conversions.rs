//! Change explicit f-string conversion letters, bounded by AST field ranges.
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{
    ConversionFlag, ExprFString, FStringPartRef, InterpolatedElement, InterpolatedStringElement,
};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn emit(
    value: &ExprFString,
    tokens: &Tokens,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, String) -> bool,
) {
    for part in &value.value {
        if cancelled() {
            return;
        }
        let FStringPartRef::FString(part) = part else {
            continue;
        };
        let mut stack = vec![part.elements.iter()];
        while let Some(elements) = stack.last_mut() {
            if cancelled() {
                return;
            }
            let Some(element) = elements.next() else {
                stack.pop();
                continue;
            };
            let InterpolatedStringElement::Interpolation(field) = element else {
                continue;
            };
            if let Some((range, replacement)) = conversion(field, tokens, cancelled)
                && !emit(range, replacement.to_owned())
            {
                return;
            }
            if let Some(spec) = &field.format_spec {
                stack.push(spec.elements.iter());
            }
        }
    }
}
fn conversion(
    field: &InterpolatedElement,
    tokens: &Tokens,
    cancelled: &impl Fn() -> bool,
) -> Option<(TextRange, &'static str)> {
    let replacement = match field.conversion {
        ConversionFlag::Repr => "s",
        ConversionFlag::Str => "r",
        _ => return None,
    };
    let end = field
        .format_spec
        .as_ref()
        .map_or(field.end(), |spec| spec.start());
    let mut tokens = tokens
        .in_range(TextRange::new(field.expression.end(), end))
        .iter();
    while let Some(token) = tokens.next() {
        if cancelled() {
            return None;
        }
        if token.kind() == TokenKind::Exclamation {
            let flag = tokens.next()?;
            return Some((flag.range(), replacement));
        }
    }
    None
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_emit(
        value: &ExprFString,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
        sink: &mut dyn FnMut(TextRange, String) -> bool,
    ) {
        emit(value, tokens, &cancelled, sink);
    }

    #[rstest::rstest]
    #[case("f'{value!r}'", vec!["s"])]
    #[case("f'{value!s}'", vec!["r"])]
    #[case("f'prefix {value!r}'", vec!["s"])]
    #[case("f'{value=!r}'", vec!["s"])]
    #[case("f'{value=}'", vec![])]
    #[case("f'{value!a}'", vec![])]
    #[case("f'{value}'", vec![])]
    #[case("'prefix' f'{value!r}'", vec!["s"])]
    #[case("f'{value!r:{width!s}}'", vec!["s", "r"])]
    fn conversion_edits_follow_fstring_fields_and_nested_format_specs(
        #[case] source: &str,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let value = parsed.syntax().body.as_f_string_expr().unwrap();
        let mut edits = Vec::new();
        run_emit(value, parsed.tokens(), &|| false, &mut |_, replacement| {
            edits.push(replacement);
            true
        });
        assert_eq!(edits, expected);
    }

    #[rstest::rstest]
    #[case(0, 0)]
    #[case(1, 0)]
    #[case(2, 0)]
    #[case(3, 1)]
    #[case(4, 1)]
    fn cancellation_prevents_partial_conversion_edits(
        #[case] checkpoint: usize,
        #[case] expected_count: usize,
    ) {
        let source = "f'{value!r}'";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let value = parsed.syntax().body.as_f_string_expr().unwrap();
        let polls = Cell::new(0);
        let mut edits = Vec::new();
        run_emit(
            value,
            parsed.tokens(),
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
            &mut |_, replacement| {
                edits.push(replacement);
                true
            },
        );
        assert_eq!(edits.len(), expected_count);
        assert_eq!(edits, ["s"][..expected_count]);
    }

    #[test]
    fn rejected_conversion_edit_stops_before_later_fields() {
        let parsed = ruff_python_parser::parse_expression("f'{first!r}{second!s}'").unwrap();
        let value = parsed.syntax().body.as_f_string_expr().unwrap();
        let mut edits = Vec::new();
        run_emit(value, parsed.tokens(), &|| false, &mut |_, replacement| {
            edits.push(replacement);
            false
        });
        assert_eq!(edits, vec!["s"]);
    }

    #[rstest::rstest]
    #[case("f'{value!r}'", vec!["s"])]
    #[case("value", vec![])]
    #[case("f'{value  !r}'", vec![])]
    fn conversion_requires_matching_conversion_tokens(
        #[case] token_source: &str,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression("f'{value!r}'").unwrap();
        let tokens = ruff_python_parser::parse_expression(token_source).unwrap();
        let value = parsed.syntax().body.as_f_string_expr().unwrap();
        let mut edits = Vec::new();
        run_emit(value, tokens.tokens(), &|| false, &mut |_, replacement| {
            edits.push(replacement);
            true
        });
        assert_eq!(edits, expected);
    }
}
