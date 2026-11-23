//! Exact neighbors of integer and floating-point literals.
use super::source_text;
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprNumberLiteral, Number, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

fn number(expression: &Expr) -> Option<(&ExprNumberLiteral, bool)> {
    let (operand, negative) = match expression {
        Expr::UnaryOp(unary) if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub) => {
            (unary.operand.as_ref(), unary.op == UnaryOp::USub)
        }
        operand => (operand, false),
    };
    Some((operand.as_number_literal_expr()?, negative))
}

/// Whether this numeric atom owns a unary sign and must suppress its leaf visit.
pub(super) fn is_signed(expression: &Expr) -> bool {
    expression.is_unary_op_expr() && number(expression).is_some()
}

/// Emit up to two exact finite neighbors, stopping when the sink declines.
/// The collector owns source-role eligibility and room for one enclosing pair.
/// Integer suppression leaves float literals available in computed indices.
pub(super) fn emit(
    expression: &Expr,
    source: &str,
    operators: &MutationOperatorSelection,
    integer_enabled: bool,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    let Some((number, negative)) = number(expression) else {
        return;
    };
    let range = expression.range();
    match number.value {
        Number::Int(_)
            if integer_enabled && operators.contains(MutationOperator::IntegerLiteralNeighbor) =>
        {
            if let Some(integer) = IntegerDigits::parse(source, number, negative, cancelled) {
                integer.emit(IntegerRole::Literal, cancelled, |neighbor| {
                    emit(range, neighbor, MutationOperator::IntegerLiteralNeighbor)
                });
            }
        }
        Number::Float(magnitude) if operators.contains(MutationOperator::FloatLiteralNeighbor) => {
            let value = if negative { -magnitude } else { magnitude };
            if value.is_finite() {
                for neighbor in [value.next_down(), value.next_up()] {
                    if cancelled() {
                        return;
                    }
                    // Exponent notation is a float even when the value is integral.
                    if neighbor.is_finite()
                        && !emit(
                            range,
                            format!("({neighbor:e})"),
                            MutationOperator::FloatLiteralNeighbor,
                        )
                    {
                        return;
                    }
                }
            }
        }
        _ => {}
    }
}

/// Emit integer position neighbors with the established unsigned-zero policy.
/// A stride excludes zero; large decimal carries may require one grouping pair.
pub(super) fn emit_index_neighbors(
    expression: &Expr,
    source: &str,
    excludes_zero: bool,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, String) -> bool,
) {
    let Some((number, negative)) = number(expression) else {
        return;
    };
    if matches!(number.value, Number::Int(_))
        && let Some(integer) = IntegerDigits::parse(source, number, negative, cancelled)
    {
        integer.emit(
            IntegerRole::Index { excludes_zero },
            cancelled,
            |neighbor| emit(expression.range(), neighbor),
        );
    }
}

#[derive(Clone, Copy)]
enum IntegerRole {
    Literal,
    Index { excludes_zero: bool },
}

struct IntegerDigits {
    digits: Vec<u8>,
    radix: u8,
    prefix: &'static str,
    negative_spelling: bool,
}

