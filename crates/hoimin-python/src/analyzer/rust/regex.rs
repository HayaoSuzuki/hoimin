//! Trusted Python matching calls emit one source edit per method, assertion,
//! character-set atom or root mode. Pattern syntax and Python quoting remain private.
#[path = "regex/atoms.rs"]
mod atoms;
#[path = "regex/characters.rs"]
mod characters;
#[path = "regex/literal.rs"]
mod literal;
#[path = "regex/pattern.rs"]
mod pattern;

use super::{
    AnalysisCancelled,
    callable_imports::CallableImports,
    local_initializers,
    processing_calls::{Site, bind},
};
use hoimin_core::MutationOperator;
use literal::Literal;
use pattern::{ASCII, Syntax, UNICODE};
use ruff_python_ast::{Expr, ExprCall, ModModule, Number, Operator};
use ruff_text_size::{Ranged, TextRange};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

const BOUND: MutationOperator = MutationOperator::RegexMatchBoundary;
const SET: MutationOperator = MutationOperator::RegexCharacterSet;
const MODE: MutationOperator = MutationOperator::RegexAsciiMode;

#[derive(Default)]
pub(super) struct Regex<'a> {
    imports: CallableImports,
    bindings: Option<local_initializers::Index<&'a Expr>>,
    cache: PatternCache<'a>,
}

