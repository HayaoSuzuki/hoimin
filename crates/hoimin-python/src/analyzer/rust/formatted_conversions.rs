//! Edit f-string conversion flags and format specifications at their AST field ranges.
use super::{source_delimiters::DelimiterDepth, source_text};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{
    ConversionFlag, ExprFString, FStringPartRef, InterpolatedElement,
    InterpolatedStringElement::{Interpolation, Literal},
    InterpolatedStringFormatSpec, InterpolatedStringLiteralElement, StringFlags,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::ops::Range;

/// Edit explicit conversions and recognized format fields without evaluating code.
/// Removed dynamic specifications retain compiler-visible reads in dead syntax.
/// The sink owns selection and admission; declining it stops field traversal.
pub(super) fn emit_edits(
    value: &ExprFString,
    tokens: &Tokens,
    source: &str,
    delimiters: &DelimiterDepth,
    operators: &MutationOperatorSelection,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    let swap = MutationOperator::FStringConversionSwap;
    let remove = MutationOperator::FStringFormatSpecRemove;
    let change_value = MutationOperator::FStringFormatValueNeighbor;
    if ![swap, remove, change_value]
        .into_iter()
        .any(|op| operators.contains(op))
    {
        return;
    }
    for part in &value.value {
        if cancelled() {
            return;
        }
        let FStringPartRef::FString(part) = part else {
            continue;
        };
        let empty = if part.flags.quote_style().is_double() {
            "''"
        } else {
            "\"\""
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
            let Interpolation(field) = element else {
                continue;
            };
            if operators.contains(swap)
                && let Some(range) = conversion_range(field, tokens, source, cancelled)
            {
                let alternatives = match field.conversion {
                    ConversionFlag::Repr => ["s", "a"],
                    ConversionFlag::Str => ["r", "a"],
                    ConversionFlag::Ascii => ["r", "s"],
                    ConversionFlag::None => unreachable!("conversion range requires a flag"),
                };
                for replacement in alternatives {
                    if cancelled() || !emit(range, replacement.to_owned(), swap) {
                        return;
                    }
                }
            }
            if let Some(spec) = &field.format_spec {
                if operators.contains(remove)
                    && let Some((range, replacement)) =
                        remove_spec(field, spec, tokens, source, empty, delimiters, cancelled)
                    && !emit(range, replacement, remove)
                {
                    return;
                }
                if operators.contains(change_value)
                    && !emit_spec_values(spec, source, cancelled, |range, replacement| {
                        emit(range, replacement, change_value)
                    })
                {
                    return;
                }
                stack.push(spec.elements.iter());
            }
        }
    }
}

fn conversion_range(
    field: &InterpolatedElement,
    tokens: &Tokens,
    source: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<TextRange> {
    let original = field.conversion.to_byte()?;
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
            let range = tokens.next()?.range();
            let text = source_text(source, range)?;
            return (text.as_bytes() == [original]).then_some(range);
        }
    }
    None
}

fn remove_spec(
    field: &InterpolatedElement,
    spec: &InterpolatedStringFormatSpec,
    tokens: &Tokens,
    source: &str,
    empty: &str,
    delimiters: &DelimiterDepth,
    cancelled: &impl Fn() -> bool,
) -> Option<(TextRange, String)> {
    // Without debug text, an empty spec is equivalent to an omitted one.
    if spec.elements.is_empty() && field.debug_text.is_none() {
        return None;
    }
    // A nested field may affect symbol-table or generator classification.
    for token in tokens.in_range(spec.range()) {
        if cancelled() || matches!(token.kind(), TokenKind::ColonEqual | TokenKind::Yield) {
            return None;
        }
    }
    for token in tokens.in_range(TextRange::new(field.expression.end(), spec.start())) {
        if cancelled() {
            return None;
        }
        if token.kind() == TokenKind::Colon {
            let range = TextRange::new(token.start(), spec.end());
            let replacement =
                retain_field_reads(field, spec, source, empty, delimiters, cancelled)?;
            return Some((range, replacement));
        }
    }
    None
}

