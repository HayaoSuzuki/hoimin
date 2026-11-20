//! Lex Python bracket and escape atoms, including the separate numeric rules inside sets.
use super::pattern::{ASCII, IGNORE, Variant};

pub(super) enum Escape {
    Literal(u32),
    Category(char),
    Anchor(char),
    Ref(usize),
}

pub(super) fn escape(s: &[char], at: &mut usize, bytes: bool, class: bool) -> Option<Escape> {
    let c = *s.get(*at)?;
    *at += 1;
    let literal = match c {
        'a' => 7,
        'f' => 12,
        'n' => 10,
        'r' => 13,
        't' => 9,
        'v' => 11,
        'b' if class => 8,
        'd' | 'D' | 'w' | 'W' | 's' | 'S' => return Some(Escape::Category(c)),
        'A' | 'Z' | 'z' | 'b' | 'B' if !class => return Some(Escape::Anchor(c)),
        'x' | 'u' | 'U' if c == 'x' || !bytes => {
            let n = match c {
                'x' => 2,
                'u' => 4,
                _ => 8,
            };
            let mut value = 0u32;
            for digit in s.get(*at..*at + n)? {
                value = value.checked_mul(16)? + digit.to_digit(16)?;
            }
            *at += n;
            if value > 0x0010_ffff {
                return None;
            }
            value
        }
        'N' if !bytes => {
            if s.get(*at) != Some(&'{') {
                return None;
            }
            let end = s[*at + 1..].iter().take(128).position(|&c| c == '}')? + *at + 1;
            let name = s[*at + 1..end].iter().collect::<String>();
            *at = end + 1;
            u32::from(super::characters::named(&name)?)
        }
        '0'..='9' => {
            let octal = |c: &char| matches!(c, '0'..='7');
            let triple = octal(&c) && s.get(*at..*at + 2).is_some_and(|v| v.iter().all(octal));
            if c == '0' || class || triple {
                let mut n = c.to_digit(8)?;
                for _ in 0..2 {
                    if let Some(d) = s.get(*at).and_then(|c| c.to_digit(8)) {
                        n = 8 * n + d;
                        *at += 1;
                    } else {
                        break;
                    }
                }
                if n > 255 {
                    return None;
                }
                n
            } else {
                let mut n = c.to_digit(10)? as usize;
                if let Some(d) = s.get(*at).and_then(|c| c.to_digit(10)) {
                    n = 10 * n + d as usize;
                    *at += 1;
                }
                return Some(Escape::Ref(n));
            }
        }
        c if c.is_ascii_alphabetic() => return None,
        c => u32::from(c),
    };
    Some(Escape::Literal(literal))
}

pub(super) fn fold_sensitive(n: u32) -> bool {
    char::from_u32(n).is_some_and(|c| {
        matches!(c, 'i' | 'I' | 'k' | 'K' | 's' | 'S')
            || !c.is_ascii()
                && (c.to_uppercase().ne(std::iter::once(c))
                    || c.to_lowercase().ne(std::iter::once(c)))
    })
}

static CASED: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();

// Index long Unicode intervals once. Repeated uncased ranges then need only a binary search.
fn interval_has_fold(lo: u32, hi: u32, cancelled: &dyn Fn() -> bool) -> Option<bool> {
    if hi - lo < 256 {
        return Some((lo..=hi).any(fold_sensitive));
    }
    if CASED.get().is_none() {
        let mut points = vec![];
        for n in 128..=0x0010_ffff {
            if n % 1024 == 0 && cancelled() {
                return None;
            }
            if fold_sensitive(n) {
                points.push(n);
            }
        }
        let _ = CASED.set(points);
    }
    let points = CASED.get()?;
    Some(
        points
            .get(points.partition_point(|&n| n < lo))
            .is_some_and(|&n| n <= hi),
    )
}

pub(super) fn category(c: char, flags: u16) -> Variant {
    if flags & ASCII != 0 {
        return Variant::Scope('u');
    }
    let text = match c {
        'd' => "[0-9]",
        'D' => "[^0-9]",
        'w' if flags & IGNORE != 0 => "(?a:[A-Za-z0-9_])",
        'W' if flags & IGNORE != 0 => "(?a:[^A-Za-z0-9_])",
        'w' => "[A-Za-z0-9_]",
        'W' => "[^A-Za-z0-9_]",
        's' => r"[\x09-\x0d\x20]",
        _ => r"[^\x09-\x0d\x20]",
    };
    Variant::Text(text)
}

