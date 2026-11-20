//! Callable and exact receiver proofs for Decimal rounding and context edits.

use super::{
    AnalysisCancelled,
    callable_imports::{CallableImports, ResolvedCallable},
    decimal_quantum::exact_integer,
    local_initializers,
    processing_calls::bind,
};
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, ModModule, Operator, Stmt, UnaryOp};
use ruff_text_size::{Ranged, TextRange};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum Kind {
    #[default]
    Unknown,
    Decimal,
    Integer,
    Context,
}

#[derive(Default)]
pub(super) struct Decimals {
    pub(super) imports: CallableImports,
    bindings: local_initializers::Index<Kind>,
    types: RefCell<HashMap<TextRange, Kind>>,
    context_type_stable: bool,
}

impl Decimals {
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let imports = CallableImports::build_family(module, cancelled, &["decimal"])?;
        if !imports.has_trusted_bindings() {
            return Ok(Self {
                imports,
                ..Self::default()
            });
        }
        let mut stability = ContextStability {
            imports: &imports,
            cancelled,
            writes: false,
        };
        stability.visit_body(&module.body);
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let context_type_stable = !stability.writes;
        let mut decimals = Self {
            imports,
            context_type_stable,
            ..Self::default()
        };
        decimals.bindings = local_initializers::Index::build(module, cancelled, |expr, with| {
            if with {
                let call = expr.as_call_expr()?;
                decimals.localcontext_arguments(call).map(|_| Kind::Context)
            } else {
                match decimals.kind(expr, cancelled) {
                    kind @ (Kind::Decimal | Kind::Context) => Some(kind),
                    _ => None,
                }
            }
        })?;
        // The first pass classifies only closed initializers. It has no lexical
        // bindings, so later queries cannot reuse its unknown-name results.
        decimals.types.get_mut().clear();
        Ok(decimals)
    }

    fn direct<'a>(&'a self, call: &'a ExprCall) -> Option<ResolvedCallable<'a>> {
        self.imports.resolve(&call.func)
    }

    pub(super) fn decimal_argument<'a>(&self, call: &'a ExprCall) -> Option<&'a Expr> {
        let [value, _] = self.decimal_arguments(call)?;
        value
    }

    fn decimal_arguments<'a>(&self, call: &'a ExprCall) -> Option<[Option<&'a Expr>; 2]> {
        if self.direct(call)?.member != "Decimal" {
            return None;
        }
        bind(call, ["value", "context"], 0, 2, 0)
    }

    pub(super) fn quantize<'a>(
        &self,
        call: &'a ExprCall,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Quantize<'a>> {
        let attribute = quantize_method(call)?;
        let [Some(exp), rounding, _] = bind(call, ["exp", "rounding", "context"], 0, 3, 1)? else {
            return None;
        };
        (self.kind(&attribute.value, cancelled) == Kind::Decimal).then_some(Quantize {
            attribute,
            exp,
            rounding,
        })
    }

    pub(super) fn context_arguments<'a>(&self, call: &'a ExprCall) -> Option<ContextSettings<'a>> {
        let [precision, rounding, ..] = match self.direct(call)?.member {
            "Context" => bind(call, CONTEXT_FIELDS, 0, 8, 0)?,
            "localcontext" => return self.localcontext_arguments(call),
            _ => return None,
        };
        Some(ContextSettings {
            precision,
            rounding,
        })
    }

    fn localcontext_arguments<'a>(&self, call: &'a ExprCall) -> Option<ContextSettings<'a>> {
        if self.direct(call)?.member != "localcontext" {
            return None;
        }
        let [_, precision, rounding, ..] = bind(
            call,
            [
                "ctx", "prec", "rounding", "Emin", "Emax", "capitals", "clamp", "flags", "traps",
            ],
            0,
            1,
            0,
        )?;
        Some(ContextSettings {
            precision,
            rounding,
        })
    }

    pub(super) fn context_target(
        &self,
        target: &ExprAttribute,
        cancelled: &dyn Fn() -> bool,
    ) -> bool {
        self.kind(&target.value, cancelled) == Kind::Context
    }

    fn kind(&self, expr: &Expr, cancelled: &dyn Fn() -> bool) -> Kind {
        if cancelled() {
            return Kind::Unknown;
        }
        if let Some(found) = self.types.borrow().get(&expr.range()).copied() {
            return found;
        }
        let kind = match expr {
            Expr::Name(name) => return self.bindings.get(name).unwrap_or_default(),
            Expr::Call(call) => self.call_kind(call, cancelled),
            Expr::UnaryOp(unary) if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub) => {
                match self.kind(&unary.operand, cancelled) {
                    numeric @ (Kind::Decimal | Kind::Integer) => numeric,
                    _ => Kind::Unknown,
                }
            }
            Expr::BinOp(binary) if arithmetic(binary.op) => {
                match (
                    self.kind(&binary.left, cancelled),
                    self.kind(&binary.right, cancelled),
                ) {
                    (Kind::Decimal, Kind::Decimal | Kind::Integer)
                    | (Kind::Integer, Kind::Decimal) => Kind::Decimal,
                    _ => Kind::Unknown,
                }
            }
            _ if exact_integer(expr) => return Kind::Integer,
            _ => return Kind::Unknown,
        };
        self.types.borrow_mut().insert(expr.range(), kind);
        kind
    }

    fn call_kind(&self, call: &ExprCall, cancelled: &dyn Fn() -> bool) -> Kind {
        if let Some(source) = self.direct(call) {
            return match source.member {
                "Decimal" if self.decimal_arguments(call).is_some() => Kind::Decimal,
                "Context" if self.context_arguments(call).is_some() => Kind::Context,
                "getcontext" if self.context_type_stable && call.arguments.is_empty() => {
                    Kind::Context
                }
                _ => Kind::Unknown,
            };
        }
        if self.quantize(call, cancelled).is_some() {
            return Kind::Decimal;
        }
        // from_float explicitly constructs Decimal without relying on implicit
        // conversion and cannot dispatch to a subclass when the class is proven.
        if let Some(attribute) = call.func.as_attribute_expr()
            && attribute.attr.as_str() == "from_float"
            && self
                .imports
                .resolve(&attribute.value)
                .is_some_and(|resolved| resolved.member == "Decimal")
            && bind(call, ["f"], 1, 1, 1).is_some()
        {
            return Kind::Decimal;
        }
        Kind::Unknown
    }
}

