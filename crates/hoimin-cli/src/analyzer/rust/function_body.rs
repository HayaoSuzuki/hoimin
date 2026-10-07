//! Own-scope eligibility and source spans for whole-function body mutations.
use ruff_python_ast::{
    Expr, Number, Stmt, StmtFunctionDef,
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
        reject_value_return: true,
        has_value_return: false,
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

pub(super) fn plain_synchronous<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    cancelled: &F,
) -> bool {
    if definition.is_async || cancelled() {
        return false;
    }
    let mut check = OwnScope {
        safe: true,
        reject_value_return: false,
        has_value_return: false,
        cancelled,
    };
    for statement in &definition.body {
        check.visit_stmt(statement);
        if !check.safe {
            return false;
        }
    }
    !cancelled()
}

/// The caller establishes that the annotation names a stable builtin.
pub(super) fn constant_returns<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    annotation: &str,
    cancelled: &F,
) -> Option<(TextRange, Vec<&'static str>)> {
    let name = definition.name.as_str();
    if definition.is_async || (name.starts_with("__") && name.ends_with("__")) || cancelled() {
        return None;
    }
    let mut replacements = match annotation {
        "bool" => vec!["return False", "return True"],
        "int" => vec!["return 0", "return 1"],
        "str" => vec!["return \"\"", "return \"A\""],
        _ => return None,
    };
    let body = definition.body.as_slice();
    let body = if body.first().is_some_and(is_docstring) {
        &body[1..]
    } else {
        body
    };
    let first = body.first()?;
    let mut check = OwnScope {
        safe: true,
        reject_value_return: false,
        has_value_return: false,
        cancelled,
    };
    for statement in body {
        check.visit_stmt(statement);
        if !check.safe {
            return None;
        }
    }
    if !check.has_value_return || cancelled() {
        return None;
    }
    if let [Stmt::Return(statement)] = body
        && let Some(value) = statement.value.as_deref()
    {
        replacements.retain(|replacement| !same_literal(value, replacement));
    }
    Some((
        TextRange::new(first.start(), body.last()?.end()),
        replacements,
    ))
}

fn same_literal(value: &Expr, replacement: &str) -> bool {
    match (value, replacement) {
        (Expr::BooleanLiteral(value), "return False") => !value.value,
        (Expr::BooleanLiteral(value), "return True") => value.value,
        (Expr::NumberLiteral(value), "return 0") => {
            matches!(&value.value, Number::Int(n) if n.as_u64() == Some(0))
        }
        (Expr::NumberLiteral(value), "return 1") => {
            matches!(&value.value, Number::Int(n) if n.as_u64() == Some(1))
        }
        (Expr::StringLiteral(value), "return \"\"") => value.value.to_str().is_empty(),
        (Expr::StringLiteral(value), "return \"A\"") => value.value.to_str() == "A",
        _ => false,
    }
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
    reject_value_return: bool,
    has_value_return: bool,
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
            Stmt::Return(value) => {
                let has_value = value
                    .value
                    .as_deref()
                    .is_some_and(|value| !matches!(value, Expr::NoneLiteral(_)));
                self.has_value_return |= has_value;
                if self.reject_value_return && has_value {
                    self.safe = false;
                } else {
                    visitor::walk_stmt(self, statement);
                }
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
