//! Collection edits preserve iterator construction and affect one predicate,
//! set method, or aggregate at a time. Admission happens before source copying.

use super::{
    AnalysisCancelled,
    callable_imports::CallableImports,
    local_initializers,
    processing_calls::{Site, bind, implicit_generator_interior},
};
use hoimin_core::MutationOperator;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{CmpOp, Comprehension, Expr, ExprCall, ModModule, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};

const REMOVE: MutationOperator = MutationOperator::ComprehensionFilterRemove;
const INVERT: MutationOperator = MutationOperator::ComprehensionFilterInvert;
const FILTER: MutationOperator = MutationOperator::FilterPredicateSwap;
const SET: MutationOperator = MutationOperator::SetUnionIntersectionSwap;
const COUNT: MutationOperator = MutationOperator::AggregateSumCount;

#[derive(Default)]
pub(super) struct Collections {
    itertools: CallableImports,
    exact_sets: local_initializers::Index<()>,
    named_expressions: Vec<TextSize>,
    yields: Vec<TextSize>,
    comprehensions: bool,
    calls: bool,
}

impl Collections {
    pub(super) fn build(
        module: &ModModule,
        site: &Site<'_>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let calls = [FILTER, SET, COUNT].into_iter().any(|op| site.supports(op));
        let exact_sets = if site.supports(SET) {
            local_initializers::Index::build(module, cancelled, |value, is_context| {
                (!is_context && exact_set(value, site)).then_some(())
            })?
        } else {
            local_initializers::Index::default()
        };
        let itertools = if site.supports(FILTER) {
            CallableImports::build_family(module, cancelled, &["itertools"])?
        } else {
            CallableImports::default()
        };
        let mut collections = Self {
            itertools,
            exact_sets,
            comprehensions: [REMOVE, INVERT].into_iter().any(|op| site.supports(op)),
            calls,
            ..Self::default()
        };
        if site.supports(REMOVE) || site.supports(COUNT) {
            for token in site.tokens() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                match token.kind() {
                    TokenKind::ColonEqual => collections.named_expressions.push(token.start()),
                    TokenKind::Yield if site.supports(COUNT) => {
                        collections.yields.push(token.start());
                    }
                    _ => {}
                }
            }
        }
        Ok(collections)
    }

    pub(super) fn visits_comprehensions(&self) -> bool {
        self.comprehensions
    }

    pub(super) fn visits_calls(&self) -> bool {
        self.calls
    }

    pub(super) fn emit_filters(
        &self,
        clause: &Comprehension,
        site: &Site<'_>,
        mut selected: impl FnMut(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        for condition in &clause.ifs {
            if (site.cancelled)() {
                return;
            }
            let range = condition.range();
            if condition.is_boolean_literal_expr() || !selected(range) {
                continue;
            }
            let (before, after) = filter_spacing(range, site);
            if site.supports(REMOVE)
                && !contains(&self.named_expressions, range)
                && !emit(range, format!("{before}True{after}"), REMOVE)
            {
                return;
            }
            if site.supports(INVERT)
                && !duplicate_inversion(condition)
                && site.delimiters.can_enclose(range, 1)
                && let Some(text) = site.text(range)
                && ((site.cancelled)() || !emit(range, format!("{before}not ({text})"), INVERT))
            {
                return;
            }
        }
    }

    pub(super) fn emit_call(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        mut selected: impl FnMut(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        let callee = call.func.range();
        if site.supports(FILTER)
            && selected(callee)
            && bind(call, ["function", "iterable"], 2, 2, 2).is_some()
            && let Some(replacement) = self.filter_replacement(call, site)
            && ((site.cancelled)() || !emit(callee, replacement, FILTER))
        {
            return;
        }
        if site.supports(SET)
            && let Some(attribute) = call.func.as_attribute_expr()
            && let Some(replacement) = match attribute.attr.as_str() {
                "union" => Some("intersection"),
                "intersection" => Some("union"),
                _ => None,
            }
            && selected(attribute.attr.range())
            && (exact_set(&attribute.value, site)
                || attribute
                    .value
                    .as_name_expr()
                    .is_some_and(|name| self.exact_sets.get(name).is_some()))
            && set_operands(call, site)
            && ((site.cancelled)() || !emit(attribute.attr.range(), replacement.to_owned(), SET))
        {
            return;
        }
        if site.supports(COUNT)
            && let Some([Some(iterable)]) = bind(call, ["iterable"], 1, 1, 1)
        {
            if site.builtins.resolves(&call.func, "sum", site) {
                self.emit_count(call, iterable, site, selected, emit);
            } else if selected(callee)
                && site.builtins.resolves(&call.func, "len", site)
                && let Some(replacement) = site.builtins.destination("sum", &call.func, site)
                && numeric_sequence(iterable, site)
                && !(site.cancelled)()
            {
                emit(callee, replacement, COUNT);
            }
        }
    }

    fn filter_replacement(&self, call: &ExprCall, site: &Site<'_>) -> Option<String> {
        if site.builtins.resolves(&call.func, "filter", site) {
            site.builtins.import_member(
                &self.itertools,
                "itertools",
                "filterfalse",
                &call.func,
                site,
            )
        } else if self
            .itertools
            .resolve(&call.func)
            .is_some_and(|callable| callable.member == "filterfalse")
        {
            site.builtins.destination("filter", &call.func, site)
        } else {
            None
        }
    }

    fn emit_count(
        &self,
        call: &ExprCall,
        iterable: &Expr,
        site: &Site<'_>,
        mut selected: impl FnMut(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        let args = call.arguments.range();
        let range = implicit_generator_interior(call)
            .unwrap_or_else(|| site.argument_range(iterable, call));
        if !selected(range)
            || contains(&self.named_expressions, range)
            || contains(&self.yields, range)
        {
            return;
        }
        let trailing_comma = site
            .tokens()
            .in_range(TextRange::new(range.end(), args.end()))
            .iter()
            .any(|token| token.kind() == TokenKind::Comma);
        if site
            .delimiters
            .can_enclose(range, 1 + usize::from(trailing_comma))
            && let Some(text) = site.text(range)
            && !(site.cancelled)()
        {
            let counted = if trailing_comma {
                format!("(1 for _ in ({text}))")
            } else {
                format!("1 for _ in ({text})")
            };
            emit(range, counted, COUNT);
        }
    }
}

fn contains(offsets: &[TextSize], range: TextRange) -> bool {
    offsets
        .get(offsets.partition_point(|offset| *offset < range.start()))
        .is_some_and(|offset| *offset < range.end())
}

// Python permits adjacent delimiters and clause keywords. A replacement
// starting or ending in an identifier needs a separator at those boundaries.
fn filter_spacing(range: TextRange, site: &Site<'_>) -> (&'static str, &'static str) {
    let tokens = site.tokens();
    let before = tokens
        .before(range.start())
        .last()
        .is_some_and(|token| token.kind() == TokenKind::If && token.end() == range.start());
    let after = tokens.after(range.end()).first().is_some_and(|token| {
        matches!(
            token.kind(),
            TokenKind::If | TokenKind::For | TokenKind::Async
        ) && token.start() == range.end()
    });
    (if before { " " } else { "" }, if after { " " } else { "" })
}

fn duplicate_inversion(condition: &Expr) -> bool {
    matches!(condition, Expr::UnaryOp(unary) if unary.op == UnaryOp::Not)
        || matches!(condition, Expr::Compare(compare)
            if matches!(compare.ops.as_ref(), [CmpOp::Is | CmpOp::IsNot | CmpOp::In | CmpOp::NotIn]))
}

fn exact_set(expr: &Expr, site: &Site<'_>) -> bool {
    match expr {
        Expr::Set(_) | Expr::SetComp(_) => true,
        Expr::Call(call) => {
            bind(call, ["iterable"], 1, 1, 0).is_some()
                && (site.builtins.resolves(&call.func, "set", site)
                    || site.builtins.resolves(&call.func, "frozenset", site))
        }
        _ => false,
    }
}

fn set_operands(call: &ExprCall, site: &Site<'_>) -> bool {
    call.arguments.keywords.is_empty()
        && !call.arguments.args.is_empty()
        && call
            .arguments
            .args
            .iter()
            .all(|value| !(site.cancelled)() && !value.is_starred_expr())
}

fn numeric_sequence(iterable: &Expr, site: &Site<'_>) -> bool {
    let values = match iterable {
        Expr::Tuple(tuple) => tuple.elts.as_slice(),
        Expr::List(list) => list.elts.as_slice(),
        _ => return false,
    };
    values.iter().all(|value| {
        !(site.cancelled)()
            && (value.is_number_literal_expr()
                || value.is_boolean_literal_expr()
                || matches!(value, Expr::UnaryOp(unary)
                    if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub)
                        && unary.operand.is_number_literal_expr()))
    })
}
