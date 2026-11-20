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
use std::collections::HashSet;

const BOUND: MutationOperator = MutationOperator::RegexMatchBoundary;
const SET: MutationOperator = MutationOperator::RegexCharacterSet;
const MODE: MutationOperator = MutationOperator::RegexAsciiMode;

#[derive(Default)]
pub(super) struct Regex<'a> {
    imports: CallableImports,
    bindings: Option<local_initializers::Index<&'a Expr>>,
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

    pub fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
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
        let pattern = !target.compiled
            && (intersects(target.pattern.range())
                || site.supports(MODE) && target.flags.is_some_and(|f| intersects(f.range())));
        if !method && !pattern {
            return;
        }
        if [Some(target.pattern), target.subject]
            .into_iter()
            .flatten()
            .any(|v| {
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
            })
        {
            return;
        }
        let compiled = self.compile(target.pattern);
        let mut flags = Flags::read(target.flags, &self.imports, site.cancelled);
        if let Some((_, compile_flags)) = compiled {
            if flags.as_ref().is_some_and(|v| v.value != 0) {
                return;
            }
            flags = Flags::read(compile_flags, &self.imports, site.cancelled);
        }
        let value = self.literal(compiled.map_or(target.pattern, |(p, _)| p));
        if let (Some(p), Some(s)) = (value, target.subject.and_then(|s| self.literal(s)))
            && p.is_bytes_literal_expr() != s.is_bytes_literal_expr()
        {
            return;
        }
        let literal = value.and_then(|e| Literal::read(e, site.source, site.cancelled));
        if value.is_some() && literal.is_none() {
            return;
        }
        let parsed = if let (Some(mapped), Some(external)) = (&literal, &flags) {
            let Some(syntax) = Syntax::scan(
                &mapped.chars,
                mapped.bytes,
                external.value,
                if target.compiled {
                    "compile"
                } else {
                    target.method
                },
                site.cancelled,
            ) else {
                return;
            };
            Some(syntax)
        } else {
            None
        };
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
        if !pattern || compiled.is_some() {
            return;
        }
        if let (Some(literal), Some(syntax), Some(external)) = (literal, parsed, flags) {
            let pattern = PatternSite {
                literal,
                syntax,
                external,
                flag_expr: target.flags,
                reference: target
                    .pattern
                    .is_name_expr()
                    .then(|| target.pattern.range()),
            };
            pattern.emit(site, max, selected, emit);
        }
    }
}

struct PatternSite<'a> {
    literal: Literal<'a>,
    syntax: Syntax,
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
        let mut variants = [0usize; 2];
        let mut seen = HashSet::new();
        for edit in &self.syntax.edits {
            if (site.cancelled)() {
                return;
            }
            let op = if edit.boundary { BOUND } else { SET };
            let group = usize::from(edit.boundary);
            if !site.supports(op)
                || self.reference.is_some() && variants[group] >= max.saturating_add(1)
            {
                continue;
            }
            let Some(range) = self.literal.range_for(&edit.span) else {
                continue;
            };
            if !selected(self.reference.unwrap_or(range)) {
                continue;
            }
            if self.reference.is_some() && !seen.insert((edit.span.clone(), edit.variant, op)) {
                continue;
            }
            if let Some((range, text)) = self.literal.edit(edit.span.clone(), edit.variant) {
                let (target, text) = self.project(range, text);
                variants[group] += 1;
                if !emit(target, text, op) {
                    return;
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
