//! Map decoded Python regex units back to the token bodies that produced them.
//! Cover edits across tokens without reconstructing their quotes or comments.
use super::super::source_escapes::{Decoded, decode};
use super::characters;
use ruff_python_ast::Expr;
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::ops::Range;

#[cfg(test)]
#[path = "literal/oracle_tests.rs"]
mod oracle_tests;

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

#[derive(Clone, Copy)]
pub(super) enum Change<'a> {
    Replace(&'a str),
    /// ASCII delimiters that can be written directly inside each token body.
    Surround {
        prefix: &'a str,
        suffix: &'a str,
    },
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
        #[cfg(test)]
        super::record_work(result.chars.len(), 0);
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
                match decode(
                    &self.source[..body.end],
                    &mut at,
                    self.bytes,
                    characters::named,
                )? {
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

    pub fn edit(&self, span: Range<usize>, change: Change<'_>) -> Option<(TextRange, String)> {
        let range = self.range_for(&span)?;
        let first = self.units.get(span.start).map_or(0, |u| u.token);
        let last = if span.is_empty() {
            first
        } else {
            self.units[span.end - 1].token
        };
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let (code, suffix) = match change {
            Change::Replace(text) => (text, None),
            Change::Surround { prefix, suffix } => (prefix, Some(suffix)),
        };
        if let Some(suffix) = suffix
            && first != last
            && self.tokens[first].raw
        {
            // A new prefix splits a backslash run crossing the start of the
            // span. An odd remainder would escape the first token's closing
            // quote. Write the decoded edit together inside that token.
            let tail = &self.source[start..self.tokens[first].body.end];
            if tail.len() % 2 == 1 && tail.bytes().all(|byte| byte == b'\\') {
                let mut text = code.to_owned();
                text.extend(self.chars[span.clone()].iter().copied());
                text.push_str(suffix);
                return self.edit(span, Change::Replace(&text));
            }
        }
        let mut out = String::new();
        let mut finish = end;
        let next_cont = self.continuations.partition_point(|r| r.end <= start);
        let mut cont = self.continuations[next_cont..].iter().peekable();
        if let Some(suffix) = suffix {
            out = format!("{code}{}{suffix}", &self.source[start..end]);
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
                let empty = (index != first || code.is_empty())
                    && left == token.body.start
                    && right == token.body.end;
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
        if first != last || finish != end {
            self.verify_cross_token(span, first..last + 1, edit, &out, code, suffix)?;
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
        suffix: Option<&str>,
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
                    .take(if suffix.is_some() { span.len() } else { 0 }),
            )
            .chain(suffix.unwrap_or_default().chars())
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
