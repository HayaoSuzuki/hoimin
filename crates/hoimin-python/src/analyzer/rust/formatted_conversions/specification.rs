//! The built-in format mini-language, with interpolation represented by holes.
use super::super::source_escapes::{Decoded, decode};
use super::source_text;
use ruff_python_ast::{
    ConversionFlag, Expr, InterpolatedElement, InterpolatedStringElement,
    InterpolatedStringFormatSpec, Number, UnaryOp,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::ops::Range;

const ALIGNMENTS: &str = "<>^=";
const SIGNS: &str = "+- ";
const GROUPINGS: &str = ",_";
const INTEGER_TYPES: &str = "bdoxXn";
const FLOAT_TYPES: &str = "eEfFgGn%";

#[derive(Clone, Copy)]
enum Part {
    Literal(char, Option<TextRange>),
    Field,
}
impl Part {
    fn spelling(self) -> Option<char> {
        match self {
            Self::Literal(ch, _) => Some(ch),
            Self::Field => None,
        }
    }

    fn source(self) -> Option<TextRange> {
        match self {
            Self::Literal(_, source) => source,
            Self::Field => None,
        }
    }
}

#[derive(Clone, Copy)]
struct OptionSite {
    ch: char,
    range: Option<TextRange>,
}

#[derive(Clone, Copy)]
struct Options {
    alignment: Option<OptionSite>,
    sign: Option<OptionSite>,
    negative_zero: bool,
    alternate: bool,
    zero_padding: bool,
    grouping: Option<OptionSite>,
    fractional_grouping: Option<OptionSite>,
    presentation: Option<OptionSite>,
}
impl Options {
    fn valid(&self, subject: Subject, has_precision: bool) -> bool {
        let presentation = self.presentation.map(|part| part.ch);
        let code = presentation.unwrap_or_default();
        if (subject == Subject::String && !matches!(presentation, None | Some('s')))
            || (matches!(subject, Subject::Float | Subject::Complex)
                && (integer_type(code) || code == 's'))
            || (subject == Subject::Integer && code == 's')
        {
            return false;
        }
        if code == 's' || subject == Subject::String {
            return self.sign.is_none()
                && !self.negative_zero
                && !self.alternate
                && self.grouping.is_none()
                && self.fractional_grouping.is_none()
                && self.alignment.is_none_or(|part| part.ch != '=');
        }
        if subject == Subject::Complex
            && (code == '%'
                || self.zero_padding
                || self.alignment.is_some_and(|part| part.ch == '='))
        {
            return false;
        }
        if (integer_type(code)
            || subject == Subject::Integer && matches!(presentation, None | Some('n')))
            && (has_precision || self.negative_zero || self.fractional_grouping.is_some())
        {
            return false;
        }
        if code == 'c' && (self.sign.is_some() || self.alternate) {
            return false;
        }
        if code == 'n' && (self.grouping.is_some() || self.fractional_grouping.is_some()) {
            return false;
        }
        match self.grouping.map(|part| part.ch) {
            Some(_) if code == 'c' => false,
            Some(',') if "boxX".contains(code) => false,
            _ => true,
        }
    }
}

fn integer_type(ch: char) -> bool {
    "bcdoxX".contains(ch)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Subject {
    Unknown,
    String,
    Integer,
    Float,
    Complex,
}
impl Subject {
    fn of(field: &InterpolatedElement) -> Self {
        let mut expression = field.expression.as_ref();
        if field.conversion != ConversionFlag::None
            || matches!(expression, Expr::FString(_) | Expr::StringLiteral(_))
        {
            return Self::String;
        }
        while let Expr::UnaryOp(unary) = expression
            && matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub)
        {
            expression = &unary.operand;
        }
        match expression {
            Expr::BooleanLiteral(_) => Self::Integer,
            Expr::NumberLiteral(value) => match value.value {
                Number::Int(_) => Self::Integer,
                Number::Float(_) => Self::Float,
                Number::Complex { .. } => Self::Complex,
            },
            _ => Self::Unknown,
        }
    }
}

/// A parsed standard spec. Static options can be edited independently of the
/// dynamic fields, which stay in place and retain their evaluation order.
pub(super) struct Format {
    parts: Vec<Part>,
    options: Options,
    width: Range<usize>,
    precision: Range<usize>,
    value_neighbors: bool,
}
impl Format {
    pub(super) fn parse(
        spec: &InterpolatedStringFormatSpec,
        source: &str,
        raw: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Self> {
        let mut parts = Vec::new();
        let mut fields = 0;
        let mut unescaped = true;
        for element in &spec.elements {
            if cancelled() {
                return None;
            }
            match element {
                InterpolatedStringElement::Interpolation(_) => {
                    fields += 1;
                    parts.push(Part::Field);
                }
                InterpolatedStringElement::Literal(literal) => {
                    let text = source_text(source, literal.range())?;
                    unescaped &= text == literal.value.as_ref();
                    append_literal(
                        &mut parts,
                        text,
                        &literal.value,
                        literal.start(),
                        raw,
                        cancelled,
                    )?;
                }
            }
        }
        let mut parser = Parser {
            parts: &parts,
            at: 0,
            cancelled,
        };
        if parts
            .get(1)
            .and_then(|part| part.spelling())
            .is_some_and(|ch| ALIGNMENTS.contains(ch))
        {
            parser.at = 1;
        }
        let zero_fill = parser.at == 1 && parts[0].spelling() == Some('0');
        let alignment = parser.option(ALIGNMENTS);
        let sign = parser.option(SIGNS);
        let negative_zero = parser.option("z").is_some();
        let alternate = parser.option("#").is_some();
        let zero_padding = parser.option("0").is_some() || zero_fill;
        let width = parser.digits()?;
        let grouping = parser.option(GROUPINGS);
        let (precision, fractional_grouping) = if parser.option(".").is_some() {
            let precision = parser.digits()?;
            let grouping = parser.option(GROUPINGS);
            if precision.is_empty() && grouping.is_none() {
                return None;
            }
            (precision, grouping)
        } else {
            (0..0, None)
        };
        let presentation = parser.option("bcdeEfFgGnosxX%");
        if parser.at != parts.len() {
            return None;
        }
        // The earlier width/precision operator edits simple numeric specs.
        // Option swaps can use additional dynamic or escaped source fragments.
        let value_neighbors =
            unescaped && fields <= 1 && (fractional_grouping.is_none() || !precision.is_empty());
        Some(Self {
            parts,
            options: Options {
                alignment,
                sign,
                negative_zero,
                alternate,
                zero_padding,
                grouping,
                fractional_grouping,
                presentation,
            },
            width,
            precision,
            value_neighbors,
        })
    }

    pub(super) fn emit_values(
        &self,
        cancelled: &dyn Fn() -> bool,
        mut emit: impl FnMut(TextRange, String) -> bool,
    ) -> bool {
        if !self.value_neighbors {
            return !cancelled();
        }
        for span in [&self.width, &self.precision] {
            // Width and precision use Python-sized integers. A dynamic field
            // can participate in the number but never becomes part of an edit.
            if span.is_empty() || span.len() > 19 {
                continue;
            }
            let parts = &self.parts[span.clone()];
            let Some(range) = parts
                .iter()
                .try_fold(None, |range: Option<TextRange>, part| {
                    let next = part.source()?;
                    if range.is_some_and(|previous| previous.end() != next.start()) {
                        return None;
                    }
                    Some(Some(range.map_or(next, |previous| previous.cover(next))))
                })
                .flatten()
            else {
                continue;
            };
            let digits: String = parts.iter().filter_map(|part| part.spelling()).collect();
            let Ok(value) = digits.parse::<isize>() else {
                continue;
            };
            for neighbor in [
                value.checked_sub(1).filter(|&value| value >= 0),
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

    pub(super) fn emit_options(
        &self,
        field: &InterpolatedElement,
        cancelled: &dyn Fn() -> bool,
        mut emit: impl FnMut(TextRange, String) -> bool,
    ) -> bool {
        let subject = Subject::of(field);
        let has_precision = !self.precision.is_empty();
        if !self.options.valid(subject, has_precision) {
            return !cancelled();
        }
        let presentation = self.options.presentation.map(|part| part.ch);
        let type_choices = match presentation {
            Some(ch) if integer_type(ch) => INTEGER_TYPES,
            Some('n') if subject == Subject::Integer => INTEGER_TYPES,
            Some('n')
                if has_precision
                    || self.options.negative_zero
                    || matches!(subject, Subject::Float | Subject::Complex) =>
            {
                FLOAT_TYPES
            }
            Some(ch) if ch != 'n' && FLOAT_TYPES.contains(ch) => {
                if matches!(subject, Subject::Float | Subject::Complex)
                    || !has_precision && !self.options.negative_zero
                {
                    FLOAT_TYPES
                } else {
                    "eEfFgG%"
                }
            }
            _ => "",
        };
        let align_choices = if matches!(subject, Subject::Integer | Subject::Float)
            || presentation.is_some_and(integer_type)
        {
            ALIGNMENTS
        } else {
            "<>^"
        };
        let mut swap =
            |site: Option<OptionSite>, choices: &str, adjust: fn(&mut Options, OptionSite)| {
                let Some(OptionSite {
                    ch,
                    range: Some(range),
                }) = site
                else {
                    return true;
                };
                for replacement in choices.chars().filter(|&replacement| replacement != ch) {
                    let mut options = self.options;
                    adjust(
                        &mut options,
                        OptionSite {
                            ch: replacement,
                            range: Some(range),
                        },
                    );
                    if cancelled()
                        || (options.valid(subject, has_precision)
                            && !emit(range, replacement.to_string()))
                    {
                        return false;
                    }
                }
                true
            };
        if !swap(self.options.alignment, align_choices, |opts, site| {
            opts.alignment = Some(site);
        }) || !swap(self.options.sign, SIGNS, |opts, site| {
            opts.sign = Some(site);
        }) || !swap(self.options.grouping, GROUPINGS, |opts, site| {
            opts.grouping = Some(site);
        }) || !swap(self.options.fractional_grouping, GROUPINGS, |opts, site| {
            opts.fractional_grouping = Some(site);
        }) || !swap(self.options.presentation, type_choices, |opts, site| {
            opts.presentation = Some(site);
        }) {
            return false;
        }
        if presentation == Some('n')
            && let Some(range) = self.options.presentation.and_then(|part| part.range)
            && !cancelled()
        {
            return emit(range, String::new());
        }
        !cancelled()
    }
}

fn append_literal(
    parts: &mut Vec<Part>,
    text: &str,
    decoded: &str,
    start: TextSize,
    raw: bool,
    cancelled: &dyn Fn() -> bool,
) -> Option<()> {
    let original_len = parts.len();
    if map_literal(parts, text, decoded, start, raw, cancelled).is_none() {
        parts.truncate(original_len);
        // Ruff provides the decoded format text even for a source spelling we
        // cannot map. It can still disambiguate options in adjacent segments.
        for ch in decoded.chars() {
            if cancelled() {
                return None;
            }
            parts.push(Part::Literal(ch, None));
        }
    }
    (!cancelled()).then_some(())
}

fn map_literal(
    parts: &mut Vec<Part>,
    text: &str,
    decoded: &str,
    start: TextSize,
    raw: bool,
    cancelled: &dyn Fn() -> bool,
) -> Option<()> {
    let exact = text == decoded;
    let mut expected = decoded.chars();
    let mut at = 0;
    while at < text.len() {
        if cancelled() {
            return None;
        }
        let from = at;
        let mut ch = text[at..].chars().next()?;
        at += ch.len_utf8();
        if !exact && ch == '\r' {
            at += usize::from(text.as_bytes().get(at) == Some(&b'\n'));
            ch = '\n';
        }
        if !exact && !raw && ch == '\\' {
            match decode(text, &mut at, false, unicode_names2::character)? {
                Decoded::Character(character) => ch = character,
                Decoded::Continuation => continue,
                Decoded::Preserved(character) => {
                    if expected.next() != Some(ch) {
                        return None;
                    }
                    parts.push(Part::Literal(
                        ch,
                        Some(literal_range(start, from..from + 1)),
                    ));
                    if expected.next() != Some(character) {
                        return None;
                    }
                    parts.push(Part::Literal(
                        character,
                        Some(literal_range(start, from + 1..at)),
                    ));
                    continue;
                }
            }
        }
        if expected.next() != Some(ch) {
            return None;
        }
        parts.push(Part::Literal(ch, Some(literal_range(start, from..at))));
    }
    expected.next().is_none().then_some(())
}

fn literal_range(start: TextSize, offsets: Range<usize>) -> TextRange {
    TextRange::new(
        start + TextSize::try_from(offsets.start).expect("literal offset"),
        start + TextSize::try_from(offsets.end).expect("literal offset"),
    )
}

struct Parser<'a> {
    parts: &'a [Part],
    at: usize,
    cancelled: &'a dyn Fn() -> bool,
}
impl Parser<'_> {
    fn option(&mut self, choices: &str) -> Option<OptionSite> {
        let part = *self.parts.get(self.at)?;
        let ch = part.spelling()?;
        if !choices.contains(ch) {
            return None;
        }
        self.at += 1;
        Some(OptionSite {
            ch,
            range: part.source(),
        })
    }

    fn digits(&mut self) -> Option<Range<usize>> {
        let start = self.at;
        while let Some(part) = self.parts.get(self.at) {
            if (self.cancelled)() {
                return None;
            }
            if !matches!(part, Part::Field | Part::Literal('0'..='9', _)) {
                break;
            }
            self.at += 1;
        }
        Some(start..self.at)
    }
}