const CONTEXT_FIELDS: [&str; 8] = [
    "prec", "rounding", "Emin", "Emax", "capitals", "clamp", "flags", "traps",
];

pub(super) struct Quantize<'a> {
    pub(super) attribute: &'a ExprAttribute,
    pub(super) exp: &'a Expr,
    pub(super) rounding: Option<&'a Expr>,
}

pub(super) struct ContextSettings<'a> {
    pub(super) precision: Option<&'a Expr>,
    pub(super) rounding: Option<&'a Expr>,
}

fn quantize_method(call: &ExprCall) -> Option<&ExprAttribute> {
    let attribute = call.func.as_attribute_expr()?;
    (attribute.attr.as_str() == "quantize").then_some(attribute)
}

pub(super) fn arithmetic(operator: Operator) -> bool {
    matches!(
        operator,
        Operator::Add
            | Operator::Sub
            | Operator::Mult
            | Operator::Div
            | Operator::FloorDiv
            | Operator::Mod
            | Operator::Pow
    )
}

struct ContextStability<'a, F> {
    imports: &'a CallableImports,
    cancelled: &'a F,
    writes: bool,
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for ContextStability<'_, F> {
    fn visit_body(&mut self, body: &'ast [Stmt]) {
        for statement in body {
            if self.writes || (self.cancelled)() {
                break;
            }
            self.visit_stmt(statement);
        }
    }

    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if !self.writes && !(self.cancelled)() {
            visitor::walk_stmt(self, statement);
        }
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.writes || (self.cancelled)() {
            return;
        }
        if self.imports.may_reference(expr, "decimal", "setcontext") {
            // setcontext accepts subclasses, which getcontext then returns.
            self.writes = true;
        } else {
            visitor::walk_expr(self, expr);
        }
    }
}
