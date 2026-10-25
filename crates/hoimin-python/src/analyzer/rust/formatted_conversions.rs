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