fn retain_field_reads(
    field: &InterpolatedElement,
    spec: &InterpolatedStringFormatSpec,
    source: &str,
    empty: &str,
    delimiters: &DelimiterDepth,
    cancelled: &impl Fn() -> bool,
) -> Option<String> {
    let mut replacement = String::new();
    let mut stack = vec![spec.elements.iter()];
    while let Some(elements) = stack.last_mut() {
        if cancelled() {
            return None;
        }
        let Some(element) = elements.next() else {
            stack.pop();
            continue;
        };
        let Interpolation(nested) = element else {
            continue;
        };
        if replacement.is_empty() {
            if !delimiters.can_enclose(spec.range(), 2) {
                return None;
            }
            // An omitted spec activates implicit repr on debug fields.
            if field.debug_text.is_some() && field.conversion == ConversionFlag::None {
                replacement.push_str("!r");
            }
            // Preserve closure and implicit class-cell references in the source
            // while the format specification evaluates to the empty string.
            replacement.push_str(":{");
            replacement.push_str(empty);
            replacement.push_str(" if True else (");
        }
        replacement.push('(');
        replacement.push_str(source_text(source, nested.expression.range())?);
        replacement.push_str("),");
        if let Some(nested_spec) = &nested.format_spec {
            stack.push(nested_spec.elements.iter());
        }
    }
    if !replacement.is_empty() {
        replacement.push_str(")}");
    }
    Some(replacement)
}

fn emit_spec_values(
    spec: &InterpolatedStringFormatSpec,
    source: &str,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, String) -> bool,
) -> bool {
    let (head, hole, tail) = match &*spec.elements {
        [Literal(head)] => (Some(head), false, None),
        [Interpolation(_), Literal(tail)] => (None, true, Some(tail)),
        [Literal(head), Interpolation(_)] => (Some(head), true, None),
        [Literal(head), Interpolation(_), Literal(tail)] => (Some(head), true, Some(tail)),
        _ => return true,
    };
    let Some(head_text) = unescaped_text(head, source) else {
        return true;
    };
    let Some(tail_text) = unescaped_text(tail, source) else {
        return true;
    };
    // A single nested numeric field can supply the other dimension. Treat it as
    // one digit for parsing, and never edit a number that touches that placeholder.
    let skeleton;
    let text = if hole {
        skeleton = format!("{head_text}1{tail_text}");
        &skeleton
    } else {
        head_text
    };
    for span in format_value_spans(text, cancelled)
        .into_iter()
        .flatten()
        .flatten()
    {
        let range = if let Some(head) = head.filter(|_| span.end <= head_text.len()) {
            literal_subrange(head.start(), span.clone())
        } else if let Some(tail) = tail.filter(|_| span.start > head_text.len()) {
            let offset = head_text.len() + 1;
            literal_subrange(tail.start(), span.start - offset..span.end - offset)
        } else {
            continue;
        };
        let digits = &text[span];
        // Width/precision are Python size values. Avoid parsing arbitrarily
        // large custom specs or emitting a new integer overflow.
        if digits.len() > 19 {
            continue;
        }
        let Ok(value) = digits.parse::<isize>() else {
            continue;
        };
        for neighbor in [
            value.checked_sub(1).filter(|value| *value >= 0),
            value.checked_add(1),
        ]
        .into_iter()
        .flatten()
        {
            if cancelled() || !emit(range, neighbor.to_string()) {
                return false;
            }
        }
    }
    !cancelled()
}

fn unescaped_text<'s>(
    literal: Option<&InterpolatedStringLiteralElement>,
    source: &'s str,
) -> Option<&'s str> {
    let Some(literal) = literal else {
        return Some("");
    };
    let text = source_text(source, literal.range())?;
    (text == literal.value.as_ref()).then_some(text)
}

fn literal_subrange(start: TextSize, span: Range<usize>) -> TextRange {
    // Offsets are within the corresponding AST literal's source range.
    TextRange::new(
        start + TextSize::try_from(span.start).expect("literal span"),
        start + TextSize::try_from(span.end).expect("literal span"),
    )
}