impl IntegerDigits {
    fn parse(
        source: &str,
        number: &ExprNumberLiteral,
        negative_spelling: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Self> {
        let text = source_text(source, number.range())?;
        let (radix, prefix) = match text.as_bytes().get(..2) {
            Some(b"0b" | b"0B") => (2, "0b"),
            Some(b"0o" | b"0O") => (8, "0o"),
            Some(b"0x" | b"0X") => (16, "0x"),
            _ => (10, ""),
        };
        let mut digits = Vec::new();
        for byte in text.bytes().skip(prefix.len()) {
            if cancelled() {
                return None;
            }
            match byte {
                b'_' => {}
                b'0' if digits.is_empty() => {}
                _ => digits.push(u8::try_from(char::from(byte).to_digit(u32::from(radix))?).ok()?),
            }
        }
        Some(Self {
            digits,
            radix,
            prefix,
            negative_spelling,
        })
    }

    fn emit(
        &self,
        role: IntegerRole,
        cancelled: &impl Fn() -> bool,
        mut emit: impl FnMut(String) -> bool,
    ) {
        // Index/slice candidates retain their original order and zero rules.
        let (directions, excludes_negative, excludes_zero) = match role {
            IntegerRole::Literal => ([false, true], false, false),
            IntegerRole::Index { excludes_zero } => {
                ([true, false], !self.negative_spelling, excludes_zero)
            }
        };
        for higher in directions {
            if cancelled() {
                return;
            }
            let (negative, increase) = if self.digits.is_empty() {
                (!higher, true)
            } else if self.negative_spelling {
                (true, !higher)
            } else {
                (false, higher)
            };
            if excludes_negative && negative {
                continue;
            }
            let Some(digits) = self.step_magnitude(increase, cancelled) else {
                return;
            };
            let zero = digits == [0];
            if excludes_zero && zero {
                continue;
            }
            let negative = negative && !zero;
            // CPython can cap the number of decimal literal digits at any
            // value from 640 upward. A decimal carry to an extra digit is
            // exactly a power of ten, which needs no large decimal token.
            if self.radix == 10 && self.digits.len() >= 640 && digits.len() > self.digits.len() {
                let sign = if negative { "-" } else { "" };
                if !emit(format!("({sign}10**{})", self.digits.len())) {
                    return;
                }
                continue;
            }
            let grouped = matches!(role, IntegerRole::Literal);
            let mut replacement = String::new();
            if grouped {
                replacement.push('(');
            }
            if negative {
                replacement.push('-');
            }
            replacement.push_str(self.prefix);
            // Preserve power-of-two radices for large literals. Rendering in decimal
            // could exceed CPython's integer string conversion limit.
            for digit in digits {
                if cancelled() {
                    return;
                }
                replacement.push(char::from(b"0123456789abcdef"[usize::from(digit)]));
            }
            if grouped {
                replacement.push(')');
            }
            if !emit(replacement) {
                return;
            }
        }
    }

    fn step_magnitude(&self, increase: bool, cancelled: &impl Fn() -> bool) -> Option<Vec<u8>> {
        let mut digits = self.digits.clone();
        let carry_from = if increase { self.radix - 1 } else { 0 };
        let carry_to = if increase { 0 } else { self.radix - 1 };
        for digit in digits.iter_mut().rev() {
            if cancelled() {
                return None;
            }
            if *digit != carry_from {
                if increase {
                    *digit += 1;
                } else {
                    *digit -= 1;
                }
                if digits.len() > 1 && digits[0] == 0 {
                    digits.remove(0);
                }
                return Some(digits);
            }
            *digit = carry_to;
        }
        // Only addition can carry beyond the most significant digit.
        digits.insert(0, 1);
        Some(digits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[rstest::rstest]
    #[case::hex("0x_FF", MutationOperator::IntegerLiteralNeighbor, vec!["(0xfe)", "(0x100)"])]
    #[case::binary("-0B1_000", MutationOperator::IntegerLiteralNeighbor, vec!["(-0b1001)", "(-0b111)"])]
    #[case::octal("+0o_10", MutationOperator::IntegerLiteralNeighbor, vec!["(0o7)", "(0o11)"])]
    #[case::decimal("999_999_999_999_999_999_999_999_999_999", MutationOperator::IntegerLiteralNeighbor, vec![
        "(999999999999999999999999999998)", "(1000000000000000000000000000000)",
    ])]
    #[case::float("1.0", MutationOperator::FloatLiteralNeighbor, vec![
        "(9.999999999999999e-1)", "(1.0000000000000002e0)",
    ])]
    #[case::signed_float("-1.0", MutationOperator::FloatLiteralNeighbor, vec![
        "(-1.0000000000000002e0)", "(-9.999999999999999e-1)",
    ])]
    #[case::float_maximum("1.7976931348623157e308", MutationOperator::FloatLiteralNeighbor, vec![
        "(1.7976931348623155e308)",
    ])]
    #[case::negative_zero("-0.0", MutationOperator::FloatLiteralNeighbor, vec!["(-5e-324)", "(5e-324)"])]
    #[case::infinite("1e999", MutationOperator::FloatLiteralNeighbor, vec![])]
    #[case::complex("1.0j", MutationOperator::FloatLiteralNeighbor, vec![])]
    fn emits_exact_typed_neighbors(
        #[case] source: &str,
        #[case] operator: MutationOperator,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let mut operators = MutationOperatorSelection::default();
        operators.include(operator);
        let mut actual = Vec::new();

        emit(
            &parsed.syntax().body,
            source,
            &operators,
            true,
            &|| false,
            |range, replacement, kind| {
                let mut mutant = source.to_owned();
                mutant.replace_range(
                    usize::from(range.start())..usize::from(range.end()),
                    &replacement,
                );
                assert_eq!(kind, operator);
                ruff_python_parser::parse_expression(&mutant).unwrap();
                actual.push(mutant);
                true
            },
        );

        assert_eq!(actual, expected);
    }

    #[rstest::rstest]
    #[case::unsigned_zero("0x0", false, vec!["0x1"])]
    #[case::negative_zero("-0", false, vec!["1", "-1"])]
    #[case::zero_step("0b1", true, vec!["0b10"])]
    #[case::negative_zero_step("-1", true, vec!["-2"])]
    #[case::negative_zero_index("-1", false, vec!["0", "-2"])]
    fn index_and_slice_policies_preserve_zero_rules(
        #[case] source: &str,
        #[case] excludes_zero: bool,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let mut actual = Vec::new();

        emit_index_neighbors(
            &parsed.syntax().body,
            source,
            excludes_zero,
            &|| false,
            |_, replacement| {
                actual.push(replacement);
                true
            },
        );

        assert_eq!(actual, expected);
    }

    #[rstest::rstest]
    #[case::positive("1")]
    #[case::negative("-0x1")]
    fn integer_neighbors_require_operand_source(#[case] source: &str) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let mut count = 0;

        emit_index_neighbors(&parsed.syntax().body, "", false, &|| false, |_, _| {
            count += 1;
            true
        });

        assert_eq!(count, 0);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        #[test]
        fn signed_radix_neighbors_equal_exact_arithmetic(
            value in -i128::from(u64::MAX)..=i128::from(u64::MAX),
            radix in prop::sample::select(vec![2, 8, 10, 16]),
        ) {
            let magnitude = value.unsigned_abs();
            let (prefix, digits) = match radix {
                2 => ("0b", format!("{magnitude:b}")),
                8 => ("0o", format!("{magnitude:o}")),
                10 => ("", magnitude.to_string()),
                16 => ("0x", format!("{magnitude:x}")),
                _ => unreachable!(),
            };
            let sign = if value < 0 { "-" } else { "" };
            let source = format!("{sign}{prefix}{digits}");
            let parsed = ruff_python_parser::parse_expression(&source).unwrap();
            let operators = serde_json::from_value(serde_json::json!(["integer_literal_neighbor"])).unwrap();
            let mut actual = Vec::new();

            emit(&parsed.syntax().body, &source, &operators, true, &|| false, |_, replacement, _| {
                let number = &replacement[1..replacement.len() - 1];
                let digits = number.trim_start_matches('-').strip_prefix(prefix).unwrap();
                let magnitude = i128::from_str_radix(digits, radix).unwrap();
                actual.push(if number.starts_with('-') { -magnitude } else { magnitude });
                true
            });

            prop_assert_eq!(actual, [value - 1, value + 1]);
        }
    }

    #[test]
    fn cancellation_during_carry_prevents_emitting_unfinished_neighbor() {
        let source = format!("0x{}", "f".repeat(2_000));
        let parsed = ruff_python_parser::parse_expression(&source).unwrap();
        let polls = std::cell::Cell::new(0);
        let mut count = 0;

        emit_index_neighbors(
            &parsed.syntax().body,
            &source,
            false,
            &|| {
                polls.set(polls.get() + 1);
                polls.get() >= 2_050
            },
            |_, _| {
                count += 1;
                true
            },
        );

        assert_eq!(count, 0);
    }
}
