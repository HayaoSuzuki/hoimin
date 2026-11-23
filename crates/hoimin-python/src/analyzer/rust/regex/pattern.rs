//! Validate the whole Python pattern before publishing any atom. Frames model
//! branch widths and scoped flags without recursive descent on untrusted text.
use super::atoms::{self, Escape};
use super::structure::ReferenceIndex;
use super::width::{Constraints, Interval, Model, Width};
use std::{cell::RefCell, collections::HashMap, ops::Range};

pub(super) const IGNORE: u16 = 2;
pub(super) const LOCALE: u16 = 4;
pub(super) const MULTI: u16 = 8;
pub(super) const UNICODE: u16 = 32;
pub(super) const VERBOSE: u16 = 64;
pub(super) const ASCII: u16 = 256;
const MODES: u16 = ASCII | UNICODE | LOCALE;
pub(super) const MASK: u16 = IGNORE | LOCALE | MULTI | 16 | UNICODE | VERBOSE | ASCII;

/// Anchor redundancy depends only on where the consumer permits the match to start and end.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) enum Consumer {
    Search,
    Prefix,
    Full,
}
impl Consumer {
    pub(super) fn from_method(method: &str) -> Self {
        match method {
            "fullmatch" => Self::Full,
            "match" | "prefixmatch" => Self::Prefix,
            _ => Self::Search,
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) enum Variant {
    Text(&'static str),
    Unicode(&'static str),
    Scope(char),
    Count(usize),
    MakeLazy,
    MakeGreedy,
    DeleteAlternative,
}
#[derive(Clone, Copy, Default)]
pub(super) struct Behavior {
    pub counts: bool,
    pub preferences: bool,
    pub alternatives: bool,
}
pub(super) struct Edit {
    pub span: Range<usize>,
    pub variant: Variant,
}
struct Anchor {
    span: Range<usize>,
    ch: char,
    multi: bool,
    root: bool,
}
pub(super) struct Syntax {
    pub sets: Vec<Edit>,
    pub anchors: Vec<Edit>,
    pub counts: Vec<Edit>,
    pub preferences: Vec<Edit>,
    pub alternatives: Vec<Edit>,
    pub sensitive: bool,
    pub global_modes: Vec<usize>,
    pub flags: u16,
    count_choices: Vec<CountChoice>,
    widths: RefCell<Constraints>,
}

struct CountChoice {
    count: Count,
    before: Width,
    after: Interval,
}

#[derive(Clone, Copy)]
enum Group {
    Root,
    Ordinary,
    Capture(usize),
    Assertion(bool),
    Conditional,
}
struct Frame {
    group: Group,
    flags: u16,
    inherits: bool,
    behind: Option<usize>,
    may_fold: bool,
    previous: Width,
    last: Option<Width>,
    repeatable: bool,
    alternatives: Option<Width>,
    count: usize,
    body_start: usize,
    bars: Vec<usize>,
}
impl Frame {
    fn new(
        group: Group,
        flags: u16,
        inherits: bool,
        behind: Option<usize>,
        body_start: usize,
    ) -> Self {
        Self {
            group,
            flags,
            inherits,
            behind,
            may_fold: false,
            previous: Width::default(),
            last: None,
            repeatable: false,
            alternatives: None,
            count: 0,
            body_start,
            bars: Vec::new(),
        }
    }
    fn push(&mut self, w: Width, repeatable: bool, widths: &mut Model) {
        if let Some(last) = self.last.replace(w) {
            self.previous = widths.add(self.previous, last);
        }
        self.repeatable = repeatable;
        self.count += 1;
    }
    fn width(&self, widths: &mut Model) -> Width {
        widths.add(self.previous, self.last.unwrap_or_default())
    }
    fn alternate(&mut self, bar: usize, collect: bool, widths: &mut Model) -> Option<()> {
        if matches!(self.group, Group::Conditional) && self.alternatives.is_some() {
            return None;
        }
        let w = self.width(widths);
        self.alternatives = Some(self.alternatives.map_or(w, |a| widths.branch(a, w)));
        self.previous = Width::default();
        self.last = None;
        self.repeatable = false;
        if collect && !matches!(self.group, Group::Conditional) {
            self.bars.push(bar);
        }
        Some(())
    }

    fn alternative_edits(&self, end: usize, edits: &mut Vec<Edit>) {
        let mut start = self.body_start;
        for &bar in &self.bars {
            edits.push(Edit {
                span: start..bar + 1,
                variant: Variant::DeleteAlternative,
            });
            start = bar + 1;
        }
        if let Some(&bar) = self.bars.last() {
            edits.push(Edit {
                span: bar..end,
                variant: Variant::DeleteAlternative,
            });
        }
    }
}

pub(super) fn flag(c: char) -> Option<u16> {
    Some(match c {
        'a' => ASCII,
        'L' => LOCALE,
        'u' => UNICODE,
        'i' => IGNORE,
        'm' => MULTI,
        's' => 16,
        'x' => VERBOSE,
        _ => return None,
    })
}
pub(super) fn valid(flags: u16, bytes: bool) -> bool {
    flags & !MASK == 0
        && (flags & MODES).count_ones() <= 1
        && if bytes {
            flags & UNICODE == 0
        } else {
            flags & LOCALE == 0
        }
}
fn flags(
    s: &[char],
    at: &mut usize,
    bytes: bool,
    cancelled: &dyn Fn() -> bool,
) -> Option<(u16, u16, bool, Vec<usize>)> {
    let mut add = 0;
    let mut del = 0;
    let mut minus = false;
    let mut mode_positions = vec![];
    loop {
        if cancelled() {
            return None;
        }
        let c = *s.get(*at)?;
        *at += 1;
        match c {
            ':' | ')'
                if (!minus || del != 0 && c == ':') && add & del == 0 && valid(add, bytes) =>
            {
                return Some((add, del, c == ')', mode_positions));
            }
            '-' if !minus => minus = true,
            c => {
                let f = flag(c)?;
                if minus {
                    if f & MODES != 0 {
                        return None;
                    }
                    del |= f;
                } else {
                    add |= f;
                    if f & MODES != 0 {
                        mode_positions.push(*at - 1);
                    }
                }
            }
        }
    }
}

// A malformed brace expression is a literal; a syntactically complete repeat
// whose number overflows CPython's SRE_CODE is an invalid pattern.
enum Repeat {
    Literal,
    Bounds(Count),
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum CountStyle {
    Shorthand,
    Exact,
    Range { implicit_min: bool },
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) struct Count {
    lo: u32,
    hi: u32,
    style: CountStyle,
}
impl Count {
    fn shorthand(lo: u32, hi: u32) -> Repeat {
        Repeat::Bounds(Self {
            lo,
            hi,
            style: CountStyle::Shorthand,
        })
    }

    fn neighbors(self) -> impl Iterator<Item = Self> {
        let bound = |value: u32| value.checked_add(1).filter(|&value| value < u32::MAX);
        let adjust = |lo, hi| Self { lo, hi, ..self };
        let exact = self.style == CountStyle::Exact;
        [
            self.lo
                .checked_sub(1)
                .map(|lo| adjust(lo, if exact { lo } else { self.hi })),
            bound(self.lo).map(|lo| adjust(lo, if exact { lo } else { self.hi })),
            (!exact).then_some(self.hi).and_then(|hi| {
                (hi < u32::MAX)
                    .then(|| hi.checked_sub(1))
                    .flatten()
                    .map(|hi| adjust(self.lo, hi))
            }),
            (!exact)
                .then(|| bound(self.hi))
                .flatten()
                .map(|hi| adjust(self.lo, hi)),
        ]
        .into_iter()
        .flatten()
        .filter(|count| count.lo <= count.hi)
    }

    pub(super) fn spelling(self) -> String {
        match (self.style, self.lo, self.hi) {
            (CountStyle::Shorthand, 0, u32::MAX) => "*".to_owned(),
            (CountStyle::Shorthand, 1, u32::MAX) => "+".to_owned(),
            (CountStyle::Shorthand, 0, 1) => "?".to_owned(),
            (CountStyle::Exact | CountStyle::Shorthand, lo, hi) if lo == hi => {
                format!("{{{lo}}}")
            }
            (style, lo, hi) => {
                let lo = if lo == 0 && style == (CountStyle::Range { implicit_min: true }) {
                    String::new()
                } else {
                    lo.to_string()
                };
                let hi = if hi == u32::MAX {
                    String::new()
                } else {
                    hi.to_string()
                };
                format!("{{{lo},{hi}}}")
            }
        }
    }
}
const MAX_DECIMAL_DIGITS: usize = 4300;
fn decimal(digits: impl ExactSizeIterator<Item = char>) -> Option<u32> {
    if digits.len() > MAX_DECIMAL_DIGITS {
        return None;
    }
    let mut digits = digits;
    digits
        .try_fold(0u32, |n, d| n.checked_mul(10)?.checked_add(d.to_digit(10)?))
        .filter(|&n| n < u32::MAX)
}
fn quant(s: &[char], at: &mut usize, cancelled: &dyn Fn() -> bool) -> Option<Repeat> {
    let start = *at;
    let mut end = start;
    while s.get(end).is_some_and(char::is_ascii_digit) {
        if cancelled() {
            return None;
        }
        end += 1;
    }
    let a = &s[start..end];
    let comma = s.get(end) == Some(&',');
    let mut b = a;
    if comma {
        end += 1;
        let start = end;
        while s.get(end).is_some_and(char::is_ascii_digit) {
            if cancelled() {
                return None;
            }
            end += 1;
        }
        b = &s[start..end];
    }
    if s.get(end) != Some(&'}') || (!comma && a.is_empty()) {
        return Some(Repeat::Literal);
    }
    let lo = decimal(a.iter().copied())?;
    let hi = if comma && b.is_empty() {
        u32::MAX
    } else {
        decimal(b.iter().copied())?
    };
    if lo > hi {
        return None;
    }
    *at = end + 1;
    Some(Repeat::Bounds(Count {
        lo,
        hi,
        style: if comma {
            CountStyle::Range {
                implicit_min: a.is_empty(),
            }
        } else {
            CountStyle::Exact
        },
    }))
}
fn identifier(s: &str, bytes: bool) -> bool {
    let mut cs = s.chars();
    (!bytes || s.is_ascii())
        && cs
            .next()
            .is_some_and(|c| c == '_' || unicode_ident::is_xid_start(c))
        && cs.all(unicode_ident::is_xid_continue)
        && s.chars().all(super::characters::assigned_in_both)
}

struct Scanner<'a> {
    source: &'a [char],
    at: usize,
    bytes: bool,
    cancelled: &'a dyn Fn() -> bool,
    stack: Vec<Frame>,
    captures: Vec<Option<(Width, bool)>>,
    names: HashMap<String, usize>,
    max_condition: usize,
    first: Option<usize>,
    last: Option<usize>,
    anchors: Vec<Anchor>,
    behavior: Behavior,
    references: ReferenceIndex,
    widths: Model,
    output: Syntax,
}

impl Syntax {
    pub fn scan(
        source: &[char],
        bytes: bool,
        external: u16,
        consumer: Consumer,
        behavior: Behavior,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Self> {
        #[cfg(test)]
        super::record_work(0, source.len());
        if !valid(external, bytes) {
            return None;
        }
        let mut parser = Scanner {
            source,
            at: 0,
            bytes,
            cancelled,
            stack: vec![Frame::new(Group::Root, external, true, None, 0)],
            captures: vec![None],
            names: HashMap::new(),
            max_condition: 0,
            first: None,
            last: None,
            anchors: vec![],
            behavior,
            references: ReferenceIndex::default(),
            widths: Model::default(),
            output: Self {
                sets: vec![],
                anchors: vec![],
                counts: vec![],
                preferences: vec![],
                alternatives: vec![],
                sensitive: false,
                global_modes: vec![],
                flags: external,
                count_choices: Vec::new(),
                widths: RefCell::default(),
            },
        };
        while parser.at < source.len() {
            if cancelled() {
                return None;
            }
            parser.step()?;
        }
        parser.finish(consumer)
    }

    /// Returns a spelling only when the edit preserves every dependent
    /// lookbehind's fixed width. The parsed model is shared between edits.
    pub(super) fn count_replacement(
        &self,
        index: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<String> {
        let choice = &self.count_choices[index];
        self.widths
            .borrow_mut()
            .permits(choice.before, choice.after, cancelled)
            .then(|| choice.count.spelling())
    }
}

impl Scanner<'_> {
    fn next(&mut self) -> Option<char> {
        let ch = *self.source.get(self.at)?;
        self.at += 1;
        Some(ch)
    }
    fn eat(&mut self, wanted: char) -> bool {
        if self.source.get(self.at) == Some(&wanted) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn comment(&mut self, terminal: char) -> Option<()> {
        while let Some(ch) = self.next() {
            if (self.cancelled)() {
                return None;
            }
            if ch == terminal {
                return Some(());
            }
            if ch == '\\' {
                self.next()?;
            }
        }
        (terminal == '\n').then_some(())
    }
    fn note_root_atom(&mut self, start: usize) {
        if self.stack.len() == 1 {
            self.first.get_or_insert(start);
            self.last = Some(start);
        }
    }
    fn step(&mut self) -> Option<()> {
        let start = self.at;
        let ch = self.next()?;
        if self.stack.last()?.flags & VERBOSE != 0 {
            if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c') {
                return Some(());
            }
            if ch == '#' {
                return self.comment('\n');
            }
        }
        match ch {
            '|' => {
                return self.stack.last_mut()?.alternate(
                    start,
                    self.behavior.alternatives,
                    &mut self.widths,
                );
            }
            ')' => return self.close(),
            '(' => return self.open(start),
            _ => {}
        }
        let repeat = match ch {
            '*' => Count::shorthand(0, u32::MAX),
            '+' => Count::shorthand(1, u32::MAX),
            '?' => Count::shorthand(0, 1),
            '{' => quant(self.source, &mut self.at, self.cancelled)?,
            _ => Repeat::Literal,
        };
        if let Repeat::Bounds(count) = repeat {
            let frame = self.stack.last_mut()?;
            if !frame.repeatable {
                return None;
            }
            let atom = frame.last?;
            let width = self
                .widths
                .repeat(atom, count.lo, count.hi, self.behavior.counts);
            frame.last = Some(width);
            frame.repeatable = false;
            let suffix = self.source.get(self.at).copied();
            let base = start..self.at;
            if matches!(suffix, Some('?' | '+')) {
                self.at += 1;
            }
            if self.behavior.counts {
                for replacement in count.neighbors() {
                    let index = self.output.count_choices.len();
                    self.output.count_choices.push(CountChoice {
                        count: replacement,
                        before: width,
                        after: atom.interval.repeat(replacement.lo, replacement.hi),
                    });
                    self.output.counts.push(Edit {
                        span: base.clone(),
                        variant: Variant::Count(index),
                    });
                }
            }
            if self.behavior.preferences && count.lo != count.hi {
                let preference = match suffix {
                    Some('?') => Some((base.end..self.at, Variant::MakeGreedy)),
                    Some('+') => None,
                    _ => Some((base, Variant::MakeLazy)),
                };
                if let Some((span, variant)) = preference {
                    self.output.preferences.push(Edit { span, variant });
                }
            }
            return Some(());
        }
        self.note_root_atom(start);
        self.atom(start, ch)
    }

    fn open(&mut self, start: usize) -> Option<()> {
        let parent = self.stack.last()?;
        let (mut group, mut effective, mut inherited, mut behind) = (
            Group::Ordinary,
            parent.flags,
            parent.inherits,
            parent.behind,
        );
        let mut capture = true;
        let mut group_name = None;
        if self.eat('?') {
            capture = false;
            match self.next()? {
                ':' | '>' => {}
                '#' => return self.comment(')'),
                'P' if self.eat('<') => {
                    group_name = Some(self.name('>')?);
                    capture = true;
                }
                'P' if self.eat('=') => {
                    let name = self.name(')')?;
                    let group = *self.names.get(&name)?;
                    self.note_root_atom(start);
                    return self.reference(group, start, false);
                }
                '=' | '!' => group = Group::Assertion(false),
                '<' if self.eat('=') || self.eat('!') => {
                    group = Group::Assertion(true);
                    behind = behind.or(Some(self.captures.len()));
                }
                '(' => {
                    let id = self.name(')')?;
                    let numeric = !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
                    let n = if numeric {
                        usize::try_from(decimal(id.bytes().map(char::from))?).ok()?
                    } else {
                        *self.names.get(&id)?
                    };
                    if n == 0
                        || behind.is_some_and(|lo| {
                            n >= lo || self.captures.get(n).copied().flatten().is_none()
                        })
                    {
                        return None;
                    }
                    self.max_condition = self.max_condition.max(n);
                    if self.behavior.alternatives {
                        self.references.record(start, n, numeric);
                    }
                    group = Group::Conditional;
                }
                option if option == '-' || flag(option).is_some() => {
                    self.at -= 1;
                    let (add, del, global, positions) =
                        flags(self.source, &mut self.at, self.bytes, self.cancelled)?;
                    if global {
                        let root = self.stack.len() == 1;
                        let frame = self.stack.last_mut()?;
                        if !root || frame.count != 0 || frame.alternatives.is_some() {
                            return None;
                        }
                        frame.flags |= add;
                        if !valid(frame.flags, self.bytes) {
                            return None;
                        }
                        self.output.flags = frame.flags;
                        self.output.global_modes.extend(positions);
                        frame.body_start = self.at;
                        return Some(());
                    }
                    if add & MODES != 0 {
                        effective &= !MODES;
                        inherited = false;
                    }
                    effective = (effective | add) & !del;
                }
                _ => return None,
            }
        }
        if capture {
            let id = self.captures.len();
            if let Some(name) = group_name
                && (!identifier(&name, self.bytes) || self.names.insert(name, id).is_some())
            {
                return None;
            }
            self.captures.push(None);
            if self.behavior.alternatives {
                self.references.capture(start);
            }
            group = Group::Capture(id);
        }
        self.note_root_atom(start);
        // Bound regex nesting separately from Python's lexical delimiter count.
        if self.stack.len() >= 256 {
            return None;
        }
        self.stack
            .push(Frame::new(group, effective, inherited, behind, self.at));
        Some(())
    }
    fn name(&mut self, terminal: char) -> Option<String> {
        let start = self.at;
        loop {
            let ch = self.next()?;
            if ch == terminal {
                break;
            }
            if (self.cancelled)() {
                return None;
            }
        }
        Some(self.source[start..self.at - 1].iter().collect())
    }
    fn close(&mut self) -> Option<()> {
        if self.stack.len() == 1 {
            return None;
        }
        let frame = self.stack.pop()?;
        if self.behavior.alternatives {
            frame.alternative_edits(self.at - 1, &mut self.output.alternatives);
        }
        let mut width = frame.width(&mut self.widths);
        if let Some(alt) = frame.alternatives {
            width = self.widths.branch(width, alt);
        }
        let mut may_fold = frame.may_fold;
        match frame.group {
            Group::Capture(id) => self.captures[id] = Some((width, may_fold)),
            Group::Assertion(behind) => {
                if behind {
                    self.widths.lookbehind(width)?;
                }
                width = Width::default();
                may_fold = false;
            }
            Group::Conditional if frame.alternatives.is_none() => {
                width = self.widths.branch(width, Width::default());
            }
            _ => {}
        }
        let parent = self.stack.last_mut()?;
        parent.may_fold |= may_fold;
        parent.push(width, true, &mut self.widths);
        Some(())
    }
    fn reference(&mut self, id: usize, start: usize, numeric: bool) -> Option<()> {
        let (width, may_fold) = self.captures.get(id).copied().flatten()?;
        let frame = self.stack.last_mut()?;
        if frame.behind.is_some_and(|lo| id >= lo) {
            return None;
        }
        frame.push(width, true, &mut self.widths);
        frame.may_fold |= may_fold;
        self.output.sensitive |=
            !self.bytes && frame.inherits && frame.flags & IGNORE != 0 && may_fold;
        if self.behavior.alternatives {
            self.references.record(start, id, numeric);
        }
        Some(())
    }
    fn atom(&mut self, start: usize, ch: char) -> Option<()> {
        let root = self.stack.len() == 1;
        let frame = self.stack.last_mut()?;
        let mut width = Width::ONE;
        let mut repeatable = true;
        let mut may_fold = ch == '.';
        let mut sensitive = false;
        let mut variant = None;
        let token = match ch {
            '\\' => Some(atoms::escape(self.source, &mut self.at, self.bytes, false)?),
            '^' | '$' => Some(Escape::Anchor(ch)),
            '[' => {
                let class = atoms::class(
                    self.source,
                    &mut self.at,
                    self.bytes,
                    frame.flags,
                    self.cancelled,
                )?;
                may_fold = class.may_fold;
                sensitive = class.sensitive;
                variant = class.variant;
                None
            }
            '.' => None,
            _ => Some(Escape::Literal(u32::from(ch))),
        };
        match token {
            Some(Escape::Anchor(code)) => {
                width = Width::default();
                repeatable = false;
                if matches!(code, 'b' | 'B') {
                    sensitive = !self.bytes;
                    if sensitive {
                        variant = Some(Variant::Scope(if frame.flags & ASCII != 0 {
                            'u'
                        } else {
                            'a'
                        }));
                    }
                } else {
                    self.anchors.push(Anchor {
                        span: start..self.at,
                        ch: code,
                        multi: frame.flags & MULTI != 0,
                        root,
                    });
                }
            }
            Some(Escape::Category(code)) if !self.bytes => {
                may_fold = !matches!(code, 'd' | 's');
                sensitive = true;
                variant = Some(atoms::category(code, frame.flags));
            }
            Some(Escape::Literal(code)) => {
                may_fold = atoms::fold_sensitive(code);
                sensitive = !self.bytes && frame.flags & IGNORE != 0 && may_fold;
            }
            Some(Escape::Ref(id)) => return self.reference(id, start, true),
            _ => {}
        }
        self.output.sensitive |= frame.inherits && sensitive;
        if let Some(variant) = variant {
            self.output.sets.push(Edit {
                span: start..self.at,
                variant,
            });
        }
        frame.may_fold |= may_fold;
        frame.push(width, repeatable, &mut self.widths);
        Some(())
    }
    fn finish(mut self, consumer: Consumer) -> Option<Syntax> {
        if self.stack.len() != 1 || self.max_condition >= self.captures.len() {
            return None;
        }
        if self.behavior.alternatives {
            self.stack[0].alternative_edits(self.at, &mut self.output.alternatives);
            self.references
                .retain_deletions(&mut self.output.alternatives, self.cancelled)?;
        }
        self.output.widths = RefCell::new(self.widths.finish(self.cancelled)?);
        for anchor in self.anchors {
            if (self.cancelled)() {
                return None;
            }
            let simple = anchor.root && self.stack[0].alternatives.is_none();
            let leading = simple
                && self.first == Some(anchor.span.start)
                && matches!(consumer, Consumer::Prefix | Consumer::Full);
            let terminal =
                simple && self.last == Some(anchor.span.start) && consumer == Consumer::Full;
            if (leading && matches!(anchor.ch, '^' | 'A'))
                || (terminal && matches!(anchor.ch, '$' | 'Z' | 'z'))
            {
                continue;
            }
            let alternate = match anchor.ch {
                '^' if anchor.multi => Some(r"\A"),
                'A' if anchor.multi => Some("^"),
                '$' => Some(r"\Z"),
                'Z' | 'z' => Some("$"),
                _ => None,
            };
            for replacement in [Some("(?:)"), alternate].into_iter().flatten() {
                self.output.anchors.push(Edit {
                    span: anchor.span.clone(),
                    variant: Variant::Text(replacement),
                });
            }
        }
        Some(self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[rstest::rstest]
    #[case::verbose_comment("(?x)#", "x", "\n\\d")]
    #[case::group_comment("(?#", "x", ")\\d")]
    #[case::bounded_repeat("a{", "0", "2}\\d")]
    #[case::capture_name("(?P<", "x", ">\\d)")]
    fn long_lexical_units_observe_cancellation(
        #[case] prefix: &str,
        #[case] repeated: &str,
        #[case] tail: &str,
    ) {
        let text = format!("{prefix}{}{tail}", repeated.repeat(2048))
            .chars()
            .collect::<Vec<_>>();
        let polls = std::cell::Cell::new(0);

        let result = Syntax::scan(
            &text,
            false,
            0,
            Consumer::Search,
            Behavior::default(),
            &|| {
                polls.set(polls.get() + 1);
                polls.get() >= 50
            },
        );

        assert!(result.is_none());
        assert_eq!(polls.get(), 50);
    }
}
