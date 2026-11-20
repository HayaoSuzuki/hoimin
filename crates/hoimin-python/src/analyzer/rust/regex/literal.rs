//! Map decoded Python regex units back to the token bodies that produced them.
//! Cover edits across tokens without reconstructing their quotes or comments.
use super::pattern::Variant;
use ruff_python_ast::Expr;
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::ops::Range;

#[derive(Clone, Copy)]
struct Unit {
    start: usize,
    end: usize,
    token: usize,
}
struct Token {
    body: Range<usize>,
    whole: Range<usize>,
    decoded: Range<usize>,
    raw: bool,
    quote: u8,
    single: bool,
}

pub(super) struct Literal<'a> {
    pub chars: Vec<char>,
    pub bytes: bool,
    pub range: TextRange,
    units: Vec<Unit>,
    tokens: Vec<Token>,
    continuations: Vec<Range<usize>>,
    source: &'a str,
    cancelled: &'a dyn Fn() -> bool,
}
fn text_range(start: usize, end: usize) -> Option<TextRange> {
    Some(TextRange::new(
        TextSize::try_from(start).ok()?,
        TextSize::try_from(end).ok()?,
    ))
}
impl<'a> Literal<'a> {
    pub fn read(
        expression: &Expr,
        source: &'a str,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Option<Self> {
        let mut result = Self {
            chars: vec![],
            bytes: expression.is_bytes_literal_expr(),
            range: expression.range(),
            units: vec![],
            tokens: vec![],
            continuations: vec![],
            source,
            cancelled,
        };
        match expression {
            Expr::BytesLiteral(e) => {
                for part in &e.value {
                    result.token(
                        part.range(),
                        part.value.iter().copied().map(char::from),
                        cancelled,
                    )?;
                }
            }
            Expr::StringLiteral(e) => {
                for part in &e.value {
                    result.token(part.range(), part.value.chars(), cancelled)?;
                }
            }
            _ => return None,
        }
        Some(result)
    }