fn format_value_spans(
    text: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<[Option<Range<usize>>; 2]> {
    let mut chars = text.chars();
    let _ = chars.next();
    let align = ['<', '>', '=', '^'];
    let mut rest = if chars.next().is_some_and(|ch| align.contains(&ch)) {
        chars.as_str()
    } else {
        text.strip_prefix(align).unwrap_or(text)
    };
    for flag in [&['+', '-', ' '][..], &['z'], &['#'], &['0']] {
        rest = rest.strip_prefix(flag).unwrap_or(rest);
    }
    let width = take_digits(text, &mut rest, cancelled)?;
    let width = (!width.is_empty()).then_some(width);
    rest = rest.strip_prefix([',', '_']).unwrap_or(rest);
    let precision = if let Some(after_dot) = rest.strip_prefix('.') {
        rest = after_dot;
        let precision = take_digits(text, &mut rest, cancelled)?;
        if precision.is_empty() {
            return None;
        }
        rest = rest.strip_prefix([',', '_']).unwrap_or(rest);
        Some(precision)
    } else {
        None
    };
    matches!(
        rest,
        "" | "b"
            | "c"
            | "d"
            | "e"
            | "E"
            | "f"
            | "F"
            | "g"
            | "G"
            | "n"
            | "o"
            | "s"
            | "x"
            | "X"
            | "%"
    )
    .then_some([width, precision])
}

fn take_digits(text: &str, rest: &mut &str, cancelled: &impl Fn() -> bool) -> Option<Range<usize>> {
    let start = text.len() - rest.len();
    let mut length = 0;
    for byte in rest.bytes() {
        if cancelled() {
            return None;
        }
        if !byte.is_ascii_digit() {
            break;
        }
        length += 1;
    }
    *rest = &rest[length..];
    Some(start..start + length)
}
#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_emit(
        value: &ExprFString,
        tokens: &Tokens,
        source: &str,
        cancelled: &dyn Fn() -> bool,
        sink: &mut dyn FnMut(TextRange, String) -> bool,
    ) {
        let selection =
            serde_json::from_value(serde_json::json!(["fstring_conversion_swap"])).unwrap();
        emit_edits(
            value,
            tokens,
            source,
            &DelimiterDepth::default(),
            &selection,
            &cancelled,
            |range, replacement, _| sink(range, replacement),
        );
    }

    #[rstest::rstest]
    #[case("f'{value!r}'", vec!["s", "a"])]
    #[case("f'{value!s}'", vec!["r", "a"])]
    #[case("f'prefix {value!r}'", vec!["s", "a"])]
    #[case("f'{value=!r}'", vec!["s", "a"])]
    #[case("f'{value=}'", vec![])]
    #[case("f'{value!a}'", vec!["r", "s"])]
    #[case("f'{value}'", vec![])]
    #[case("'prefix' f'{value!r}'", vec!["s", "a"])]
    #[case("f'{value!r:{width!s}}'", vec!["s", "a", "r", "a"])]
    fn conversion_edits_follow_fstring_fields_and_nested_format_specs(
        #[case] source: &str,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let value = parsed.syntax().body.as_f_string_expr().unwrap();
        let mut edits = Vec::new();
        run_emit(
            value,
            parsed.tokens(),
            source,
            &|| false,
            &mut |_, replacement| {
                edits.push(replacement);
                true
            },
        );

        assert_eq!(edits, expected);
    }

    #[rstest::rstest]
    #[case(0, 0)]
    #[case(1, 0)]
    #[case(2, 0)]
    #[case(3, 0)]
    #[case(4, 1)]
    #[case(5, 2)]
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
            source,
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

        let actual = edits.len();

        assert_eq!(actual, expected_count);
        assert_eq!(edits, ["s", "a"][..expected_count]);
    }

    #[test]
    fn rejected_conversion_edit_stops_before_later_fields() {
        let source = "f'{first!r}{second!s}'";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let value = parsed.syntax().body.as_f_string_expr().unwrap();
        let mut edits = Vec::new();
        run_emit(
            value,
            parsed.tokens(),
            source,
            &|| false,
            &mut |_, replacement| {
                edits.push(replacement);
                false
            },
        );

        assert_eq!(edits, vec!["s"]);
    }

    #[rstest::rstest]
    #[case("f'{value!r}'", vec!["s", "a"])]
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
        run_emit(
            value,
            tokens.tokens(),
            token_source,
            &|| false,
            &mut |_, replacement| {
                edits.push(replacement);
                true
            },
        );

        assert_eq!(edits, expected);
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    use hoimin_core::MutationOperator;

    #[rstest::rstest]
    #[case::static_remove("f'{x!s:>4}'", MutationOperator::FStringFormatSpecRemove, &["f'{x!s}'"])]
    #[case::dynamic_remove("f'{x:>{width()}}'", MutationOperator::FStringFormatSpecRemove,
        &["f'{x:{\"\" if True else ((width()),)}}'"])]
    #[case::debug_dynamic_remove("f'{x=:{width}}'", MutationOperator::FStringFormatSpecRemove,
        &["f'{x=!r:{\"\" if True else ((width),)}}'"])]
    #[case::assignment_in_spec("f'{x:{(width := 4)}}'", MutationOperator::FStringFormatSpecRemove, &[])]
    #[case::yield_in_spec("f'{x:{(yield 4)}}'", MutationOperator::FStringFormatSpecRemove, &[])]
    #[case::debug_remove("f'{x=:}'", MutationOperator::FStringFormatSpecRemove, &["f'{x=}'"])]
    #[case::empty_remove("f'{x:}'", MutationOperator::FStringFormatSpecRemove, &[])]
    #[case::width_precision("f'{x:0=+010,.2f}'", MutationOperator::FStringFormatValueNeighbor, &[
        "f'{x:0=+09,.2f}'", "f'{x:0=+011,.2f}'", "f'{x:0=+010,.1f}'", "f'{x:0=+010,.3f}'",
    ])]
    #[case::fill_digit("f'{x:3>4}'", MutationOperator::FStringFormatValueNeighbor, &[
        "f'{x:3>3}'", "f'{x:3>5}'",
    ])]
    #[case::unicode_fill("f'{x:é^2}'", MutationOperator::FStringFormatValueNeighbor, &[
        "f'{x:é^1}'", "f'{x:é^3}'",
    ])]
    #[case::width_one("f'{x:1}'", MutationOperator::FStringFormatValueNeighbor, &["f'{x:0}'", "f'{x:2}'"])]
    #[case::precision_zero("f'{x:.0f}'", MutationOperator::FStringFormatValueNeighbor, &["f'{x:.1f}'"])]
    #[case::unknown_type("f'{x:4custom}'", MutationOperator::FStringFormatValueNeighbor, &[])]
    #[case::dynamic_width("f'{x:0{width}.2f}'", MutationOperator::FStringFormatValueNeighbor, &["f'{x:0{width}.1f}'", "f'{x:0{width}.3f}'"])]
    #[case::dynamic_precision("f'{x:>8.{precision}f}'", MutationOperator::FStringFormatValueNeighbor,
        &["f'{x:>7.{precision}f}'", "f'{x:>9.{precision}f}'"])]
    #[case::ambiguous_width("f'{x:8{width}f}'", MutationOperator::FStringFormatValueNeighbor, &[])]
    fn emits_format_edits_in_their_own_fields(
        #[case] source: &str,
        #[case] operator: MutationOperator,
        #[case] expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let selection = serde_json::from_value(serde_json::json!([operator.as_str()])).unwrap();
        let mut actual = Vec::new();

        emit_edits(
            parsed.syntax().body.as_f_string_expr().unwrap(),
            parsed.tokens(),
            source,
            &DelimiterDepth::default(),
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

    #[rstest::rstest]
    #[case::at_limit(196, 1)]
    #[case::above_limit(197, 0)]
    fn dynamic_removal_accounts_for_added_delimiters(#[case] depth: usize, #[case] count: usize) {
        let source = format!(
            "{}f'{{x:{{width}}}}'{}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let parsed = ruff_python_parser::parse_expression(&source).unwrap();
        let delimiters = DelimiterDepth::build(parsed.tokens(), &|| false).unwrap();
        let selection =
            serde_json::from_value(serde_json::json!(["fstring_format_spec_remove"])).unwrap();
        let mut actual = 0;

        emit_edits(
            parsed.syntax().body.as_f_string_expr().unwrap(),
            parsed.tokens(),
            &source,
            &delimiters,
            &selection,
            &|| false,
            |_, _, _| {
                actual += 1;
                true
            },
        );

        assert_eq!(actual, count);
    }
}