#[derive(Clone)]
struct Prepared<'a> {
    literal: Rc<Literal<'a>>,
    syntax: Rc<Syntax>,
}
type GrammarBySite<'a> = HashMap<(TextRange, u16, pattern::Consumer), Option<Prepared<'a>>>;
#[derive(Default)]
struct PatternCache<'a> {
    literals: RefCell<HashMap<TextRange, Option<Rc<Literal<'a>>>>>,
    parsed: RefCell<GrammarBySite<'a>>,
    // Every constructor has one argument site. Consumers can check its validity
    // without reevaluating its flag expression or traversing its source mapping.
    compiled_validity: RefCell<HashMap<TextRange, bool>>,
}
impl<'a> PatternCache<'a> {
    fn mapped(&self, expr: &Expr, shared: bool, site: &Site<'a>) -> Option<Rc<Literal<'a>>> {
        let cached = self.literals.borrow().get(&expr.range()).cloned();
        if let Some(known) = cached {
            known
        } else {
            let mapped = Literal::read(expr, site.source, site.cancelled).map(Rc::new);
            if shared && !(site.cancelled)() {
                self.literals
                    .borrow_mut()
                    .insert(expr.range(), mapped.clone());
            }
            mapped
        }
    }
    fn prepare(
        &self,
        expr: &Expr,
        flags: u16,
        consumer: &str,
        shared: bool,
        site: &Site<'a>,
    ) -> Option<Prepared<'a>> {
        let context = pattern::Consumer::from_method(consumer);
        let key = (expr.range(), flags, context);
        if let Some(known) = self.parsed.borrow().get(&key) {
            return known.clone();
        }
        let mapped = self.mapped(expr, shared, site)?;
        let parsed = Syntax::scan(&mapped.chars, mapped.bytes, flags, context, site.cancelled).map(
            |syntax| Prepared {
                literal: mapped,
                syntax: Rc::new(syntax),
            },
        );
        if shared && !(site.cancelled)() {
            self.parsed.borrow_mut().insert(key, parsed.clone());
        }
        parsed
    }
}
struct Target<'a> {
    method: &'a str,
    member: Option<TextRange>,
    pattern: &'a Expr,
    flags: Option<&'a Expr>,
    subject: Option<&'a Expr>,
    compiled: bool,
}
fn is_literal(e: &Expr) -> bool {
    matches!(e, Expr::StringLiteral(_) | Expr::BytesLiteral(_))
}
fn methods(name: &str) -> &'static [&'static str] {
    match name {
        "match" | "prefixmatch" => &["fullmatch", "search"],
        "search" | "fullmatch" => &["match"],
        _ => &[],
    }
}
fn bit(expr: &Expr, imports: &CallableImports) -> Option<u16> {
    if let Some(n) = expr.as_number_literal_expr() {
        let Number::Int(n) = &n.value else {
            return None;
        };
        return u16::try_from(n.as_i64()?)
            .ok()
            .filter(|n| n & !pattern::MASK == 0);
    }
    Some(match imports.resolve(expr)?.member {
        "A" | "ASCII" => ASCII,
        "U" | "UNICODE" => UNICODE,
        "L" | "LOCALE" => 4,
        "I" | "IGNORECASE" => 2,
        "M" | "MULTILINE" => 8,
        "S" | "DOTALL" => 16,
        "X" | "VERBOSE" => 64,
        "NOFLAG" => 0,
        _ => return None,
    })
}
struct Flags<'a> {
    value: u16,
    mode_terms: Vec<(&'a Expr, u16)>,
}
impl<'a> Flags<'a> {
    fn read(
        expr: Option<&'a Expr>,
        imports: &CallableImports,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Self> {
        let mut result = Self {
            value: 0,
            mode_terms: vec![],
        };
        let mut stack = Vec::from_iter(expr);
        while let Some(e) = stack.pop() {
            if cancelled() {
                return None;
            }
            if let Expr::BinOp(op) = e
                && op.op == Operator::BitOr
            {
                stack.extend([&*op.right, &*op.left]);
                continue;
            }
            let n = bit(e, imports)?;
            result.value |= n;
            if n & (ASCII | UNICODE) != 0 {
                result.mode_terms.push((e, n));
            }
        }
        Some(result)
    }
    fn edit(&self, e: &Expr, site: &Site<'_>) -> Option<(TextRange, String)> {
        if self.mode_terms.is_empty() {
            return Some((e.range(), format!("{} | 256", site.text(e.range())?)));
        }
        let from = if self.value & ASCII != 0 {
            ASCII
        } else {
            UNICODE
        };
        let to = if from == ASCII { UNICODE } else { ASCII };
        let range = TextRange::new(
            self.mode_terms.first()?.0.start(),
            self.mode_terms.last()?.0.end(),
        );
        let mut cursor = usize::from(range.start());
        let mut out = String::new();
        for (term, n) in &self.mode_terms {
            out.push_str(&site.source[cursor..usize::from(term.start())]);
            out.push_str(&((n & !from) | to).to_string());
            cursor = usize::from(term.end());
        }
        Some((range, out))
    }
}

impl<'a> Regex<'a> {
    pub fn build(
        module: &'a ModModule,
        site: &Site<'_>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        if ![BOUND, SET, MODE].into_iter().any(|op| site.supports(op)) {
            return Ok(Self::default());
        }
        let imports = CallableImports::build_family(module, cancelled, &["re"])?;
        if !imports.has_trusted_bindings() {
            return Ok(Self::default());
        }
        let bindings = local_initializers::Index::build(module, cancelled, |e, with| {
            (!with
                && (is_literal(e)
                    || matches!(e, Expr::FString(_))
                    || e.as_call_expr().is_some_and(|c| {
                        imports
                            .resolve(&c.func)
                            .is_some_and(|m| m.member == "compile")
                    })))
            .then_some(e)
        })?;
        Ok(Self {
            imports,
            bindings: Some(bindings),
            cache: PatternCache::default(),
        })
    }
    pub fn active(&self) -> bool {
        self.bindings.is_some()
    }
    fn initializer(&self, e: &Expr) -> Option<&'a Expr> {
        self.bindings.as_ref()?.get(e.as_name_expr()?)
    }
    fn literal<'b>(&'b self, e: &'b Expr) -> Option<&'b Expr> {
        if is_literal(e) {
            Some(e)
        } else {
            self.initializer(e).filter(|v| is_literal(v))
        }
    }
    fn compile<'b>(&'b self, e: &'b Expr) -> Option<(&'b Expr, Option<&'b Expr>)> {
        let call = e
            .as_call_expr()
            .or_else(|| self.initializer(e)?.as_call_expr())?;
        if self.imports.resolve(&call.func)?.member != "compile" {
            return None;
        }
        let [Some(p), flags] = bind(call, ["pattern", "flags"], 0, 2, 1)? else {
            return None;
        };
        Some((p, flags))
    }
    fn target<'b>(&'b self, c: &'b ExprCall) -> Option<Target<'b>> {
        if let Some(m) = self.imports.resolve(&c.func) {
            if m.member == "compile" {
                let [Some(pattern), flags] = bind(c, ["pattern", "flags"], 0, 2, 1)? else {
                    return None;
                };
                return Some(Target {
                    method: m.member,
                    member: m.member_range,
                    pattern,
                    flags,
                    subject: None,
                    compiled: false,
                });
            }
            if methods(m.member).is_empty() {
                return None;
            }
            let [Some(pattern), subject, flags] = bind(c, ["pattern", "string", "flags"], 0, 3, 2)?
            else {
                return None;
            };
            return Some(Target {
                method: m.member,
                member: m.member_range,
                pattern,
                flags,
                subject,
                compiled: false,
            });
        }
        let attr = c.func.as_attribute_expr()?;
        if methods(attr.attr.as_str()).is_empty() {
            return None;
        }
        let [subject, _, _] = bind(c, ["string", "pos", "endpos"], 0, 3, 1)?;
        let (pattern, flags) = self.compile(&attr.value)?;
        Some(Target {
            method: attr.attr.as_str(),
            member: Some(attr.attr.range()),
            pattern,
            flags,
            subject,
            compiled: true,
        })
    }

    fn text_is_bytes(&self, expr: &Expr) -> Option<bool> {
        let value = self.initializer(expr).unwrap_or(expr);
        match value {
            Expr::BytesLiteral(_) => Some(true),
            Expr::StringLiteral(_) | Expr::FString(_) => Some(false),
            _ => None,
        }
    }

    fn compatible_types(&self, pattern: &Expr, subject: Option<&Expr>) -> bool {
        if [Some(pattern), subject].into_iter().flatten().any(|v| {
            matches!(
                v,
                Expr::NoneLiteral(_)
                    | Expr::NumberLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::List(_)
                    | Expr::Dict(_)
                    | Expr::Set(_)
                    | Expr::Tuple(_)
            )
        }) {
            return false;
        }
        if let (Some(pattern_type), Some(subject_type)) = (
            self.text_is_bytes(pattern),
            subject.and_then(|v| self.text_is_bytes(v)),
        ) && pattern_type != subject_type
        {
            return false;
        }
        true
    }
    fn compiled_valid(&self, pattern: &Expr, flags: Option<&Expr>, site: &Site<'a>) -> bool {
        if let Some(&valid) = self.cache.compiled_validity.borrow().get(&pattern.range()) {
            return valid;
        }
        let known = Flags::read(flags, &self.imports, site.cancelled);
        let valid = match (self.literal(pattern), known) {
            (Some(expr), Some(flags)) => self
                .cache
                .prepare(expr, flags.value, "compile", true, site)
                .is_some(),
            (Some(expr), None) => self.cache.mapped(expr, true, site).is_some(),
            (_, Some(flags)) => self
                .text_is_bytes(pattern)
                .is_none_or(|bytes| pattern::valid(flags.value, bytes)),
            _ => true,
        };
        if !(site.cancelled)() {
            self.cache
                .compiled_validity
                .borrow_mut()
                .insert(pattern.range(), valid);
        }
        valid
    }

    pub fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'a>,
        max: usize,
        selected: impl Fn(TextRange) -> bool,
        intersects: impl Fn(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        let Some(target) = self.target(call) else {
            return;
        };
        let method_range = target.member.unwrap_or_else(|| call.func.range());
        let method =
            site.supports(BOUND) && !methods(target.method).is_empty() && selected(method_range);
        let compiled_argument = self.compile(target.pattern);
        let is_compiled = target.compiled || compiled_argument.is_some();
        let inspect = !is_compiled
            && (intersects(target.pattern.range())
                || site.supports(MODE) && target.flags.is_some_and(|f| intersects(f.range())));
        if !method && !inspect {
            return;
        }
        let (pattern, compile_flags) = compiled_argument.unwrap_or((target.pattern, target.flags));
        if !self.compatible_types(pattern, target.subject) {
            return;
        }
        if compiled_argument.is_some()
            && Flags::read(target.flags, &self.imports, site.cancelled)
                .is_some_and(|f| f.value != 0)
        {
            return;
        }

        let mut pattern_site = None;
        if is_compiled {
            if !self.compiled_valid(pattern, compile_flags, site) {
                return;
            }
        } else if let Some(external) = Flags::read(target.flags, &self.imports, site.cancelled) {
            if self
                .text_is_bytes(pattern)
                .is_some_and(|bytes| !pattern::valid(external.value, bytes))
            {
                return;
            }
            if let Some(value) = self.literal(pattern) {
                let reference = pattern.is_name_expr().then(|| pattern.range());
                let prepared = self.cache.prepare(
                    value,
                    external.value,
                    target.method,
                    reference.is_some(),
                    site,
                );
                if target.method == "compile" && !(site.cancelled)() {
                    self.cache
                        .compiled_validity
                        .borrow_mut()
                        .insert(pattern.range(), prepared.is_some());
                }
                let Some(prepared) = prepared else { return };
                pattern_site = Some(PatternSite {
                    literal: prepared.literal,
                    syntax: prepared.syntax,
                    external,
                    flag_expr: target.flags,
                    reference,
                });
            }
        } else if let Some(value) = self.literal(pattern)
            && self
                .cache
                .mapped(value, pattern.is_name_expr(), site)
                .is_none()
        {
            return;
        }
        if method {
            for &dest in methods(target.method) {
                if (site.cancelled)() {
                    return;
                }
                let spelling = if target.member.is_some() {
                    Some(dest.to_owned())
                } else {
                    site.builtins
                        .import_member(&self.imports, "re", dest, &call.func, site)
                };
                if let Some(spelling) = spelling
                    && !emit(method_range, spelling, BOUND)
                {
                    return;
                }
            }
        }
        if inspect && let Some(pattern_site) = pattern_site {
            pattern_site.emit(site, max, selected, emit);
        }
    }
}