    fn token(
        &mut self,
        range: TextRange,
        mut decoded: impl Iterator<Item = char>,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<()> {
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let text = &self.source[start..end];
        let quote = text.find(['\'', '"'])?;
        let raw = text[..quote].to_ascii_lowercase().contains('r');
        let q = text.as_bytes()[quote];
        let quotes = if text.as_bytes().get(quote..quote + 3) == Some(&[q; 3]) {
            3
        } else {
            1
        };
        let body = start + quote + quotes..end - quotes;
        let token = self.tokens.len();
        let mut at = body.start;
        let decoded_start = self.chars.len();
        while at < body.end {
            if cancelled() {
                return None;
            }
            let from = at;
            let mut c = self.source[at..body.end].chars().next()?;
            at += c.len_utf8();
            if c == '\r' {
                if self.source.as_bytes().get(at) == Some(&b'\n') {
                    at += 1;
                }
                c = '\n';
            }
            if c == '\\' && !raw {
                match python_escape(&self.source[..body.end], &mut at, self.bytes)? {
                    Decoded::Character(value) => c = value,
                    Decoded::Continuation => {
                        self.continuations.push(from..at);
                        continue;
                    }
                    Decoded::Preserved(value) => {
                        if decoded.next() != Some('\\') || decoded.next() != Some(value) {
                            return None;
                        }
                        self.units.extend([
                            Unit {
                                start: from,
                                end: from + 1,
                                token,
                            },
                            Unit {
                                start: from + 1,
                                end: at,
                                token,
                            },
                        ]);
                        self.chars.extend(['\\', value]);
                        continue;
                    }
                }
            }
            let actual = decoded.next()?;
            if actual != c {
                // Ruff can retain the file's physical CRLF inside raw tokens.
                if c != '\n'
                    || actual != '\r'
                    || (self.source[from..at].ends_with("\r\n") && decoded.next() != Some('\n'))
                {
                    return None;
                }
            }
            self.units.push(Unit {
                start: from,
                end: at,
                token,
            });
            self.chars.push(c);
        }
        self.tokens.push(Token {
            body,
            whole: start..end,
            decoded: decoded_start..self.chars.len(),
            raw,
            quote: q,
            single: quotes == 1,
        });
        decoded.next().is_none().then_some(())
    }

    pub fn range_for(&self, span: &Range<usize>) -> Option<TextRange> {
        if span.is_empty() {
            let at = self
                .units
                .get(span.start)
                .map(|u| u.start)
                .or_else(|| self.tokens.last().map(|t| t.body.end))?;
            return text_range(at, at);
        }
        text_range(
            self.units.get(span.start)?.start,
            self.units.get(span.end - 1)?.end,
        )
    }

    pub fn edit(&self, span: Range<usize>, variant: Variant) -> Option<(TextRange, String)> {
        let range = self.range_for(&span)?;
        let first = self.units.get(span.start).map_or(0, |u| u.token);
        let last = if span.is_empty() {
            first
        } else {
            self.units[span.end - 1].token
        };
        // Inserting a mode opener immediately before the last raw backslash
        // leaves an odd run before the closing quote. Carry the whole scoped
        // escape in that token and retain the following literal as an empty piece.
        if first != last
            && self.tokens[first].raw
            && self.chars[span.start] == '\\'
            && let Variant::Scope(mode) = variant
        {
            let code = match (mode, self.chars[span.start + 1]) {
                ('a', 'b') => r"(?a:\b)",
                ('a', 'B') => r"(?a:\B)",
                ('u', 'b') => r"(?u:\b)",
                ('u', 'B') => r"(?u:\B)",
                ('u', 'd') => r"(?u:\d)",
                ('u', 'D') => r"(?u:\D)",
                ('u', 'w') => r"(?u:\w)",
                ('u', 'W') => r"(?u:\W)",
                ('u', 's') => r"(?u:\s)",
                ('u', 'S') => r"(?u:\S)",
                _ => return None,
            };
            return self.edit(span, Variant::Text(code));
        }
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let (code, wrap) = match variant {
            Variant::Text(s) => (s.to_owned(), false),
            Variant::Unicode(s) => (format!("(?u:{s})"), false),
            Variant::Scope(mode) => (format!("(?{mode}:"), true),
        };
        let mut out = String::new();
        let mut finish = end;
        let next_cont = self.continuations.partition_point(|r| r.end <= start);
        let mut cont = self.continuations[next_cont..].iter().peekable();
        if wrap {
            out = format!("{code}{})", &self.source[start..end]);
        } else {
            for index in first..=last {
                if (self.cancelled)() {
                    return None;
                }
                let token = &self.tokens[index];
                let left = if index == first {
                    start
                } else {
                    token.body.start
                };
                let right = if index == last { end } else { token.body.end };
                if index == first {
                    for c in code.chars() {
                        if !token.raw && c == '\\' {
                            out.push('\\');
                        }
                        out.push(c);
                    }
                }
                while let Some(r) = cont.next_if(|r| r.start >= left && r.end <= right) {
                    out.push_str(&self.source[r.clone()]);
                }
                let empty = index != first && left == token.body.start && right == token.body.end;
                let after = token.whole.end;
                let space = empty
                    && token.single
                    && self.source.as_bytes().get(after) == Some(&token.quote);
                if index < last {
                    out.push_str(&self.source[right..after]);
                    if space {
                        out.push(' ');
                    }
                    out.push_str(&self.source[after..self.tokens[index + 1].body.start]);
                } else if space {
                    out.push_str(&self.source[right..after]);
                    out.push(' ');
                    finish = after;
                }
            }
        }
        let edit = text_range(start, finish)?;
        if first != last {
            self.verify_cross_token(span, first..last + 1, edit, &out, &code, wrap)?;
        }
        Some((edit, out))
    }
    fn verify_cross_token(
        &self,
        span: Range<usize>,
        tokens: Range<usize>,
        edit: TextRange,
        out: &str,
        code: &str,
        wrap: bool,
    ) -> Option<()> {
        let (start, finish) = (usize::from(edit.start()), usize::from(edit.end()));
        let (first, last) = (tokens.start, tokens.end - 1);
        // Parse just the touched tokens plus one neighbor on either side.
        // This also validates raw trailing backslashes and triple-quote boundaries.
        let left = &self.tokens[first.saturating_sub(1)];
        let right = &self.tokens[(last + 1).min(self.tokens.len() - 1)];
        let probe = format!(
            "({}{}{})",
            &self.source[left.whole.start..start],
            out,
            &self.source[finish..right.whole.end]
        );
        let parsed = ruff_python_parser::parse_expression(&probe).ok()?;
        let decoded = Literal::read(&parsed.syntax().body, &probe, self.cancelled)?.chars;
        let expected = self.chars[left.decoded.start..span.start]
            .iter()
            .copied()
            .chain(code.chars())
            .chain(
                self.chars[span.clone()]
                    .iter()
                    .copied()
                    .take(if wrap { span.len() } else { 0 }),
            )
            .chain(wrap.then_some(')'))
            .chain(self.chars[span.end..right.decoded.end].iter().copied());
        if !decoded.into_iter().eq(expected) {
            return None;
        }
        Some(())
    }
    pub fn at_reference(&self, range: TextRange, edit: &str) -> String {
        let l = usize::from(self.range.start());
        let r = usize::from(self.range.end());
        #[cfg(test)]
        super::record_materialized(
            r - l + edit.len() - (usize::from(range.end()) - usize::from(range.start())),
        );
        format!(
            "{}{edit}{}",
            &self.source[l..usize::from(range.start())],
            &self.source[usize::from(range.end())..r]
        )
    }
}

enum Decoded {
    Character(char),
    Preserved(char),
    Continuation,
}

fn python_escape(source: &str, at: &mut usize, bytes: bool) -> Option<Decoded> {
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
            let symbol = super::characters::named(&source[*at + 1..end])?;
            *at = end + 1;
            symbol
        }
        _ => return Some(Decoded::Preserved(escaped)),
    };
    Some(Decoded::Character(value))
}
