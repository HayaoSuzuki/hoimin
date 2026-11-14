//! Collision identities for the actual key edits: bool flips and complex conjugation.
//!
//! Real-only keys can collide with a bool replacement only at exactly 0 or 1.
//! A nonzero imaginary part cannot equal a real-only key. Conjugating zero does
//! not change equality, so cannot introduce a duplicate in a valid original.
//! Thus ordinary large integer keys never need conversion to floating point.
use std::collections::HashSet;

use num_bigint::BigUint;
use num_traits::ToPrimitive;
use ruff_python_ast::{Expr, Number, Operator, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Identity {
    Boolean(bool),
    Complex(u64, u64),
}

// Legal pattern literal syntax cannot construct NaN. After normalizing signed
// zero, IEEE bits therefore express equality of the complex components.
#[expect(clippy::float_cmp, reason = "Python key equality must be exact")]
fn identity(real: f64, imag: f64) -> Option<Identity> {
    if imag == 0.0 {
        if real == 0.0 {
            Some(Identity::Boolean(false))
        } else if real == 1.0 {
            Some(Identity::Boolean(true))
        } else {
            None
        }
    } else {
        Some(Identity::Complex(
            if real == 0.0 { 0 } else { real.to_bits() },
            imag.to_bits(),
        ))
    }
}

/// Only complex construction converts an integer real part, as `CPython` does.
/// `BigUint` handles original radix/underscore spelling and ties-to-even rounding.
/// Integer overflow is not a foldable Python complex literal; float infinity is.
fn complex_real(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::UnaryOp(unary) if unary.op == UnaryOp::USub => Some(-complex_real(&unary.operand)?),
        Expr::NumberLiteral(literal) => match &literal.value {
            Number::Float(value) => Some(*value),
            Number::Int(value) => {
                let result = if let Some(value) = value.as_u64() {
                    // Every u64 magnitude converts to a finite f64.
                    value.to_f64().expect("u64 converts to finite f64")
                } else {
                    let text = value.to_string().replace('_', "");
                    let (digits, radix) = match text.get(..2) {
                        Some("0x" | "0X") => (&text[2..], 16),
                        Some("0o" | "0O") => (&text[2..], 8),
                        Some("0b" | "0B") => (&text[2..], 2),
                        _ => (text.as_str(), 10),
                    };
                    let digits = digits.trim_start_matches('0');
                    // These generous digit bounds include every finite f64 integer
                    // conversion. Boundary rounding remains the library's job.
                    let max_digits = match radix {
                        2 => 1024,
                        8 => 342,
                        16 => 256,
                        _ => 309,
                    };
                    if digits.len() > max_digits {
                        return None;
                    }
                    // Ruff's Int preserves a validated integer token; removing
                    // its radix prefix, underscores and leading zeroes leaves
                    // nonempty valid digits because this magnitude exceeds u64.
                    let integer = BigUint::parse_bytes(digits.as_bytes(), radix)
                        .expect("parsed integer has valid digits");
                    // BigUint's ToPrimitive returns Some, using infinity for an
                    // overflowing conversion. The finite check below rejects it.
                    integer
                        .to_f64()
                        .expect("BigUint converts to f64 or infinity")
                };
                result.is_finite().then_some(result)
            }
            Number::Complex { .. } => None,
        },
        _ => None,
    }
}

fn complex(expr: &Expr) -> Option<(f64, f64)> {
    match expr {
        Expr::NumberLiteral(literal) => match literal.value {
            Number::Complex { real, imag } => Some((real, imag)),
            _ => None,
        },
        Expr::UnaryOp(unary) if unary.op == UnaryOp::USub => {
            let (real, imag) = complex(&unary.operand)?;
            Some((-real, -imag))
        }
        Expr::BinOp(binary) if matches!(binary.op, Operator::Add | Operator::Sub) => {
            let real = complex_real(&binary.left)?;
            let (_, imag) = complex(&binary.right)?;
            Some((
                real,
                if binary.op == Operator::Sub {
                    -imag
                } else {
                    imag
                },
            ))
        }
        _ => None,
    }
}

fn key_identity(expr: &Expr) -> Option<Identity> {
    match expr {
        Expr::BooleanLiteral(value) => Some(Identity::Boolean(value.value)),
        Expr::NumberLiteral(value) => match &value.value {
            Number::Int(value) => match value.as_u64() {
                Some(0) => Some(Identity::Boolean(false)),
                Some(1) => Some(Identity::Boolean(true)),
                _ => None,
            },
            Number::Float(value) => identity(*value, 0.0),
            Number::Complex { real, imag } => identity(*real, *imag),
        },
        Expr::UnaryOp(unary) if unary.op == UnaryOp::USub => {
            // Negating an integer can equal a boolean target only for zero.
            if let Expr::NumberLiteral(value) = unary.operand.as_ref()
                && let Number::Int(value) = &value.value
            {
                return (value.as_u64() == Some(0)).then_some(Identity::Boolean(false));
            }
            complex(expr)
                .and_then(|(real, imag)| identity(real, imag))
                .or_else(|| complex_real(expr).and_then(|real| identity(real, 0.0)))
        }
        _ => complex(expr).and_then(|(real, imag)| identity(real, imag)),
    }
}