struct PatternSite<'a> {
    literal: Rc<Literal<'a>>,
    syntax: Rc<Syntax>,
    external: Flags<'a>,
    flag_expr: Option<&'a Expr>,
    reference: Option<TextRange>,
}
impl PatternSite<'_> {
    fn emit(
        &self,
        site: &Site<'_>,
        max: usize,
        selected: impl Fn(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        for (op, edits) in [(SET, &self.syntax.sets), (BOUND, &self.syntax.anchors)] {
            if !site.supports(op) {
                continue;
            }
            let mut variants = 0;
            let mut seen = HashSet::new();
            for edit in edits {
                if (site.cancelled)() {
                    return;
                }
                if self.reference.is_some() && variants >= max.saturating_add(1) {
                    break;
                }
                let Some(range) = self.literal.range_for(&edit.span) else {
                    continue;
                };
                if !selected(self.reference.unwrap_or(range)) {
                    continue;
                }
                if self.reference.is_some() && !seen.insert((edit.span.clone(), edit.variant)) {
                    continue;
                }
                if let Some((range, text)) = self.literal.edit(edit.span.clone(), edit.variant) {
                    let (target, text) = self.project(range, text);
                    variants += 1;
                    if !emit(target, text, op) {
                        return;
                    }
                }
            }
        }
        if !site.supports(MODE)
            || !self.syntax.sensitive
            || self.literal.bytes
            || (site.cancelled)()
        {
            return;
        }
        if !self.syntax.global_modes.is_empty() && self.external.value & (ASCII | UNICODE) != 0 {
            return;
        }
        if self.syntax.global_modes.is_empty()
            && let Some(expr) = self.flag_expr
        {
            if let Some((range, text)) = self.external.edit(expr, site)
                && selected(range)
            {
                emit(range, text, MODE);
            }
        } else if let Some((range, text)) = self.root_mode(site)
            && selected(self.reference.unwrap_or(range))
        {
            let (target, text) = self.project(range, text);
            emit(target, text, MODE);
        }
    }

    fn project(&self, range: TextRange, text: String) -> (TextRange, String) {
        if let Some(target) = self.reference {
            (target, self.literal.at_reference(range, &text))
        } else {
            (range, text)
        }
    }

    fn root_mode(&self, site: &Site<'_>) -> Option<(TextRange, String)> {
        let modes = &self.syntax.global_modes;
        let Some(&first) = modes.first() else {
            let range = self.literal.range_for(&(0..1))?;
            return Some((range, format!("(?a){}", site.text(range)?)));
        };
        let range = self.literal.range_for(&(first..modes.last()? + 1))?;
        let destination = if self.syntax.flags & ASCII != 0 {
            'u'
        } else {
            'a'
        };
        let mut cursor = usize::from(range.start());
        let mut text = String::new();
        for &position in modes {
            if (site.cancelled)() {
                return None;
            }
            let term = self.literal.range_for(&(position..position + 1))?;
            text.push_str(&site.source[cursor..usize::from(term.start())]);
            text.push(destination);
            cursor = usize::from(term.end());
        }
        Some((range, text))
    }
}
#[cfg(test)]
thread_local! {static MATERIALIZED:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
#[cfg(test)]
fn record_materialized(n: usize) {
    MATERIALIZED.set(MATERIALIZED.get() + n)
}
#[cfg(test)]
pub(super) fn reset_materialized() {
    MATERIALIZED.set(0)
}
#[cfg(test)]
pub(super) fn materialized() -> usize {
    MATERIALIZED.get()
}

#[cfg(test)]
thread_local! {static PATTERN_WORK:std::cell::Cell<(usize,usize)>=const{std::cell::Cell::new((0,0))};}
#[cfg(test)]
pub(super) fn clear_work() {
    PATTERN_WORK.set((0, 0));
}
#[cfg(test)]
pub(super) fn pattern_work() -> (usize, usize) {
    PATTERN_WORK.get()
}
#[cfg(test)]
fn record_work(decoded: usize, scanned: usize) {
    let (d, s) = pattern_work();
    PATTERN_WORK.set((d + decoded, s + scanned));
}
