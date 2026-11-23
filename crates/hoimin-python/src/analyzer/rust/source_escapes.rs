//! Decode one Python source escape. Each consumer decides how to map the
//! decoded units and which Unicode character names it trusts.

pub(super) enum Decoded {
    Character(char),
    Preserved(char),
    Continuation,
}

/// Decode the escape starting just after a backslash, advancing the byte cursor.
pub(super) fn decode(
    source: &str,
    at: &mut usize,
    bytes: bool,
    named: impl FnOnce(&str) -> Option<char>,
) -> Option<Decoded> {
    let escaped = source[*at..].chars().next()?;
    *at += escaped.len_utf8();
    let value = match escaped {
        '\n' => return Some(Decoded::Continuation),
        '\r' => {
            if source.as_bytes().get(*at) == Some(&b'\n') {
                *at += 1;
            }
            return Some(Decoded::Continuation);
        }
        '\\' | '\'' | '"' => escaped,
        'a' => '\x07',
        'b' => '\x08',
        'f' => '\x0c',
        't' => '\t',
        'n' => '\n',
        'r' => '\r',
        'v' => '\x0b',
        '0'..='7' => {
            let mut number = escaped.to_digit(8)?;
            for _ in 0..2 {
                let Some(digit) = source
                    .as_bytes()
                    .get(*at)
                    .and_then(|b| char::from(*b).to_digit(8))
                else {
                    break;
                };
                number = 8 * number + digit;
                *at += 1;
            }
            char::from_u32(if bytes { number & 255 } else { number })?
        }
        'x' | 'u' | 'U' if escaped == 'x' || !bytes => {
            let len = match escaped {
                'x' => 2,
                'u' => 4,
                _ => 8,
            };
            let code = u32::from_str_radix(source.get(*at..*at + len)?, 16).ok()?;
            *at += len;
            char::from_u32(code)?
        }
        'N' if !bytes => {
            if source.as_bytes().get(*at) != Some(&b'{') {
                return None;
            }
            let end = *at
                + 1
                + source.as_bytes()[*at + 1..]
                    .iter()
                    .take(128)
                    .position(|&b| b == b'}')?;
            let symbol = named(&source[*at + 1..end])?;
            *at = end + 1;
            symbol
        }
        _ => return Some(Decoded::Preserved(escaped)),
    };
    Some(Decoded::Character(value))
}