/// Normalize each key at most twice and use one hash lookup per editable key.
/// Each returned range contains exactly the bool token or complex separator.
pub(super) fn colliding_edits(keys: &[Expr]) -> Vec<TextRange> {
    let identities: HashSet<_> = keys.iter().filter_map(key_identity).collect();
    keys.iter()
        .filter_map(|key| {
            let (replacement, range) = match key {
                Expr::BooleanLiteral(value) => (Identity::Boolean(!value.value), key.range()),
                Expr::BinOp(binary) => {
                    let (real, imag) = complex(key)?;
                    if imag == 0.0 {
                        return None;
                    }
                    (
                        // Negation preserves the nonzero imaginary component,
                        // which always has a complex identity.
                        identity(real, -imag).expect("nonzero imaginary identity"),
                        TextRange::new(binary.left.end(), binary.right.start()),
                    )
                }
                _ => return None,
            };
            identities.contains(&replacement).then_some(range)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_parser::parse_module;

    #[rstest::rstest]
    #[case::value_9_repeat_400_1(0)]
    #[case::format_0x1_0_repeat_256_2(1)]
    #[case::format_0o1_0_repeat_342_3(2)]
    #[case::format_0b1_0_repeat_1024_4(3)]
    fn impossible_complex_real_overflow_is_not_a_literal_identity(#[case] scenario_index: usize) {
        let digits = [
            "9".repeat(400),
            format!("0x1{}", "0".repeat(256)),
            format!("0o1{}", "0".repeat(342)),
            format!("0b1{}", "0".repeat(1024)),
        ]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

        let source = format!("{digits}+1j");
        let parsed = parse_module(&source).unwrap();
        let statement = parsed.syntax().body[0].as_expr_stmt().unwrap();

        assert!(complex(&statement.value).is_none());
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[rstest::rstest]
    #[case("1.5", Some(1.5))]
    #[case("18446744073709551616", Some(18_446_744_073_709_551_616.0))]
    #[case("0x1_0000_0000_0000_0000", Some(18_446_744_073_709_551_616.0))]
    #[case("-18446744073709551616", Some(-18_446_744_073_709_551_616.0))]
    #[case("-3", Some(-3.0))]
    #[case("1j", None)]
    #[case("-'value'", None)]
    #[case("'value'", None)]
    fn complex_real_part_requires_a_real_numeric_literal(
        #[case] source: &str,
        #[case] expected: Option<f64>,
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();

        let actual = complex_real(&parsed.syntax().body).map(f64::to_bits);

        assert_eq!(actual, expected.map(f64::to_bits));
    }

    #[test]
    fn conjugation_detects_a_collision_with_a_pure_imaginary_key() {
        let source = "(0 - 1j, 1j)";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let keys = &parsed.syntax().body.as_tuple_expr().unwrap().elts;
        let collisions = colliding_edits(keys);

        let actual = collisions.len();
        let actual_2 =
            &source[usize::from(collisions[0].start())..usize::from(collisions[0].end())];

        assert_eq!(actual, 1);
        assert_eq!(actual_2, " - ");
    }
    #[test]
    fn real_integer_within_the_digit_bound_must_still_be_finite() {
        let source = "9".repeat(309);
        let parsed = ruff_python_parser::parse_expression(&source).unwrap();

        let actual = complex_real(&parsed.syntax().body);

        assert_eq!(actual, None);
    }

    #[rstest::rstest]
    #[case("(1 + 2, True)")]
    #[case("(1 + 0j, True)")]
    fn real_only_and_noncomplex_binary_keys_do_not_gain_a_conjugate_edit(#[case] source: &str) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let keys = &parsed.syntax().body.as_tuple_expr().unwrap().elts;

        let actual = colliding_edits(keys);

        assert_eq!(actual, Vec::<TextRange>::new());
    }
    #[rstest::rstest]
    #[case("True", Some(Identity::Boolean(true)))]
    #[case("False", Some(Identity::Boolean(false)))]
    #[case("0", Some(Identity::Boolean(false)))]
    #[case("1", Some(Identity::Boolean(true)))]
    #[case("2", None)]
    #[case("18446744073709551616", None)]
    #[case("0.0", Some(Identity::Boolean(false)))]
    #[case("1.0", Some(Identity::Boolean(true)))]
    #[case("2.0", None)]
    #[case("-0", Some(Identity::Boolean(false)))]
    #[case("-1", None)]
    #[case("-0.0", Some(Identity::Boolean(false)))]
    #[case("-2.0", None)]
    #[case("1j", Some(Identity::Complex(0, 1.0_f64.to_bits())))]
    #[case("-2j", Some(Identity::Complex(0, (-2.0_f64).to_bits())))]
    #[case("1+2j", Some(Identity::Complex(1.0_f64.to_bits(), 2.0_f64.to_bits())))]
    #[case("None", None)]
    #[case("-'value'", None)]
    fn mapping_key_identity_matches_python_numeric_equality(
        #[case] source: &str,
        #[case] expected: Option<Identity>,
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();

        let actual = key_identity(&parsed.syntax().body) == expected;

        assert!(actual, "{source}");
    }
}
