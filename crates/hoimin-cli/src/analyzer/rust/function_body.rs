//! Eligibility and source span for the initial void-function erasure operator.
use ruff_python_ast::{
    Expr, Stmt, StmtFunctionDef,
    visitor::{self, Visitor},
};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn erased_range<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    cancelled: &F,
) -> Option<TextRange> {
    let name = definition.name.as_str();
    if definition.is_async || (name.starts_with("__") && name.ends_with("__")) || cancelled() {
        return None;
    }
    let body = definition.body.as_slice();
    let body = if body.first().is_some_and(is_docstring) {
        &body[1..]
    } else {
        body
    };
    let first = body.first()?;
    let mut check = OwnScope {
        safe: true,
        cancelled,
    };
    let mut meaningful = false;
    for statement in body {
        if cancelled() {
            return None;
        }
        meaningful |= !is_noop(statement);
        check.visit_stmt(statement);
        if !check.safe {
            return None;
        }
    }
    meaningful.then(|| TextRange::new(first.start(), body.last().expect("nonempty body").end()))
}

fn is_docstring(statement: &Stmt) -> bool {
    matches!(statement, Stmt::Expr(value) if matches!(value.value.as_ref(), Expr::StringLiteral(_)))
}
fn is_noop(statement: &Stmt) -> bool {
    match statement {
        Stmt::Pass(_) => true,
        Stmt::Return(value) => value
            .value
            .as_deref()
            .is_none_or(|value| matches!(value, Expr::NoneLiteral(_))),
        Stmt::Expr(value) => matches!(
            value.value.as_ref(),
            Expr::StringLiteral(_) | Expr::NoneLiteral(_) | Expr::EllipsisLiteral(_)
        ),
        _ => false,
    }
}

struct OwnScope<'a, F> {
    safe: bool,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for OwnScope<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if !self.safe || (self.cancelled)() {
            self.safe = false;
            return;
        }
        match statement {
            Stmt::FunctionDef(definition) => {
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                self.visit_parameters(&definition.parameters);
            }
            Stmt::ClassDef(definition) => {
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &definition.arguments {
                    self.visit_arguments(arguments);
                }
            }
            Stmt::TypeAlias(_) => {}
            Stmt::Return(value)
                if value
                    .value
                    .as_deref()
                    .is_some_and(|value| !matches!(value, Expr::NoneLiteral(_))) =>
            {
                self.safe = false;
            }
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_annotation(&mut self, _: &'ast Expr) {}
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if !self.safe || (self.cancelled)() {
            self.safe = false;
            return;
        }
        match expression {
            Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_) => self.safe = false,
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
            }
            _ => visitor::walk_expr(self, expression),
        }
    }
}