fn member(s: &[char], at: &mut usize, bytes: bool) -> Option<Escape> {
    let c = *s.get(*at)?;
    *at += 1;
    if c == '\\' {
        escape(s, at, bytes, true)
    } else {
        Some(Escape::Literal(u32::from(c)))
    }
}

pub(super) struct Set {
    pub variant: Option<Variant>,
    pub sensitive: bool,
    pub may_fold: bool,
}

pub(super) fn class(
    s: &[char],
    at: &mut usize,
    bytes: bool,
    flags: u16,
    cancelled: &dyn Fn() -> bool,
) -> Option<Set> {
    if s.get(*at) == Some(&'[') {
        return None;
    }
    let negative = s.get(*at) == Some(&'^');
    *at += usize::from(negative);
    let mut any = false;
    let mut ascii = 0u128;
    let mut wholly_ascii = true;
    let mut sensitive = false;
    let mut may_fold = negative;
    let mut categories = 0u8;
    loop {
        if cancelled() {
            return None;
        }
        let c = *s.get(*at)?;
        if any && c == ']' {
            *at += 1;
            break;
        }
        if any && "-&~|".contains(c) && s.get(*at + 1) == Some(&c) {
            return None;
        }
        any = true;
        let first = member(s, at, bytes)?;
        let range = s.get(*at) == Some(&'-') && s.get(*at + 1) != Some(&']');
        let (lo, hi) = match first {
            Escape::Category(c) if !range => {
                wholly_ascii = false;
                may_fold |= !matches!(c, 'd' | 's');
                categories |= 1 << "dDwWsS".find(c)?;
                sensitive = true;
                continue;
            }
            Escape::Literal(lo) if range => {
                *at += 1;
                if s.get(*at) == Some(&'-') {
                    return None;
                }
                let Escape::Literal(hi) = member(s, at, bytes)? else {
                    return None;
                };
                if hi < lo {
                    return None;
                }
                (lo, hi)
            }
            Escape::Literal(lo) => (lo, lo),
            _ => return None,
        };
        if hi >= 128 {
            wholly_ascii = false;
        }
        if lo < 128 {
            let top = hi.min(127);
            ascii |= (u128::MAX << lo) & (u128::MAX >> (127 - top));
        }
        if !bytes {
            let folds = [73, 75, 83, 105, 107, 115]
                .into_iter()
                .any(|n| (lo..=hi).contains(&n))
                || (hi >= 128 && interval_has_fold(lo, hi, cancelled)?);
            may_fold |= folds;
            sensitive |= flags & IGNORE != 0 && folds;
        }
    }
    if bytes {
        return Some(Set {
            variant: None,
            sensitive: false,
            may_fold: false,
        });
    }
    if flags & IGNORE != 0 {
        let letters = (1u128 << 26) - 1;
        let paired = ((ascii >> 65) | (ascii >> 97)) & letters;
        ascii |= (paired << 65) | (paired << 97);
    }
    let exact = ascii_category(ascii, wholly_ascii, negative);
    if categories & (categories >> 1) & 0b01_0101 != 0 {
        sensitive = false;
    }
    let variant = if let Some(code) = exact {
        Some(if flags & ASCII != 0 {
            Variant::Unicode(code)
        } else {
            Variant::Text(code)
        })
    } else if sensitive {
        Some(Variant::Scope(if flags & ASCII != 0 { 'u' } else { 'a' }))
    } else {
        None
    };
    Some(Set {
        variant,
        sensitive,
        may_fold,
    })
}

fn ascii_category(ascii: u128, wholly_ascii: bool, negative: bool) -> Option<&'static str> {
    let digits = ((1u128 << 10) - 1) << 48;
    let letters = (1u128 << 26) - 1;
    let word = digits | (letters << 65) | (letters << 97) | (1u128 << 95);
    let space = (31u128 << 9) | (1u128 << 32);
    if wholly_ascii {
        match (ascii, negative) {
            (v, false) if v == digits => Some(r"\d"),
            (v, true) if v == digits => Some(r"\D"),
            (v, false) if v == word => Some(r"\w"),
            (v, true) if v == word => Some(r"\W"),
            (v, false) if v == space => Some(r"\s"),
            (v, true) if v == space => Some(r"\S"),
            _ => None,
        }
    } else {
        None
    }
}
