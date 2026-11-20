//! The exponent of an exact Decimal literal is the quantum used by quantize.

use ruff_python_ast::{Expr, Number, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

pub(super) const MAX_PRECISION: i64 = 999_999_999_999_999_999;
const MIN_EXPONENT: i64 = -1_999_999_999_999_999_997;

pub(super) fn signed_integer(expr: &Expr) -> Option<i64> {
    let (number, negative) = match expr {
        Expr::UnaryOp(unary) if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub) => {
            (unary.operand.as_ref(), unary.op == UnaryOp::USub)
        }
        _ => (expr, false),
    };
    let Number::Int(value) = &number.as_number_literal_expr()?.value else {
        return None;
    };
    let value = value.as_i64()?;
    if negative {
        value.checked_neg()
    } else {
        Some(value)
    }
}

pub(super) struct Quantum {
    pub(super) range: TextRange,
    exponent: i64,
    coefficient_size: i64,
    tuple_exponent: bool,
}

impl Quantum {
    pub(super) fn edit_range(expr: &Expr) -> Option<TextRange> {
        if let Expr::Tuple(tuple) = expr {
            let [_, _, exponent] = tuple.elts.as_slice() else {
                return None;
            };
            Some(exponent.range())
        } else {
            Some(expr.range())
        }
    }

    pub(super) fn from_literal(expr: &Expr, cancelled: &dyn Fn() -> bool) -> Option<Self> {
        let (range, exponent, coefficient_size, tuple_exponent) = match expr {
            Expr::StringLiteral(literal) => {
                let chars = literal
                    .value
                    .iter()
                    .map_while(|part| (!cancelled()).then(|| part.value.chars()))
                    .flatten();
                let (exponent, size) = string_exponent(chars, cancelled)?;
                (expr.range(), exponent, size, false)
            }
            Expr::Tuple(tuple) => {
                let [sign, coefficient, exponent] = tuple.elts.as_slice() else {
                    return None;
                };
                if !matches!(signed_integer(sign), Some(0 | 1)) {
                    return None;
                }
                let digits = &coefficient.as_tuple_expr()?.elts;
                let mut size = 0;
                for digit in digits {
                    if cancelled() {
                        return None;
                    }
                    let digit = signed_integer(digit)?;
                    if !(0..=9).contains(&digit) {
                        return None;
                    }
                    size += i64::from(digit != 0 || size != 0);
                }
                (
                    exponent.range(),
                    signed_integer(exponent)?,
                    size.max(1),
                    true,
                )
            }
            // Decimal(float) has its own FloatOperation trap and flag. Keep
            // that executable conversion outside the literal-quantum proof.
            _ if exact_integer(expr) => (expr.range(), 0, 1, false),
            _ => return None,
        };
        valid_exponent(exponent, coefficient_size).then_some(Self {
            range,
            exponent,
            coefficient_size,
            tuple_exponent,
        })
    }

    pub(super) fn shifted(&self, direction: i64) -> Option<String> {
        let exponent = self.exponent.checked_add(direction)?;
        let size = if self.tuple_exponent {
            self.coefficient_size
        } else {
            1
        };
        valid_exponent(exponent, size).then(|| {
            if self.tuple_exponent {
                exponent.to_string()
            } else {
                format!("'1e{exponent}'")
            }
        })
    }
}

pub(super) fn exact_integer(expr: &Expr) -> bool {
    match expr {
        Expr::NumberLiteral(literal) => matches!(literal.value, Number::Int(_)),
        Expr::BooleanLiteral(_) => true,
        Expr::UnaryOp(unary) if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub) => {
            matches!(unary.operand.as_ref(), Expr::NumberLiteral(literal) if matches!(literal.value, Number::Int(_)))
        }
        _ => false,
    }
}

fn valid_exponent(exponent: i64, coefficient_size: i64) -> bool {
    (MIN_EXPONENT..=MAX_PRECISION).contains(&exponent)
        && exponent
            .checked_add(coefficient_size - 1)
            .is_some_and(|adjusted| adjusted <= MAX_PRECISION)
}

fn string_exponent(
    chars: impl Iterator<Item = char>,
    cancelled: &dyn Fn() -> bool,
) -> Option<(i64, i64)> {
    // CPython strips whitespace at either end, then discards underscores.
    // Scan decoded parts directly, including the ignored characters, so a
    // long literal can be interrupted without materializing a joined string.
    let mut saw_nonspace = false;
    let mut trailing_space = false;
    let mut sign_allowed = true;
    let mut fractional = false;
    let mut fractional_digits = 0i64;
    let mut coefficient_size = 0i64;
    let mut has_digit = false;
    let mut exponent = None;
    let mut exponent_negative = false;
    let mut exponent_has_digit = false;
    for ch in chars {
        if cancelled() {
            return None;
        }
        if ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch) {
            trailing_space = saw_nonspace;
            continue;
        }
        if trailing_space {
            return None;
        }
        saw_nonspace = true;
        if ch == '_' {
            continue;
        }

        if let Some(power) = exponent {
            if !exponent_has_digit && sign_allowed && matches!(ch, '+' | '-') {
                exponent_negative = ch == '-';
            } else {
                let digit = i64::from(decimal_digit(ch)?);
                exponent = Some(i64::checked_add(i64::checked_mul(power, 10)?, digit)?);
                exponent_has_digit = true;
            }
        } else if sign_allowed && matches!(ch, '+' | '-') {
            // The coefficient sign does not influence its scale.
        } else if ch == '.' && !fractional {
            fractional = true;
        } else if has_digit && matches!(ch, 'e' | 'E') {
            exponent = Some(0);
            sign_allowed = true;
            continue;
        } else {
            let digit = decimal_digit(ch)?;
            has_digit = true;
            fractional_digits += i64::from(fractional);
            coefficient_size += i64::from(digit != 0 || coefficient_size != 0);
        }
        sign_allowed = false;
    }
    if !has_digit || (exponent.is_some() && !exponent_has_digit) {
        return None;
    }
    let exponent = exponent.unwrap_or_default();
    let exponent = if exponent_negative {
        exponent.checked_neg()?
    } else {
        exponent
    };
    Some((
        exponent.checked_sub(fractional_digits)?,
        coefficient_size.max(1),
    ))
}

fn decimal_digit(ch: char) -> Option<u32> {
    // Start of each ten-character decimal digit set in Unicode 16.0.
    // Some scripts place consecutive sets in the same Nd block.
    const DECIMAL_ZEROS: &[u32] = &[
        0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66, 0xde6,
        0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
        0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
        0xff10, 0x104a0, 0x10d30, 0x10d40, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450,
        0x114d0, 0x11650, 0x116c0, 0x116d0, 0x116da, 0x11730, 0x118e0, 0x11950, 0x11bf0, 0x11c50,
        0x11d50, 0x11da0, 0x11f50, 0x16130, 0x16a60, 0x16ac0, 0x16b50, 0x16d70, 0x1ccf0, 0x1d7ce,
        0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e5f1, 0x1e950, 0x1fbf0,
    ];
    if let Some(digit) = ch.to_digit(10) {
        return Some(digit);
    }
    let code = u32::from(ch);
    let previous = DECIMAL_ZEROS
        .partition_point(|start| *start <= code)
        .checked_sub(1)?;
    let digit = code - DECIMAL_ZEROS[previous];
    (digit <= 9).then_some(digit)
}
